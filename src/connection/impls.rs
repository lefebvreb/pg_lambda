use std::io::{Read, Result, Write};
use std::net::TcpStream;

use futures::FutureExt;

use super::{Config, SyncTransport, TransportSend};

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

#[cfg(feature = "bb8")]
impl<T: TransportSend + 'static> bb8::ManageConnection for super::ConnectionManager<T> {
    type Connection = super::Connection<T>;

    type Error = std::io::Error;

    async fn connect(&self) -> Result<Self::Connection> {
        super::Connection::connect(&self.config).await
    }

    async fn is_valid(&self, _: &mut Self::Connection) -> Result<()> {
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
        super::Connection::connect(&self.config)
            .now_or_never()
            .expect("transport marked as sync should not use awaits")
    }

    fn is_valid(&self, _: &mut Self::Connection) -> Result<()> {
        Ok(())
    }

    fn has_broken(&self, _: &mut Self::Connection) -> bool {
        false
    }
}
