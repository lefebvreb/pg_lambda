use std::io::{self, Read, Write};
use std::net::TcpStream;

use futures::Stream;
use messages::DataRow;
use util::{read_i32, read_slice};

mod messages;
mod util;

pub(crate) trait Transport {
    async fn read_exact(&mut self, limit: i32, dst: &mut Vec<u8>) -> io::Result<()>;

    async fn write_all(&mut self, src: &[u8]) -> io::Result<()>;
}

pub struct Config {
    pub user: String,
    pub dbname: String,
    pub host: String,
}

pub(crate) struct GenericConnection<T> {
    transport: T,
    buf: Vec<u8>,
}

impl<T: Transport> GenericConnection<T> {
    async fn connect(config: &Config, transport: T) -> Self {
        Self {
            transport,
            buf: Vec::new(),
        }
    }

    pub(crate) async fn query<'a>(&mut self, statement: &str, params: ()) -> impl Stream<Item = Row<'a>> {
        futures::stream::empty()
    }
}

pub struct Row<'a> {
    inner: DataRow<'a>,
}

impl<'a> Iterator for Row<'a> {
    type Item = io::Result<Option<&'a [u8]>>;

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

impl Transport for TcpStream {
    async fn read_exact(&mut self, limit: i32, dst: &mut Vec<u8>) -> io::Result<()> {
        Read::take(self, limit as u64).read_to_end(dst).map(drop)
    }

    async fn write_all(&mut self, src: &[u8]) -> io::Result<()> {
        Write::write_all(self, src)
    }
}

pub struct SyncConnection(GenericConnection<TcpStream>);

// #[cfg(feature = "deadpool")]
// impl deadpool::managed::Manager for AsyncConnectionManager {
//     type Type = AsyncConnection;

//     type Error = AsyncConnectionManagerError;

//     async fn create(&self) -> Result<Self::Type, Self::Error> {
//         let (client, conn) =
//             tokio_postgres::connect(&self.connection_url, tokio_postgres::NoTls).await?;
//         tokspawn(conn);
//         Ok(AsyncConnection {
//             inner: client,
//             rows: Vec::default(),
//         })
//     }

//     async fn recycle(
//         &self,
//         _: &mut Self::Type,
//         _: &deadpool::managed::Metrics,
//     ) -> deadpool::managed::RecycleResult<Self::Error> {
//         Ok(())
//     }
// }
