use std::convert::identity;
use std::io::{Error, ErrorKind, Result};
use std::ops::{Deref, DerefMut};

use buffer::{AnyMessage, Buffer};
use futures::FutureExt;
use messages::{
    Authentication, BackendKeyData, BackendMessage, Bind, BindComplete, CommandComplete, DataRow,
    EmptyQueryResponse, ErrorResponse, Execute, NegotiateProtocolVersion, NoticeResponse,
    ParameterStatus, Parse, ParseComplete, ReadyForQuery, SaslInitialResponse, SaslResponse,
    StartupMessage, Sync,
};
use params::QueryParams;
use result::{FromQueryResult, FromRow, Row};
use rsasl::config::SASLConfig;
use rsasl::prelude::{Mechname, SASLClient};
use util::{from_session_error, unexpected_authentication_message, unexpected_message_prefix};

mod buffer;
mod impls;
mod messages;
pub mod params;
pub mod result;
mod util;

// todo: add a config for max connection buffer size, and implement a mechanism to shrink buffers when it gets too large.
#[derive(Clone, Debug)]
pub struct Config {
    pub user: String,
    pub password: String,
    pub database: String,
    pub host: String,
    pub port: u16,
}

#[trait_variant::make(TransportSend: Send)]
pub trait Transport: Sized {
    async fn connect(config: &Config) -> Result<Self>;

    async fn read(&mut self, dst: &mut [u8]) -> Result<i32>;

    async fn write(&mut self, src: &[u8]) -> Result<i32>;
}

/// Marker trait for [`Transport`] implementations that can be used in synchronous contexts.
///
/// By implementing this trait on `T`, the implementer asserts that futures returned by
/// the implementation of [`Transport`] on `T` are immediately ready. Failure to uphold
/// this assertion will result in [`panic`]s when using a [`Connection`] with this
/// `T` in a synchronous context.
pub trait SyncTransport: Transport {}

#[derive(Debug)]
pub struct Connection<T> {
    buffer: Buffer<T>,
    ready_for_query: bool,
}

impl<T: Transport> Connection<T> {
    /// Receives a single message that is not an error or warning and cannot be automatically handled.
    async fn receive_unhandled(&mut self) -> Result<AnyMessage> {
        loop {
            let msg = self.buffer.receive_any().await?;
            // todo: handle these better.
            let res = match msg.prefix {
                ErrorResponse::PREFIX => Err(self
                    .buffer
                    .parse::<ErrorResponse>(&msg)
                    .map_or_else(identity, Error::from)),
                NegotiateProtocolVersion::PREFIX => Err(self
                    .buffer
                    .parse::<NegotiateProtocolVersion>(&msg)
                    .map_or_else(identity, Error::from)),
                NoticeResponse::PREFIX | ParameterStatus::PREFIX => Ok(()),
                _ => return Ok(msg),
            };
            self.buffer.cut(&msg);
            res?;
        }
    }

    /// Performs SASL authentication. Only supports `SCRAM-SHA-256` for now.
    async fn sasl(&mut self, config: &Config, mechanisms: Vec<Box<[u8]>>) -> Result<()> {
        let mut session =
            SASLConfig::with_credentials(None, config.user.clone(), config.password.clone())
                .and_then(|sasl_config| {
                    SASLClient::new(sasl_config).start_suggested_iter(
                        mechanisms.iter().flat_map(|name| Mechname::parse(name)),
                    )
                })
                .map_err(|e| Error::new(ErrorKind::Unsupported, e))?;

        let mut buf = Vec::new();
        if session.are_we_first() {
            session.step(None, &mut buf).map_err(from_session_error)?;
        }

        self.buffer
            .send_all(|mut writer| {
                writer.add(SaslInitialResponse {
                    mechanism: session.get_mechname(),
                    data: session.are_we_first().then_some(buf.as_slice()),
                })
            })
            .await?;
        buf.clear();

        loop {
            let msg = self.receive_unhandled().await?;
            if msg.prefix != Authentication::PREFIX {
                return Err(unexpected_message_prefix(msg.prefix));
            }
            match self.buffer.parse(&msg)? {
                Authentication::Ok => return Ok(()),
                Authentication::SaslContinue { data } => {
                    let state = session
                        .step(Some(data), &mut buf)
                        .map_err(from_session_error)?;
                    if state.is_running() {
                        self.buffer
                            .send_all(|mut writer| {
                                writer.add(SaslResponse {
                                    data: buf.as_slice(),
                                })
                            })
                            .await?;
                        buf.clear();
                    }
                }
                Authentication::SaslFinal => (),
                _ => return Err(unexpected_authentication_message()),
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
        if msg.prefix != Authentication::PREFIX {
            return Err(unexpected_message_prefix(msg.prefix));
        }
        match this.buffer.parse(&msg)? {
            Authentication::Ok => (),
            Authentication::Sasl { mechanisms } => {
                let mechanisms = mechanisms
                    .into_iter()
                    .map(|mechanism| mechanism.to_bytes().into())
                    .collect();
                this.sasl(config, mechanisms).await?;
            }
            _ => return Err(unexpected_authentication_message()),
        }
        this.buffer.clear();

        // Wait for BackendKeyData
        let msg = this.receive_unhandled().await?;
        if msg.prefix != BackendKeyData::PREFIX {
            return Err(unexpected_message_prefix(msg.prefix));
        }
        this.buffer.clear();

        // Wait for ReadyForQuery
        let msg = this.receive_unhandled().await?;
        if msg.prefix != ReadyForQuery::PREFIX {
            return Err(unexpected_message_prefix(msg.prefix));
        }
        this.buffer.clear();

        Ok(this)
    }

    pub async fn query<'a, R, U>(
        &'a mut self,
        statement: &str,
        params: impl QueryParams,
    ) -> Result<U>
    where
        U: FromQueryResult<'a, R, T>,
    {
        self.recycle().await?;

        // Send Parse, Bind, Execute and Sync
        self.buffer
            .send_all(|mut writer| {
                writer.add(Parse { statement })?;
                writer.add(Bind { params })?;
                writer.add(Execute)?;
                writer.add(Sync)?;
                self.ready_for_query = false;
                Ok(())
            })
            .await?;

        // Waits for ParseComplete
        let msg = self.receive_unhandled().await?;
        if msg.prefix != ParseComplete::PREFIX {
            return Err(unexpected_message_prefix(msg.prefix));
        }
        self.buffer.clear();

        // Waits for BindComplete
        let msg = self.receive_unhandled().await?;
        if msg.prefix != BindComplete::PREFIX {
            return Err(unexpected_message_prefix(msg.prefix));
        }
        self.buffer.clear();

        U::from_conn(self).await
    }

    pub async fn recycle(&mut self) -> Result<()> {
        if self.ready_for_query {
            // Would it be OK to cut/clear the buffer here?
            return Ok(());
        }

        loop {
            let msg = self.receive_unhandled().await?;
            self.buffer.clear();
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

    pub fn is_closed(&self) -> bool {
        self.buffer.is_closed()
    }

    pub fn into_sync(self) -> SyncConnection<T>
    where
        T: SyncTransport,
    {
        SyncConnection { inner: self }
    }

    async fn next_row(&mut self) -> Result<Option<AnyMessage>> {
        let msg = self.receive_unhandled().await?;
        let res = match msg.prefix {
            CommandComplete::PREFIX | EmptyQueryResponse::PREFIX => Ok(None),
            DataRow::PREFIX => return Ok(Some(msg)),
            n => Err(unexpected_message_prefix(n)),
        };
        self.buffer.cut(&msg);
        res
    }

    fn parse_row<'a, R, U>(&'a self, msg: &AnyMessage) -> Result<U>
    where
        U: FromRow<'a, R>,
    {
        self.buffer.parse(msg).map(Row::new).and_then(U::from_row)
    }

    async fn end_query(&mut self) -> Result<()> {
        let msg = self.receive_unhandled().await?;
        self.buffer.cut(&msg);
        match msg.prefix {
            ReadyForQuery::PREFIX => {
                self.ready_for_query = true;
                Ok(())
            }
            n => Err(unexpected_message_prefix(n)),
        }
    }
}

#[derive(Debug)]
pub struct SyncConnection<T> {
    inner: Connection<T>,
}

impl<T: SyncTransport> SyncConnection<T> {
    pub fn connect(config: &Config) -> Result<Self> {
        Ok(Self {
            inner: Connection::connect(config)
                .now_or_never()
                .expect("future should resolve immediately")?,
        })
    }

    pub fn query<'a, R, U>(&'a mut self, statement: &str, params: impl QueryParams) -> Result<U>
    where
        U: FromQueryResult<'a, R, T>,
    {
        self.inner
            .query(statement, params)
            .now_or_never()
            .expect("future should resolve immediately")
    }

    pub fn recycle(&mut self) -> Result<()> {
        self.inner
            .recycle()
            .now_or_never()
            .expect("future should resolve immediately")
    }

    pub fn into_async(self) -> Connection<T> {
        self.inner
    }
}

impl<T> Deref for SyncConnection<T> {
    type Target = Connection<T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for SyncConnection<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

#[cfg(any(feature = "bb8", feature = "deadpool", feature = "r2d2"))]
#[cfg_attr(
    docsrs,
    doc(cfg(any(feature = "bb8", feature = "deadpool", feature = "r2d2")))
)]
pub struct ConnectionManager<T> {
    config: Config,
    _marker: std::marker::PhantomData<fn(T)>,
}
