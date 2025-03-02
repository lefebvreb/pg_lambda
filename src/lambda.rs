use std::io::Result;
use std::marker::PhantomData;

pub use pg_lambda_macros::pg_lambda;

use crate::connection::params::QueryParams;
use crate::connection::result::{FromQueryResult, QueryResult};
use crate::connection::{Connection, SyncTransport, Transport};

pub struct PgLambda<P, R> {
    statement: &'static str,
    params: P,
    _marker: PhantomData<R>,
}

impl<P, R> PgLambda<P, R> {
    pub(crate) fn new(statement: &'static str, params: P) -> Self {
        Self {
            statement,
            params,
            _marker: PhantomData,
        }
    }
}

impl<P, R> PgLambda<P, R>
where
    P: QueryParams,
    R: QueryResult,
{
    pub async fn execute<'a, U, T>(self, conn: &'a mut Connection<T>) -> Result<U>
    where
        U: FromQueryResult<'a, R, T>,
        T: Transport,
    {
        conn.query(self.statement, self.params).await
    }

    pub fn execute_sync<'a, U, T>(self, conn: &'a mut Connection<T>) -> Result<U>
    where
        U: FromQueryResult<'a, R, T>,
        T: SyncTransport,
    {
        conn.query_sync(self.statement, self.params)
    }
}
