use std::convert::identity;
use std::io::{Error, Result};
use std::marker::PhantomData;
use std::mem::transmute;

use buffer::{AnyMessage, Buffer};
use futures::FutureExt;
use messages::{
    Authentication, BackendKeyData, BackendMessage, Bind, BindComplete, CommandComplete, DataRow,
    EmptyQueryResponse, ErrorResponse, Execute, NegotiateProtocolVersion, NoticeResponse,
    ParameterStatus, Parse, ParseComplete, ReadyForQuery, StartupMessage, Sync,
};
use params::QueryParams;
use result::{FromQueryResult, FromRow, QueryResult, Row};
use util::unexpected_message_prefix;

mod buffer;
mod impls;
mod messages;
pub mod params;
pub mod result;
mod util;

// todo: add a config for max connection buffer size, and implement a mechanism to shrink buffers than get too large.
pub struct Config {
    pub user: String,
    pub database: String,
    pub host: String,
}

#[trait_variant::make(TransportSend: Send)]
pub trait Transport: Sized {
    // todo: find a way to test multiple addresses
    async fn connect(config: &Config) -> Result<Self>;

    async fn read(&mut self, dst: &mut [u8]) -> Result<i32>;

    async fn write(&mut self, src: &[u8]) -> Result<i32>;
}

/// Marker trait for [`Transport`] implementations that can be used in synchronous contexts.
///
/// By implementing this trait on `T`, the implementer asserts that the implementation of
/// [`Transport`] on `T` does not use any `await`s. Failure to uphold this assertion
/// may result in [`panic`]s when using a [`Connection`] with this `T` in a
/// synchronous context.
pub trait SyncTransport: Transport {}

pub struct Connection<T> {
    buffer: Buffer<T>,
    ready_for_query: bool,
}

impl<T: Transport> Connection<T> {
    async fn receive_unhandled(&mut self) -> Result<AnyMessage> {
        loop {
            let msg = self.buffer.receive_any().await?;
            match msg.prefix {
                ErrorResponse::PREFIX => {
                    let err = self
                        .buffer
                        .parse::<ErrorResponse>(&msg)
                        .map_or_else(identity, Error::from);
                    self.buffer.clear();
                    return Err(err);
                }
                NegotiateProtocolVersion::PREFIX => {
                    let err = self
                        .buffer
                        .parse::<NegotiateProtocolVersion>(&msg)
                        .map_or_else(identity, Error::from);
                    self.buffer.clear();
                    return Err(err);
                }
                NoticeResponse::PREFIX | ParameterStatus::PREFIX => self.buffer.clear(),
                _ => return Ok(msg),
            }
        }
    }

    pub async fn connect(config: &Config) -> Result<Self> {
        let mut this = Self {
            buffer: Buffer::new(T::connect(config).await?),
            ready_for_query: true,
        };

        // Send StartupMessage
        this.buffer
            .send_all(|mut writer| {
                writer.add(StartupMessage {
                    user: &config.user,
                    database: &config.database,
                })
            })
            .await?;

        // Wait for Authenticate
        let msg = this.receive_unhandled().await?;
        match msg.prefix {
            Authentication::PREFIX => match this.buffer.parse(&msg)? {
                Authentication::Ok => (),
            },
            n => return Err(unexpected_message_prefix(n)),
        }

        // Wait for BackendKeyData
        let msg = this.receive_unhandled().await?;
        match msg.prefix {
            BackendKeyData::PREFIX => (),
            n => return Err(unexpected_message_prefix(n)),
        };

        // Wait for ReadyForQuery
        let msg = this.receive_unhandled().await?;
        match msg.prefix {
            ReadyForQuery::PREFIX => (),
            n => return Err(unexpected_message_prefix(n)),
        };

        Ok(this)
    }

    pub async fn query<'a, R, U>(
        &'a mut self,
        statement: &str,
        params: impl QueryParams,
    ) -> Result<U>
    where
        R: QueryResult,
        U: FromQueryResult<'a, R, T>,
    {
        self.recycle().await?;

        // Send Parse, Bind, Execute and Sync
        self.buffer
            .send_all(|mut writer| {
                writer.add(Parse { query: statement })?;
                writer.add(Bind { params })?;
                writer.add(Execute)?;
                writer.add(Sync)?;
                self.ready_for_query = false;
                Ok(())
            })
            .await?;

        // Waits for ParseComplete
        let msg = self.receive_unhandled().await?;
        match msg.prefix {
            ParseComplete::PREFIX => (),
            n => return Err(unexpected_message_prefix(n)),
        }

        // Waits for BindComplete
        let msg = self.receive_unhandled().await?;
        match msg.prefix {
            BindComplete::PREFIX => (),
            n => return Err(unexpected_message_prefix(n)),
        }

        U::from_stream(QueryStream::<R::Row, U::Row, T>::new(self)).await
    }

    pub async fn recycle(&mut self) -> Result<()> {
        if !self.ready_for_query {
            return Ok(());
        }

        loop {
            let msg = self.receive_unhandled().await?;
            match msg.prefix {
                ReadyForQuery::PREFIX => {
                    self.ready_for_query = true;
                    return Ok(());
                }
                CommandComplete::PREFIX | EmptyQueryResponse::PREFIX | DataRow::PREFIX => (),
                n => return Err(unexpected_message_prefix(n)),
            }
        }
    }

    pub fn connnect_sync(config: &Config) -> Result<Self>
    where
        T: SyncTransport,
    {
        Self::connect(config)
            .now_or_never()
            .expect("transport should be sync")
    }

    pub fn query_sync<'a, R, U>(
        &'a mut self,
        statement: &str,
        params: impl QueryParams,
    ) -> Result<U>
    where
        R: QueryResult,
        U: FromQueryResult<'a, R, T>,
        T: SyncTransport,
    {
        self.query(statement, params)
            .now_or_never()
            .expect("transport should be sync")
    }

    pub fn recycle_sync(&mut self) -> Result<()>
    where
        T: SyncTransport,
    {
        self.recycle()
            .now_or_never()
            .expect("transport should be sync")
    }
}

/// A stream of rows produced as a result of a query.
///
/// # Why is this not a regular [`Stream`](futures::stream::Stream)?
///
/// Values returned by [`QueryStream`] hold a reference to the buffer that
/// is inside the underlying [`Connection`]. This pattern in rust is
/// called a "lending iterator", or "streaming interator" and is not
/// well supported by the Rust ecosystem.
#[repr(transparent)]
pub struct QueryStream<R, U, T>(Connection<T>, PhantomData<(R, U)>);

impl<'a, R, U: FromRow<'a, R>, T: Transport> QueryStream<R, U, T> {
    fn new(stream: &mut Connection<T>) -> &mut Self {
        // SAFETY: `QueryStream<R, U, T>` is a `#[repr(transparent)]` wrapper over
        // a `Connection<T>`, it is therefore safe to transmute a mutable reference
        // of one into a mutable reference of the other.
        unsafe { transmute::<&mut Connection<T>, &mut QueryStream<R, U, T>>(stream) }
    }

    pub async fn next(&'a mut self) -> Result<Option<U>> {
        let msg = self.0.receive_unhandled().await?;
        match msg.prefix {
            CommandComplete::PREFIX | EmptyQueryResponse::PREFIX => Ok(None),
            DataRow::PREFIX => {
                let row = Row {
                    inner: self.0.buffer.parse(&msg)?,
                };
                U::from_row(row).map(Some)
            }
            n => Err(unexpected_message_prefix(n)),
        }
    }

    pub fn next_sync(&'a mut self) -> Result<Option<U>>
    where
        T: SyncTransport,
    {
        self.next()
            .now_or_never()
            .expect("transport should be sync")
    }
}

#[cfg(any(feature = "bb8", feature = "deadpool", feature = "r2d2"))]
pub struct ConnectionManager<T> {
    config: Config,
    _marker: std::marker::PhantomData<fn(T)>,
}
