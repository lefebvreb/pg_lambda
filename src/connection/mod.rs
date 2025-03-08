use std::convert::identity;
use std::io::{Error, ErrorKind, Result};
use std::ops::{Deref, DerefMut};

use buffer::{AnyMessage, BufTransport};
use futures::FutureExt;
use messages::{
    Authentication, BackendKeyData, BackendMessage, Bind, BindComplete, CommandComplete, DataRow,
    EmptyQueryResponse, ErrorResponse, Execute, NegotiateProtocolVersion, NoticeResponse,
    ParameterStatus, Parse, ParseComplete, ReadyForQuery, SaslInitialResponse, SaslResponse,
    StartupMessage, Sync,
};
use params::{NoParams, QueryParams};
use result::{FromQueryResult, FromRow, Row, Void};
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

/// Types that can be used as transport layers for [`Connection`]s.
///
/// This trait abstracts connection, reading and writing to a
/// PostgreSQL instance.
#[trait_variant::make(TransportSend: Send)]
pub trait Transport: Sized {
    /// Attempts to connect to the database described in `config`.
    ///
    /// Multiple hosts and ports may be tested. If a TLS handshake
    /// is desired, this function must take care of the appropriate
    /// negociations with the PostgreSQL instance on the other end
    /// of the line.
    ///
    /// # Cancel safety
    ///
    /// This method may be cancel unsafe.
    async fn connect(config: &Config) -> Result<Self>;

    /// Attempts to read some of the bytes from the network into `dst`.
    ///
    /// In case of success, the number of bytes read into `dst` must
    /// be returned. In case of failure, no bytes must have been read at all.
    ///
    /// Returning `0` means the connection was closed by the database and must
    /// not be used again.
    ///
    /// # Cancel safety
    ///
    /// This method must be cancel safe, no bytes must have been read in
    /// case of cancellation.
    async fn read(&mut self, dst: &mut [u8]) -> Result<usize>;

    /// Attempts to write some bytes to the network from `src`.
    ///
    /// In case of success, the number of bytes written from `src` must
    /// be returned. In case of failure, no bytes must have been written at all.
    ///
    /// Returning `0` means the connection was closed by the database and must
    /// not be used again.
    ///
    /// # Cancel safety
    ///
    /// This method must be cancel safe, no bytes must have been written in
    /// case of cancellation.
    async fn write(&mut self, src: &[u8]) -> Result<usize>;
}

/// Marker trait for [`Transport`] implementations that can be used in synchronous contexts.
///
/// By implementing this trait on `T`, the implementer asserts that futures returned by
/// the implementation of [`Transport`] on `T` are immediately ready. Failure to uphold
/// this assertion will result in [`panic`]s when using a [`Connection`] with this
/// `T` in a synchronous context.
pub trait SyncTransport: Transport {}

#[derive(Debug)]
enum TransactionState {
    None,
    Some,
    Failed,
}

#[derive(Debug)]
pub struct Connection<T> {
    transport: BufTransport<T>,
    transaction: TransactionState,
    ready_for_query: bool,
}

impl<T: Transport> Connection<T> {
    /// Receives a single message that is not an error or warning and cannot be automatically handled.
    async fn receive_unhandled(&mut self) -> Result<AnyMessage> {
        loop {
            let msg = self.transport.receive_any().await?;
            // todo: handle these better.
            let res = match msg.prefix {
                ErrorResponse::PREFIX => Err(self
                    .transport
                    .parse::<ErrorResponse>(&msg)
                    .map_or_else(identity, Error::from)),
                NegotiateProtocolVersion::PREFIX => Err(self
                    .transport
                    .parse::<NegotiateProtocolVersion>(&msg)
                    .map_or_else(identity, Error::from)),
                NoticeResponse::PREFIX | ParameterStatus::PREFIX => Ok(()),
                _ => return Ok(msg),
            };
            self.transport.cut(&msg);
            res?;
        }
    }

    /// Performs SASL authentication. Only supports `SCRAM-SHA-256` for now.
    async fn sasl_authentication(
        &mut self,
        config: &Config,
        mechanisms: Vec<Box<[u8]>>,
    ) -> Result<()> {
        self.transport.clear();

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

        self.transport
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
            match self.transport.parse(&msg)? {
                Authentication::Ok => return Ok(()),
                Authentication::SaslContinue { data } => {
                    let state = session
                        .step(Some(data), &mut buf)
                        .map_err(from_session_error)?;
                    if state.is_running() {
                        self.transport
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

    // Stack after successful connect: ( ∅ )
    pub async fn connect(config: &Config) -> Result<Self> {
        let mut this = Self {
            transport: BufTransport::new(T::connect(config).await?),
            transaction: TransactionState::None,
            ready_for_query: false,
        };

        // Send StartupMessage
        this.transport
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
        match this.transport.parse(&msg)? {
            Authentication::Ok => (),
            Authentication::Sasl { mechanisms } => {
                let mechanisms = mechanisms
                    .into_iter()
                    .map(|mechanism| mechanism.to_bytes().into())
                    .collect();
                this.sasl_authentication(config, mechanisms).await?;
            }
            _ => return Err(unexpected_authentication_message()),
        }
        this.transport.clear();

        // Wait for BackendKeyData
        let msg = this.receive_unhandled().await?;
        if msg.prefix != BackendKeyData::PREFIX {
            return Err(unexpected_message_prefix(msg.prefix));
        }
        this.transport.clear();

        // Wait for ReadyForQuery
        let msg = this.receive_unhandled().await?;
        if msg.prefix != ReadyForQuery::PREFIX {
            return Err(unexpected_message_prefix(msg.prefix));
        }
        this.on_ready_for_query(&msg)?;
        this.transport.clear();

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
        self.transport
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
        self.transport.clear();
        if msg.prefix != ParseComplete::PREFIX {
            return Err(unexpected_message_prefix(msg.prefix));
        }

        // Waits for BindComplete
        let msg = self.receive_unhandled().await?;
        self.transport.clear();
        if msg.prefix != BindComplete::PREFIX {
            return Err(unexpected_message_prefix(msg.prefix));
        }

        U::from_conn(self).await
    }

    pub async fn transaction(&mut self) -> Result<Transaction<T>> {
        self.query::<Void, ()>("BEGIN", NoParams).await?;
        Ok(Transaction { conn: self })
    }

    pub async fn recycle(&mut self) -> Result<()> {
        while !self.ready_for_query {
            let msg = self.receive_unhandled().await?;
            self.transport.clear();
            match msg.prefix {
                ReadyForQuery::PREFIX => self.ready_for_query = true,
                CommandComplete::PREFIX | EmptyQueryResponse::PREFIX | DataRow::PREFIX => (),
                n => return Err(unexpected_message_prefix(n)),
            }
        }

        // here: downsize buffer and other cleanup tasks.

        Ok(())
    }

    pub fn is_closed(&self) -> bool {
        self.transport.is_closed()
    }

    async fn next_row(&mut self) -> Result<Option<AnyMessage>> {
        if self.ready_for_query {
            return Ok(None);
        }

        let msg = self.receive_unhandled().await?;
        let res = match msg.prefix {
            // Another row was produced, it is appended to the buffer, so simply return it.
            DataRow::PREFIX => return Ok(Some(msg)),
            // Query is finished, we should receive a ReadyForQuery immediately after.
            CommandComplete::PREFIX | EmptyQueryResponse::PREFIX => {
                self.transport.cut(&msg);
                let msg = self.receive_unhandled().await?;
                if msg.prefix != ReadyForQuery::PREFIX {
                    Err(unexpected_message_prefix(msg.prefix))
                } else {
                    self.on_ready_for_query(&msg)?;
                    Ok(None)
                }
            }
            // Previous try to get a ReadyForQuery was most likely cancelled, so here we are again.
            ReadyForQuery::PREFIX => {
                self.on_ready_for_query(&msg)?;
                Ok(None)
            }
            n => Err(unexpected_message_prefix(n)),
        };
        self.transport.cut(&msg);
        res
    }

    fn parse_row<'a, R, U>(&'a self, msg: &AnyMessage) -> Result<U>
    where
        U: FromRow<'a, R>,
    {
        self.transport
            .parse(msg)
            .map(Row::new)
            .and_then(U::from_row)
    }

    fn on_ready_for_query(&mut self, msg: &AnyMessage) -> Result<()> {
        self.ready_for_query = true;
        self.transaction = match self.transport.parse(msg)? {
            ReadyForQuery::Idle => TransactionState::None,
            ReadyForQuery::Transaction => TransactionState::Some,
            ReadyForQuery::FailedTransaction => TransactionState::Failed,
        };
        Ok(())
    }
}

impl<T: SyncTransport> Connection<T> {
    pub fn connect_sync(config: &Config) -> Result<Self> {
        Self::connect(config)
            .now_or_never()
            .expect("future should resolve immediately")
    }

    pub fn query_sync<'a, R, U>(
        &'a mut self,
        statement: &str,
        params: impl QueryParams,
    ) -> Result<U>
    where
        U: FromQueryResult<'a, R, T>,
    {
        self.query(statement, params)
            .now_or_never()
            .expect("future should resolve immediately")
    }

    pub fn transaction_sync(&mut self) -> Result<Transaction<T>> {
        self.transaction()
            .now_or_never()
            .expect("future should resolve immediately")
    }

    pub fn recycle_sync(&mut self) -> Result<()> {
        self.recycle()
            .now_or_never()
            .expect("future should resolve immediately")
    }
}

pub struct Transaction<'a, T> {
    conn: &'a mut Connection<T>,
}

impl<T: Transport> Transaction<'_, T> {
    pub async fn commit(self) -> Result<()> {
        self.conn.query::<Void, ()>("COMMIT", NoParams).await
    }

    pub async fn rollback(self) -> Result<()> {
        self.conn.query::<Void, ()>("ROLLBACK", NoParams).await
    }
}

impl<T: SyncTransport> Transaction<'_, T> {
    pub fn commit_sync(self) -> Result<()> {
        self.commit()
            .now_or_never()
            .expect("future should resolve immediately")
    }

    pub fn rollback_sync(self) -> Result<()> {
        self.rollback()
            .now_or_never()
            .expect("future should resolve immediately")
    }
}

impl<T> Deref for Transaction<'_, T> {
    type Target = Connection<T>;

    fn deref(&self) -> &Self::Target {
        self.conn
    }
}

impl<T> DerefMut for Transaction<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.conn
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
