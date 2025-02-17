use std::marker::PhantomData;

use tokio_postgres::types::ToSql;
use tokio_postgres::{Client, Row};

use crate::error::PgLambdaError;
use crate::types::{FromQueryResult, PgType};

pub struct AsyncConnection {
    inner: Client,
    rows: Vec<Row>,
}

pub struct PgLambda<'a, T> {
    statement: &'static str,
    // in the future: ptr + fn(ptr, &mut Vec<u8>)
    params: Box<[&'a (dyn ToSql + Sync)]>,
    _marker: PhantomData<T>,
}

impl<'a, T> PgLambda<'a, T> {
    pub fn new(statement: &'static str, params: Box<[&'a (dyn ToSql + Sync)]>) -> Self {
        Self {
            statement,
            params,
            _marker: PhantomData,
        }
    }
}

impl<T: PgType> PgLambda<'_, T> {
    pub async fn call_async<'a, U: FromQueryResult<'a, T>>(self, client: &'a mut AsyncConnection) -> Result<U, PgLambdaError> {
        client.rows = client
            .inner
            .query(self.statement, &self.params)
            .await
            .map_err(PgLambdaError::Protocol)?;
        U::from_rows(&client.rows)
    }
}

#[doc(hidden)]
pub struct PgLambdaDef {
    pub name: &'static str,
    pub create_statement: &'static str,
}

inventory::collect!(PgLambdaDef);
