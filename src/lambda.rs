use std::marker::PhantomData;

use tokio_postgres::types::ToSql;

use crate::connection::AsyncConnection;
use crate::types::{FromQueryResult, PgType};

pub use pg_lambda_macros::pg_lambda;

pub enum PgLambdaError {
    Protocol(tokio_postgres::Error),
}

pub struct PgLambda<'a, T> {
    pub(crate) statement: &'static str,
    // in the future: ptr + fn(ptr, &mut Vec<u8>)
    pub(crate) params: Box<[&'a (dyn ToSql + Sync)]>,
    pub(crate) _marker: PhantomData<T>,
}

impl<T: PgType> PgLambda<'_, T> {
    pub async fn call_async<'a, U: FromQueryResult<'a, T>>(
        self,
        client: &'a mut AsyncConnection,
    ) -> Result<U, PgLambdaError> {
        client.rows = client
            .inner
            .query(self.statement, &self.params)
            .await
            .map_err(PgLambdaError::Protocol)?;
        U::from_rows(&client.rows)
    }
}
