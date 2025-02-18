use thiserror::Error;
use tokio_postgres::{Client, Row};

pub struct AsyncConnectionManager {
    connection_url: String,
}

impl AsyncConnectionManager {
    pub fn new(connection_url: impl Into<String>) -> Self {
        Self {
            connection_url: connection_url.into(),
        }
    }
}

#[derive(Error, Debug)]
pub enum AsyncConnectionManagerError {
    #[error("failed to connect to database: {0}")]
    Connect(#[from] tokio_postgres::Error),
}

#[cfg(feature = "deadpool")]
impl deadpool::managed::Manager for AsyncConnectionManager {
    type Type = AsyncConnection;

    type Error = AsyncConnectionManagerError;

    async fn create(&self) -> Result<Self::Type, Self::Error> {
        let (client, conn) =
            tokio_postgres::connect(&self.connection_url, tokio_postgres::NoTls).await?;
        tokio::spawn(conn);
        Ok(AsyncConnection {
            inner: client,
            rows: Vec::default(),
        })
    }

    async fn recycle(
        &self,
        _: &mut Self::Type,
        _: &deadpool::managed::Metrics,
    ) -> deadpool::managed::RecycleResult<Self::Error> {
        Ok(())
    }
}

pub struct AsyncConnection {
    pub(crate) inner: Client,
    pub(crate) rows: Vec<Row>,
}
