use std::io::Result;
use std::iter::FusedIterator;

use futures::Stream;
use messages::{
    Authentication, BackendKeyData, BackendMessage, Bind, BindComplete, DataRow, ErrorResponse, Execute, FrontendMessage, NegotiateProtocolVersion, NoticeResponse, ParameterStatus, Parse, ParseComplete, ReadyForQuery, StartupMessage
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
        Some(match read_i32(&mut self.inner.bytes) {
            Ok(-1) => Ok(None),
            Ok(len) => read_slice(len, &mut self.inner.bytes).map(Some),
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
    buffer: Vec<u8>,
}

impl<T: Transport> BufferedTransport<T> {
    fn clear(&mut self) {
        self.buffer.clear();
    }

    fn write_one<M: FrontendMessage>(&mut self, msg: M) -> Result<()> {
        if let Some(byte) = M::PREFIX {
            self.buffer.push(byte);
        }
        if let Some(size) = M::SIZE {
            write_i32(size, &mut self.buffer);
            msg.write(&mut self.buffer)
        } else {
            let len = self.buffer.len();
            self.buffer.extend([0; 4]);
            msg.write(&mut self.buffer)?;
            let size = (self.buffer.len() - len) as i32;
            self.buffer[len..len + 4].copy_from_slice(&size.to_be_bytes());
            Ok(())
        }
    }

    async fn send_all(&mut self) -> Result<()> {
        self.transport.write_all(&self.buffer).await?;
        self.clear();
        Ok(())
    }

    async fn receive_any(&mut self) -> Result<AnyMessage> {
        self.clear();
        self.transport.read_exact(5, &mut self.buffer).await?;
        let size = i32::from_be_bytes(self.buffer[1..5].try_into().unwrap());
        self.transport.read_exact(size, &mut self.buffer).await?;
        Ok(AnyMessage(&self.buffer))
    }

    async fn receive_unhandled(&mut self) -> Result<AnyMessage> {
        loop {
            let msg = self.receive_any().await?;
    
            // todo: enhance error and warning reportings.
            match msg.prefix() {
                ErrorResponse::PREFIX => return Err(msg.read::<ErrorResponse>()?.into()),
                NegotiateProtocolVersion::PREFIX => return Err(msg.read::<NegotiateProtocolVersion>()?.into()),
                NoticeResponse::PREFIX => {
                    msg.read::<NoticeResponse>()?;
                }
                ParameterStatus::PREFIX => {
                    msg.read::<ParameterStatus>()?;
                }
                _ => return Ok(AnyMessage(&self.buffer)),
            }
        }
    }

    // async fn receive_one(&mut self) -> Result<AnyMessage> {
    //     loop {
    //         if let Some(msg) = self.try_receive_one().await? {
    //             return Ok(msg);
    //         }
    //     }
    // }
}

pub struct Connection<T> {
    transport: BufferedTransport<T>,
    secret_key: i32,
}

impl<T: Transport> Connection<T> {
    pub async fn connect(config: &Config) -> Result<Self> {
        let mut transport = BufferedTransport {
            transport: T::connect(config).await?,
            buffer: Vec::new(),
        };

        // Send StartupMessage
        transport.clear();
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
        let secret_key;
        let msg = transport.receive_unhandled().await?;
        match msg.prefix() {
            BackendKeyData::PREFIX => {
                let msg = msg.read::<BackendKeyData>()?;
                secret_key = msg.secret_key;
            },
            n => return Err(unexpected_message_prefix(n)),
        }
        
        // Wait for ReadyForQuery
        let msg = transport.receive_unhandled().await?;
        match msg.prefix() {
            ReadyForQuery::PREFIX => (),
            n => return Err(unexpected_message_prefix(n)),
        }

        Ok(Self {
            transport,
            secret_key,
        })
    }

    pub async fn extended_query<'a>(
        &'a mut self,
        statement: &str,
        write_params: impl Fn(&mut Vec<u8>) -> Result<()>,
    ) -> Result<impl Stream<Item = Result<Row<'a>>>> {
        // Send Parse, Bind and Execute to start the query
        self.transport.clear();
        self.transport.write_one(Parse {
            query: statement,
        })?;
        self.transport.write_one(Bind {
            write_params,
        })?;
        self.transport.write_one(Execute)?;
        self.transport.send_all().await?;

        // Wait for ParseComplete
        let msg = self.transport.receive_unhandled().await?;
        match msg.prefix() {
            ParseComplete::PREFIX => (),
            n => return Err(unexpected_message_prefix(n)),
        }

        // Wait for BindComplete
        let msg = self.transport.receive_unhandled().await?;
        match msg.prefix() {
            BindComplete::PREFIX => (),
            n => return Err(unexpected_message_prefix(n)),
        }

        // and now, a series of DataRow followed by a CommandComplete or an EmptyQueryResponse

        Ok(futures::stream::empty())
    }
}

#[cfg(any(feature = "bb8", feature = "deadpool", feature = "r2d2"))]
pub struct ConnectionManager<T> {
    config: Config,
    _marker: std::marker::PhantomData<fn(T)>,
}

pub struct RowsStream<'a, T: Transport> {
    conn: &'a mut Connection<T>,
}
