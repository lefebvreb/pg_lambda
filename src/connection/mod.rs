use std::convert::identity;
use std::io::{Error, ErrorKind, Result};
use std::ops::{Deref, DerefMut};

use buffer::{AnyMessage, Buffer};
use futures::FutureExt;
use messages::{
    Authentication, BackendKeyData, BackendMessage, Bind, BindComplete, CommandComplete, DataRow,
    EmptyQueryResponse, ErrorResponse, Execute, NegotiateProtocolVersion, NoticeResponse,
    ParameterStatus, Parse, ParseComplete, ReadyForQuery, SaslInitialResponse, SaslResponse,
    StartupMessage, Sync, Terminate,
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
/// By implementing this trait on `T`, the implementer asserts that the implementation of
/// [`Transport`] on `T` does not use any `await`s. Failure to uphold this assertion
/// may result in [`panic`]s when using a [`Connection`] with this `T` in a
/// synchronous context.
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
            match msg.prefix() {
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
                // todo: handle these better.
                NoticeResponse::PREFIX | ParameterStatus::PREFIX => self.buffer.clear(),
                _ => return Ok(msg),
            }
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
            match msg.prefix() {
                Authentication::PREFIX => match self.buffer.parse(&msg)? {
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
                },
                n => return Err(unexpected_message_prefix(n)),
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
        match msg.prefix() {
            Authentication::PREFIX => match this.buffer.parse(&msg)? {
                Authentication::Ok => (),
                Authentication::Sasl { mechanisms } => {
                    let mechanisms = mechanisms
                        .into_iter()
                        .map(|mechanism| mechanism.to_bytes().into())
                        .collect();
                    this.sasl(config, mechanisms).await?;
                }
                _ => return Err(unexpected_authentication_message()),
            },
            n => return Err(unexpected_message_prefix(n)),
        }
        this.buffer.clear();

        // Wait for BackendKeyData
        let msg = this.receive_unhandled().await?;
        match msg.prefix() {
            BackendKeyData::PREFIX => (),
            n => return Err(unexpected_message_prefix(n)),
        };
        this.buffer.clear();

        // Wait for ReadyForQuery
        let msg = this.receive_unhandled().await?;
        match msg.prefix() {
            ReadyForQuery::PREFIX => (),
            n => return Err(unexpected_message_prefix(n)),
        };
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
        match msg.prefix() {
            ParseComplete::PREFIX => (),
            n => return Err(unexpected_message_prefix(n)),
        }
        self.buffer.clear();

        // Waits for BindComplete
        let msg = self.receive_unhandled().await?;
        match msg.prefix() {
            BindComplete::PREFIX => (),
            n => return Err(unexpected_message_prefix(n)),
        }
        self.buffer.clear();

        U::from_conn(self).await
    }

    pub async fn recycle(&mut self) -> Result<()> {
        if self.ready_for_query {
            return Ok(());
        }

        loop {
            let msg = self.receive_unhandled().await?;
            match msg.prefix() {
                ReadyForQuery::PREFIX => {
                    self.ready_for_query = true;
                    return Ok(());
                }
                CommandComplete::PREFIX | EmptyQueryResponse::PREFIX | DataRow::PREFIX => (),
                n => return Err(unexpected_message_prefix(n)),
            }
            self.buffer.clear();
        }
    }

    pub async fn terminate(mut self) -> Result<()> {
        self.recycle().await?;
        self.buffer
            .send_all(|mut writer| writer.add(Terminate))
            .await?;
        Ok(())
    }

    pub(crate) async fn next_row<'a, R, U>(&'a mut self) -> Result<Option<U>>
    where
        U: FromRow<'a, R>,
    {
        let msg = self.receive_unhandled().await?;
        match msg.prefix() {
            CommandComplete::PREFIX | EmptyQueryResponse::PREFIX => (),
            DataRow::PREFIX => {
                let row = Row::new(self.buffer.parse(&msg)?);
                return U::from_row(row).map(Some);
            }
            n => return Err(unexpected_message_prefix(n)),
        }
        self.buffer.clear();

        let msg = self.receive_unhandled().await?;
        match msg.prefix() {
            ReadyForQuery::PREFIX => {
                self.ready_for_query = true;
                Ok(None)
            }
            n => Err(unexpected_message_prefix(n)),
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

    pub fn terminate(self) -> Result<()>
    where
        T: SyncTransport,
    {
        self.inner
            .terminate()
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
pub struct ConnectionManager<T> {
    config: Config,
    _marker: std::marker::PhantomData<fn(T)>,
}
