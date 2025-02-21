use std::ffi::CString;
use std::io::{self, Read, Write};
use std::net::TcpStream;

use messages::DataRow;

mod messages;
mod util;

trait Transport {
    async fn read_exact(&mut self, dst: &mut Vec<u8>, limit: i32) -> io::Result<()>;

    async fn write_all(&mut self, src: &[u8]) -> io::Result<()>;
}

pub struct Config {
    pub user: CString,
    pub dbname: CString,
    pub host: String,
}

struct GenericConnection<T> {
    transport: T,
}

impl<T: Transport> GenericConnection<T> {
    pub async fn connect(config: Config) -> Self {
        todo!()
    }

    // pub async fn query<'a>(&mut self, statement: &str);
}

pub struct Row<'a> {
    inner: DataRow<'a>,
}

impl<'a> Iterator for Row<'a> {
    type Item = Option<&'a [u8]>;

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
    }
}

impl Transport for TcpStream {
    async fn read_exact(&mut self, dst: &mut Vec<u8>, limit: i32) -> io::Result<()> {
        Read::take(self, limit as u64).read_to_end(dst).map(|_| ())
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
//         tokio::spawn(conn);
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
