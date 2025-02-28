use std::io::Result;
use std::marker::PhantomData;

pub use pg_lambda_macros::pg_lambda;

use crate::connection::{Connection, SyncTransport, Transport};
use crate::types::{FromQueryResult, QueryResult};

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
    P: Fn(&mut Vec<u8>) -> Result<()>,
    R: QueryResult,
{
    pub async fn call<'a, U, T>(self, conn: &'a mut Connection<T>) -> Result<U>
    where
        U: FromQueryResult<'a, R, T>,
        T: Transport,
    {
        conn.query(self.statement, self.params).await
    }

    pub fn call_sync<'a, U, T>(self, conn: &'a mut Connection<T>) -> Result<U>
    where
        U: FromQueryResult<'a, R, T>,
        T: SyncTransport,
    {
        conn.query_sync::<_, U>(self.statement, self.params)
    }
}
