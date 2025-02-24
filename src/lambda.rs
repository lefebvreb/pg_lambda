use std::io::Result;
use std::marker::PhantomData;

pub use pg_lambda_macros::pg_lambda;

use crate::connection::{Connection, SyncTransport, Transport};
use crate::types::FromQueryResult;

pub struct PgLambda<P, R> {
    pub(crate) statement: &'static str,
    pub(crate) write_params: P,
    pub(crate) _marker: PhantomData<R>,
}

impl<P, R> PgLambda<P, R>
where
    P: Fn(&mut Vec<u8>) -> Result<()>,
{
    pub async fn call<'a, U: FromQueryResult<'a, R>>(
        self,
        conn: &'a mut Connection<impl Transport>,
    ) -> Result<U> {
        conn.query(self.statement, self.write_params).await
    }

    pub fn call_sync<'a, T: FromQueryResult<'a, R>>(
        self,
        conn: &'a mut Connection<impl SyncTransport>,
    ) -> Result<T::SyncOutput> {
        conn.query_sync::<R, T>(self.statement, self.write_params)
    }
}
