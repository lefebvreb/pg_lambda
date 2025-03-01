use std::io::{Read, Result, Write};
use std::net::TcpStream;

use super::{Config, SyncTransport, TransportSend};

impl TransportSend for TcpStream {
    async fn connect(config: &Config) -> Result<Self> {
        Self::connect(&config.host)
    }

    async fn read(&mut self, dst: &mut [u8]) -> Result<i32> {
        Read::read(self, dst).map(|count| count as i32)
    }

    async fn write(&mut self, src: &[u8]) -> Result<i32> {
        Write::write(self, src).map(|count| count as i32)
    }
}

impl SyncTransport for TcpStream {}

#[cfg(feature = "tokio")]
impl TransportSend for tokio::net::TcpStream {
    async fn connect(config: &Config) -> Result<Self> {
        Self::connect(&config.host).await
    }

    async fn read(&mut self, dst: &mut [u8]) -> Result<i32> {
        tokio::io::AsyncReadExt::read(self, dst)
            .await
            .map(|count| count as i32)
    }

    async fn write(&mut self, src: &[u8]) -> Result<i32> {
        tokio::io::AsyncWriteExt::write(self, src)
            .await
            .map(|count| count as i32)
    }
}

#[cfg(feature = "bb8")]
impl<T: TransportSend + 'static> bb8::ManageConnection for super::ConnectionManager<T> {
    type Connection = super::Connection<T>;

    type Error = std::io::Error;

    async fn connect(&self) -> Result<Self::Connection> {
        super::Connection::connect(&self.config).await
    }

    async fn is_valid(&self, _: &mut Self::Connection) -> Result<()> {
        // conn.query::<Void, ()>("", NoParams).await
        Ok(())
    }

    fn has_broken(&self, _: &mut Self::Connection) -> bool {
        false
    }
}

#[cfg(feature = "deadpool")]
impl<T: TransportSend> deadpool::managed::Manager for super::ConnectionManager<T> {
    type Type = super::Connection<T>;

    type Error = std::io::Error;

    async fn create(&self) -> Result<Self::Type> {
        super::Connection::connect(&self.config).await
    }

    async fn recycle(
        &self,
        _: &mut Self::Type,
        _: &deadpool::managed::Metrics,
    ) -> deadpool::managed::RecycleResult<Self::Error> {
        Ok(())
    }
}

#[cfg(feature = "r2d2")]
impl<T: TransportSend + SyncTransport + 'static> r2d2::ManageConnection
    for super::ConnectionManager<T>
{
    type Connection = super::Connection<T>;

    type Error = std::io::Error;

    fn connect(&self) -> Result<Self::Connection> {
        use futures::FutureExt;
        super::Connection::connect(&self.config)
            .now_or_never()
            .expect("transport marked as sync should not use awaits")
    }

    fn is_valid(&self, conn: &mut Self::Connection) -> Result<()> {
        conn.query_sync::<super::result::Void, ()>("", super::params::NoParams)
    }

    fn has_broken(&self, _: &mut Self::Connection) -> bool {
        false
    }
}
