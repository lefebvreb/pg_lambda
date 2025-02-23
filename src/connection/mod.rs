use std::io::{Read, Result, Write};
use std::net::TcpStream;

use futures::Stream;
use messages::DataRow;
use util::{read_i32, read_slice};

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

#[trait_variant::make(TransportSend: Send)]
pub trait Transport: Sized {
    async fn connect(config: &Config) -> Result<Self>;

    async fn read_exact(&mut self, limit: i32, dst: &mut Vec<u8>) -> Result<()>;

    async fn write_all(&mut self, src: &[u8]) -> Result<()>;
}

/// Marker traits for [`Transport`] implementations that can be used in synchronous contexts.
///
/// By implementing this trait on `T`, the implementer asserts that the implementation of
/// [`Transport`] on `T` does not use any `await`s. Failure to uphold this assertion
/// may result in [`panic`]s when using a [`Connection`] with this `T` in a
/// synchronous context.
pub trait SyncTransport: Transport {}

pub struct Config {
    pub user: String,
    pub dbname: String,
    pub host: String,
    pub max_buffer_capacity: Option<usize>,
}

pub struct Connection<T> {
    transport: T,
    buf: Vec<u8>,
}

impl<T: Transport> Connection<T> {
    pub async fn connect(config: &Config) -> Result<Self> {
        Ok(Self {
            transport: T::connect(config).await?,
            buf: Vec::new(),
        })
    }

    pub async fn query<'a>(
        &'a mut self,
        statement: &str,
        write_params: impl Fn(&mut Vec<u8>) -> Result<()>,
    ) -> impl Stream<Item = Row<'a>> {
        futures::stream::empty()
    }
}

impl TransportSend for TcpStream {
    async fn connect(config: &Config) -> Result<Self> {
        Self::connect(&config.host)
    }

    async fn read_exact(&mut self, limit: i32, dst: &mut Vec<u8>) -> Result<()> {
        Read::take(self, limit as u64).read_to_end(dst).map(drop)
    }

    async fn write_all(&mut self, src: &[u8]) -> Result<()> {
        Write::write_all(self, src)
    }
}

impl SyncTransport for TcpStream {}

#[cfg(feature = "tokio")]
impl TransportSend for tokio::net::TcpStream {
    async fn connect(config: &Config) -> Result<Self> {
        Self::connect(&config.host).await
    }

    async fn read_exact(&mut self, limit: i32, dst: &mut Vec<u8>) -> Result<()> {
        use tokio::io::AsyncReadExt;
        tokio::io::AsyncReadExt::take(self, limit as u64)
            .read_to_end(dst)
            .await
            .map(drop)
    }

    async fn write_all(&mut self, src: &[u8]) -> Result<()> {
        use tokio::io::AsyncWriteExt;
        AsyncWriteExt::write_all(self, src).await
    }
}

#[cfg(feature = "deadpool")]
pub struct ConnectionManager<T> {
    config: Config,
    _marker: std::marker::PhantomData<fn(T)>,
}

#[cfg(feature = "deadpool")]
impl<T: TransportSend> deadpool::managed::Manager for ConnectionManager<T> {
    type Type = Connection<T>;

    type Error = std::io::Error;

    async fn create(&self) -> Result<Self::Type> {
        Connection::connect(&self.config).await
    }

    async fn recycle(
        &self,
        conn: &mut Self::Type,
        _: &deadpool::managed::Metrics,
    ) -> deadpool::managed::RecycleResult<Self::Error> {
        if self
            .config
            .max_buffer_capacity
            .is_some_and(|max| conn.buf.capacity() > max)
        {
            conn.buf = Vec::new();
        }
        // TODO: reset transaction state and wait for ReadyForQuery
        Ok(())
    }
}
