use std::marker::PhantomData;

use tokio_postgres::types::ToSql;
use tokio_postgres::{Client, Row};

use crate::types::{FromPg, PgType};

pub struct AsyncConnection {
    inner: Client,
    rows: Vec<Row>,
}

pub struct PgLambda<'a, T> {
    statement: &'static str,
    params: &'a [&'a (dyn ToSql + Sync)],
    _marker: PhantomData<&'a T>,
}

pub enum PgLambdaError {
    Protocol(tokio_postgres::Error),
}

impl<'a, T> PgLambda<'a, T> {
    #[doc(hidden)]
    pub fn new(statement: &'static str, params: &'a [&'a (dyn ToSql + Sync)]) -> Self {
        Self {
            statement,
            params,
            _marker: PhantomData,
        }
    }
}

impl<T: PgType> PgLambda<'_, T> {
    pub async fn call_async<'a, U: FromPg<'a, T>>(self, client: &'a mut AsyncConnection) -> Result<U, PgLambdaError> {
        client.rows = client
            .inner
            .query(self.statement, self.params)
            .await
            .map_err(PgLambdaError::Protocol)?;

        todo!()
    }
}
