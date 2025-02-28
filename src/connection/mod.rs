use std::convert::identity;
use std::io::{Error, Result};
use std::iter::FusedIterator;
use std::marker::PhantomData;
use std::mem::transmute;
use std::ops::Range;

use futures::FutureExt;
use messages::{
    Authentication, BackendKeyData, BackendMessage, Bind, BindComplete, CommandComplete, DataRow,
    EmptyQueryResponse, ErrorResponse, Execute, FrontendMessage, NegotiateProtocolVersion,
    NoticeResponse, ParameterStatus, Parse, ParseComplete, ReadyForQuery, StartupMessage, Sync,
};
use util::{read_i32, read_slice, unexpected_message_prefix};

use crate::types::{FromQueryResult, FromRow, QueryResult};

mod impls;
mod messages;
mod util;

pub struct Row<'a> {
    inner: DataRow<'a>,
}

impl<'a> Iterator for Row<'a> {
    type Item = Result<Option<&'a [u8]>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.inner.len == 0 {
            return None;
        }
        self.inner.len -= 1;
        Some(match read_i32(&mut self.inner.columns) {
            Ok(-1) => Ok(None),
            Ok(len) => read_slice(len, &mut self.inner.columns).map(Some),
            Err(err) => Err(err),
        })
    }
}

impl FusedIterator for Row<'_> {}

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

    async fn read_exact(&mut self, limit: i32, dst: &mut Vec<u8>) -> Result<()>;

    async fn write_all(&mut self, src: &[u8]) -> Result<()>;
}

/// Marker trait for [`Transport`] implementations that can be used in synchronous contexts.
///
/// By implementing this trait on `T`, the implementer asserts that the implementation of
/// [`Transport`] on `T` does not use any `await`s. Failure to uphold this assertion
/// may result in [`panic`]s when using a [`Connection`] with this `T` in a
/// synchronous context.
pub trait SyncTransport: Transport {}

struct MessageWriter<'a> {
    stack: &'a mut Vec<u8>,
}

impl MessageWriter<'_> {
    fn add<M: FrontendMessage>(mut self, msg: M) -> Result<Self> {
        if let Some(byte) = M::PREFIX {
            self.stack.push(byte);
        }

        let start = self.stack.len();
        self.stack.extend([0; 4]);
        msg.write(&mut self.stack)?;
        let size = (self.stack.len() - start) as i32;
        self.stack[start..start + 4].copy_from_slice(&size.to_be_bytes());

        Ok(self)
    }
}

struct AnyMessage {
    prefix: u8,
    range: Range<usize>,
}

struct BufferedTransport<T> {
    transport: T,
    stack: Vec<u8>,
}

impl<T: Transport> BufferedTransport<T> {
    async fn send(&mut self, f: impl FnOnce(MessageWriter) -> Result<()>) -> Result<()> {
        let start = self.stack.len();
        f(MessageWriter {
            stack: &mut self.stack,
        })
        .inspect_err(|_| self.stack.truncate(start))?;
        let res = self.transport.write_all(&self.stack).await;
        self.stack.truncate(start);
        res
    }

    async fn receive_any(&mut self) -> Result<AnyMessage> {
        loop {
            let start = self.stack.len();
            self.transport
                .read_exact(5, &mut self.stack)
                .await
                .inspect_err(|_| self.stack.truncate(start))?;
            let range = (start + 1)..(start + 5);
            let size = i32::from_be_bytes(self.stack[range.clone()].try_into().unwrap());
            self.transport
                .read_exact(size, &mut self.stack)
                .await
                .inspect_err(|_| self.stack.truncate(start))?;

            // todo: enhance error and warning reportings.
            match self.stack[start] {
                ErrorResponse::PREFIX => {
                    let err = ErrorResponse::read(&mut self.stack.as_slice())
                        .map_or_else(identity, Error::from);
                    self.stack.truncate(start);
                    return Err(err);
                }
                NegotiateProtocolVersion::PREFIX => {
                    let err = NegotiateProtocolVersion::read(&mut self.stack.as_slice())
                        .map_or_else(identity, Error::from);
                    self.stack.truncate(start);
                    return Err(err);
                }
                NoticeResponse::PREFIX | ParameterStatus::PREFIX => self.stack.truncate(start),
                prefix => return Ok(AnyMessage { prefix, range }),
            }
        }
    }

    fn read_message<'a, M: BackendMessage<'a>>(&'a self, msg: &AnyMessage) -> Result<M> {
        M::read(&mut &self.stack[msg.range.clone()])
    }
}

pub struct Connection<T> {
    transport: BufferedTransport<T>,
    ready: bool,
}

impl<T: Transport> Connection<T> {
    pub async fn connect(config: &Config) -> Result<Self> {
        let mut transport = BufferedTransport {
            transport: T::connect(config).await?,
            stack: Vec::new(),
        };

        // Send StartupMessage
        transport
            .send(|writer| {
                writer.add(StartupMessage {
                    user: &config.user,
                    database: &config.database,
                })?;
                Ok(())
            })
            .await?;

        // Wait for Authenticate
        let msg = transport.receive_any().await?;
        match msg.prefix {
            Authentication::PREFIX => match transport.read_message::<Authentication>(&msg)? {
                Authentication::Ok => (),
            },
            n => return Err(unexpected_message_prefix(n)),
        }

        // Wait for BackendKeyData
        let msg = transport.receive_any().await?;
        match msg.prefix {
            BackendKeyData::PREFIX => (),
            n => return Err(unexpected_message_prefix(n)),
        };

        Ok(Self {
            transport,
            ready: false,
        })
    }

    pub async fn query<'a, R, U>(
        &'a mut self,
        statement: &str,
        params: impl Fn(&mut Vec<u8>) -> Result<()>,
    ) -> Result<U>
    where
        R: QueryResult,
        U: FromQueryResult<'a, R, T>,
    {
        self.recycle().await?;

        // Send Parse, Bind, Execute and Sync to start the query
        self.transport
            .send(|writer| {
                writer
                    .add(Parse { query: statement })?
                    .add(Bind { params })?
                    .add(Execute)?
                    .add(Sync)?;
                Ok(())
            })
            .await?;

        self.ready = false;

        // Waits for ParseComplete
        let msg = self.transport.receive_any().await?;
        match msg.prefix {
            ParseComplete::PREFIX => (),
            n => return Err(unexpected_message_prefix(n)),
        }

        // Waits for BindComplete
        let msg = self.transport.receive_any().await?;
        match msg.prefix {
            BindComplete::PREFIX => (),
            n => return Err(unexpected_message_prefix(n)),
        }

        U::from_stream(QueryStream::<R::Row, U::Row, T>::new(self)).await
    }

    pub async fn recycle(&mut self) -> Result<()> {
        if self.ready {
            return Ok(());
        }

        loop {
            let msg = self.transport.receive_any().await?;
            match msg.prefix {
                ReadyForQuery::PREFIX => {
                    self.ready = true;
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
        params: impl Fn(&mut Vec<u8>) -> Result<()>,
    ) -> Result<U>
    where
        R: QueryResult,
        U: FromQueryResult<'a, R, T>,
        T: SyncTransport,
    {
        self.query(statement, params)
            .now_or_never()
            .expect("transport should be sync")
            .map(From::from)
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
        let msg = self.0.transport.receive_any().await?;
        match msg.prefix {
            CommandComplete::PREFIX | EmptyQueryResponse::PREFIX => Ok(None),
            DataRow::PREFIX => {
                let row = Row {
                    inner: self.0.transport.read_message(&msg)?,
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
