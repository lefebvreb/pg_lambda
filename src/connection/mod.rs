use std::io::Result;
use std::iter::FusedIterator;
use std::mem::transmute;

use messages::{
    Authentication, BackendKeyData, BackendMessage, Bind, BindComplete, CommandComplete, DataRow,
    EmptyQueryResponse, ErrorResponse, Execute, FrontendMessage, NegotiateProtocolVersion,
    NoticeResponse, ParameterStatus, Parse, ParseComplete, ReadyForQuery, StartupMessage, Sync,
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

        if let Some(size) = M::SIZE {
            write_i32(size, &mut self.stack);
            msg.write(&mut self.stack)?;
        } else {
            let start = self.stack.len();
            self.stack.extend([0; 4]);
            msg.write(&mut self.stack)?;
            let size = (self.stack.len() - start) as i32;
            self.stack[start..start + 4].copy_from_slice(&size.to_be_bytes());
        }

        Ok(())
    }

    async fn send_all(&mut self) -> Result<()> {
        self.transport.write_all(&self.stack).await
    }

    async fn receive_one(&mut self) -> Result<()> {
        self.stack.clear();
        self.transport.read_exact(5, &mut self.stack).await?;
        let size = i32::from_be_bytes(self.stack[1..5].try_into().unwrap());
        self.transport.read_exact(size, &mut self.stack).await?;
        Ok(())
    }

    async fn receive_unhandled(&mut self) -> Result<AnyMessage> {
        loop {
            self.receive_one().await?;

            // todo: enhance error and warning reportings.
            match self.stack[0] {
                ErrorResponse::PREFIX => {
                    return Err(ErrorResponse::read(&mut self.stack.as_slice())?.into());
                }
                NegotiateProtocolVersion::PREFIX => {
                    return Err(NegotiateProtocolVersion::read(&mut self.stack.as_slice())?.into());
                }
                NoticeResponse::PREFIX | ParameterStatus::PREFIX => (),
                _ => return Ok(AnyMessage(&self.stack)),
            }
        }
    }
}

enum TransactionState {
    None,
    Underway,
    Failed,
}

pub struct Connection<T> {
    transport: BufferedTransport<T>,
    transaction_state: TransactionState,
    _secret_key: i32,
}

impl<T: Transport> Connection<T> {
    /// Waits for a ReadyForQuery
    async fn sync(&mut self) -> Result<()> {
        loop {
            let msg = self.transport.receive_unhandled().await?;
            match msg.prefix() {
                ReadyForQuery::PREFIX => {
                    self.transaction_state = match msg.read::<ReadyForQuery>()? {
                        ReadyForQuery::Idle => TransactionState::None,
                        ReadyForQuery::Transaction => TransactionState::Underway,
                        ReadyForQuery::FailedTransaction => TransactionState::Failed,
                    };
                    return Ok(());
                }
                _ => (),
            }
        }
    }

    pub async fn connect(config: &Config) -> Result<Self> {
        let mut transport = BufferedTransport {
            transport: T::connect(config).await?,
            stack: Vec::new(),
        };

        // Send StartupMessage
        transport.write_one(StartupMessage {
            user: &config.user,
            database: &config.database,
        })?;
        transport.send_all().await?;

        // Wait for Authenticate
        let msg = transport.receive_unhandled().await?;
        match msg.prefix() {
            Authentication::PREFIX => match msg.read::<Authentication>()? {
                Authentication::Ok => (),
            },
            n => return Err(unexpected_message_prefix(n)),
        }

        // Wait for BackendKeyData
        let msg = transport.receive_unhandled().await?;
        let secret_key = match msg.prefix() {
            BackendKeyData::PREFIX => {
                let msg = msg.read::<BackendKeyData>()?;
                msg.secret_key
            }
            n => return Err(unexpected_message_prefix(n)),
        };

        Ok(Self {
            transport,
            transaction_state: TransactionState::None,
            _secret_key: secret_key,
        })
    }

    pub async fn query<'a>(
        &'a mut self,
        statement: &str,
        params: impl Fn(&mut Vec<u8>) -> Result<()>,
    ) -> Result<&'a mut RowsStream<T>> {
        self.sync().await?;

        // Send Parse, Bind, Execute and Sync to start the query
        self.transport.clear();
        self.transport.write_one(Parse { query: statement })?;
        self.transport.write_one(Bind { params })?;
        self.transport.write_one(Execute)?;
        self.transport.write_one(Sync)?;
        self.transport.send_all().await?;

        // Waits for ParseComplete
        let msg = self.transport.receive_unhandled().await?;
        match msg.prefix() {
            ParseComplete::PREFIX => (),
            n => return Err(unexpected_message_prefix(n)),
        }

        // Waits for BindComplete
        let msg = self.transport.receive_unhandled().await?;
        match msg.prefix() {
            BindComplete::PREFIX => (),
            n => return Err(unexpected_message_prefix(n)),
        }

        Ok(RowsStream::new(self))
    }
}

/// A stream of rows produced as a result of a query.
///
/// # Why is this not a regular [`Stream`](futures::stream::Stream)?
///
/// Rows returned by [`RowsStream`] hold a reference to the buffer that
/// is inside the underlying [`Connection`]. This pattern in rust is
/// called a "lending iterator", or "streaming interator" and is not
/// well supported by the Rust ecosystem.
#[repr(transparent)]
pub struct RowsStream<T>(Connection<T>);

impl<T: Transport> RowsStream<T> {
    fn new(conn: &mut Connection<T>) -> &mut Self {
        // SAFETY: `RowsStream<T>` is a `transparent` wrapper of `Connection<T>`,
        // it is therefore safe to transmute a mutable reference of one into a
        // mutable reference of the other.
        unsafe { transmute(conn) }
    }

    pub async fn next(&mut self) -> Result<Option<Row>> {
        let msg = self.0.transport.receive_unhandled().await?;
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
