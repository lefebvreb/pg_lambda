use std::convert::identity;
use std::io::{Error, Result};
use std::iter::FusedIterator;

use messages::{
    Authentication, BackendKeyData, BackendMessage, Bind, BindComplete, CommandComplete, DataRow,
    EmptyQueryResponse, ErrorResponse, Execute, FrontendMessage, NegotiateProtocolVersion,
    NoticeResponse, ParameterStatus, Parse, ParseComplete, ReadyForQuery, StartupMessage,
};
use util::{read_i32, read_slice, unexpected_message_prefix, write_i32};

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

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl ExactSizeIterator for Row<'_> {
    fn len(&self) -> usize {
        self.inner.len as usize
    }
}

impl FusedIterator for Row<'_> {}

struct AnyMessage<'a>(&'a [u8]);

impl<'a> AnyMessage<'a> {
    fn prefix(&self) -> u8 {
        self.0[0]
    }

    fn read<M: BackendMessage<'a>>(&self) -> Result<M> {
        let mut src = &self.0[5..];
        M::read(&mut src)
    }
}

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

struct BufferedTransport<T> {
    transport: T,
    stack: Vec<u8>,
}

impl<T: Transport> BufferedTransport<T> {
    fn clear(&mut self) {
        self.stack.clear();
    }

    fn write_one<M: FrontendMessage>(&mut self, msg: M) -> Result<()> {
        if let Some(byte) = M::PREFIX {
            self.stack.push(byte);
        }
        let len = self.stack.len();
        if let Some(size) = M::SIZE {
            write_i32(size, &mut self.stack);
            msg.write(&mut self.stack)
                .inspect_err(|_| self.stack.truncate(len))?;
        } else {
            self.stack.extend([0; 4]);
            msg.write(&mut self.stack)
                .inspect_err(|_| self.stack.truncate(len))?;
            let size = (self.stack.len() - len) as i32;
            self.stack[len..len + 4].copy_from_slice(&size.to_be_bytes());
        }
        Ok(())
    }

    async fn send_multiple(&mut self, f: impl FnOnce(&mut Self) -> Result<()>) -> Result<()> {
        let len = self.stack.len();
        f(self).inspect_err(|_| self.stack.truncate(len))?;
        let res = self.transport.write_all(&self.stack).await;
        self.stack.truncate(len);
        res
    }

    async fn receive_one(&mut self) -> Result<AnyMessage> {
        let len = self.stack.len();
        self.transport.read_exact(5, &mut self.stack).await?;
        let size = i32::from_be_bytes(self.stack[len + 1..len + 5].try_into().unwrap());
        self.transport.read_exact(size, &mut self.stack).await?;
        Ok(AnyMessage(&self.stack[len..]))
    }

    async fn receive_unhandled(&mut self) -> Result<AnyMessage> {
        loop {
            let len = self.stack.len();
            let msg = match self.receive_one().await {
                Ok(msg) => msg,
                Err(err) => {
                    self.stack.truncate(len);
                    return Err(err);
                }
            };
            // todo: enhance error and warning reportings.
            match msg.prefix() {
                ErrorResponse::PREFIX => {
                    let err = msg
                        .read::<ErrorResponse>()
                        .map_or_else(identity, Error::from);
                    self.stack.truncate(len);
                    return Err(err);
                }
                NegotiateProtocolVersion::PREFIX => {
                    let err = msg
                        .read::<NegotiateProtocolVersion>()
                        .map_or_else(identity, Error::from);
                    self.stack.truncate(len);
                    return Err(err);
                }
                NoticeResponse::PREFIX | ParameterStatus::PREFIX => self.stack.truncate(len),
                _ => return Ok(AnyMessage(&self.stack[len..])),
            }
        }
    }
}

pub struct Connection<T> {
    transport: BufferedTransport<T>,
    _secret_key: i32,
}

impl<T: Transport> Connection<T> {
    pub async fn connect(config: &Config) -> Result<Self> {
        let mut transport = BufferedTransport {
            transport: T::connect(config).await?,
            stack: Vec::new(),
        };

        // Send StartupMessage
        transport
            .send_multiple(|transport| {
                transport.write_one(StartupMessage {
                    user: &config.user,
                    database: &config.database,
                })
            })
            .await?;

        // Wait for Authenticate
        let msg = transport.receive_unhandled().await?;
        match msg.prefix() {
            Authentication::PREFIX => match msg.read::<Authentication>()? {
                Authentication::Ok => (),
            },
            n => return Err(unexpected_message_prefix(n)),
        }
        transport.clear();

        // Wait for BackendKeyData
        let secret_key;
        let msg = transport.receive_unhandled().await?;
        match msg.prefix() {
            BackendKeyData::PREFIX => {
                let msg = msg.read::<BackendKeyData>()?;
                secret_key = msg.secret_key;
            }
            n => return Err(unexpected_message_prefix(n)),
        }
        transport.clear();

        // Wait for ReadyForQuery
        let msg = transport.receive_unhandled().await?;
        match msg.prefix() {
            ReadyForQuery::PREFIX => (),
            n => return Err(unexpected_message_prefix(n)),
        }
        transport.clear();

        Ok(Self {
            transport,
            _secret_key: secret_key,
        })
    }

    pub(crate) async fn extended_query(
        &mut self,
        statement: &str,
        write_params: impl Fn(&mut Vec<u8>) -> Result<()>,
    ) -> Result<()> {
        // Send Parse, Bind and Execute to start the query
        self.transport
            .send_multiple(|transport| {
                transport.write_one(Parse { query: statement })?;
                transport.write_one(Bind { write_params })?;
                transport.write_one(Execute)
            })
            .await?;

        // Wait for ParseComplete
        let msg = self.transport.receive_unhandled().await?;
        match msg.prefix() {
            ParseComplete::PREFIX => (),
            n => return Err(unexpected_message_prefix(n)),
        }
        self.transport.clear();

        // Wait for BindComplete
        let msg = self.transport.receive_unhandled().await?;
        match msg.prefix() {
            BindComplete::PREFIX => (),
            n => return Err(unexpected_message_prefix(n)),
        }
        self.transport.clear();

        Ok(())
    }

    pub(crate) async fn next_row(&mut self) -> Result<Option<Row>> {
        let msg = self.transport.receive_unhandled().await?;
        match msg.prefix() {
            CommandComplete::PREFIX | EmptyQueryResponse::PREFIX => Ok(None),
            DataRow::PREFIX => Ok(Some(Row { inner: msg.read()? })),
            n => Err(unexpected_message_prefix(n)),
        }
    }
}

#[cfg(any(feature = "bb8", feature = "deadpool", feature = "r2d2"))]
pub struct ConnectionManager<T> {
    config: Config,
    _marker: std::marker::PhantomData<fn(T)>,
}

pub struct RowsStream<'a, T>(&'a mut Connection<T>);

impl<T: Transport> RowsStream<'_, T> {
    pub async fn next<'a>(&'a mut self) -> Result<Option<Row<'a>>> {
        let msg = self.0.transport.receive_unhandled().await?;
        match msg.prefix() {
            CommandComplete::PREFIX | EmptyQueryResponse::PREFIX => Ok(None),
            DataRow::PREFIX => Ok(Some(Row { inner: msg.read()? })),
            n => Err(unexpected_message_prefix(n)),
        }
    }
}
