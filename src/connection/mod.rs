use std::io::Result;
use std::iter::FusedIterator;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::Stream;
use messages::DataRow;
use util::{read_i32, read_slice};

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

pub struct Config {
    pub user: String,
    pub dbname: String,
    pub host: String,
    pub max_buffer_capacity: Option<usize>,
}

pub struct Connection<T> {
    transport: T,
    buffer: Vec<u8>,
    max_buffer_capacity: Option<usize>,
}

impl<T: Transport> Connection<T> {
    pub async fn connect(config: &Config) -> Result<Self> {
        Ok(Self {
            transport: T::connect(config).await?,
            buffer: Vec::new(),
            max_buffer_capacity: config.max_buffer_capacity,
        })
    }

    pub async fn extended_query<'a>(
        &'a mut self,
        statement: &str,
        write_params: impl Fn(&mut Vec<u8>) -> Result<()>,
    ) -> impl Stream<Item = Result<Row<'a>>> + Unpin {
        futures::stream::empty()
    }

    async fn recycle(&mut self) {
        if self
            .max_buffer_capacity
            .is_some_and(|max| self.buffer.capacity() > max)
        {
            self.buffer = Vec::new();
        }
    }
}

#[cfg(any(feature = "bb8", feature = "deadpool", feature = "r2d2"))]
pub struct ConnectionManager<T> {
    config: Config,
    _marker: std::marker::PhantomData<fn(T)>,
}

pub struct ExtendedQuery<'a, T> {
    conn: &'a mut Connection<T>,
}

impl<'a, T> Stream for ExtendedQuery<'a, T> {
    type Item = Result<Row<'a>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        todo!()
    }
}
