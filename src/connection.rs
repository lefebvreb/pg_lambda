use tokio_postgres::{Client, Row};

pub struct AsyncConnection {
    pub(crate) inner: Client,
    pub(crate) rows: Vec<Row>,
}
