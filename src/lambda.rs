use std::io::Result;
use std::marker::PhantomData;

pub use pg_lambda_macros::pg_lambda;

use crate::connection::{Connection, SyncTransport, Transport};
use crate::types::FromQueryResult;

pub struct PgLambda<F, R> {
    pub(crate) statement: &'static str,
    pub(crate) write_params: F,
    pub(crate) _marker: PhantomData<R>,
}

impl<F, R> PgLambda<F, R>
where
    F: Fn(&mut Vec<u8>) -> Result<()>,
{
    pub async fn call<'a, T: FromQueryResult<'a, R>>(
        self,
        conn: &'a mut Connection<impl Transport>,
    ) -> Result<T> {
        conn.query(self.statement, self.write_params).await
    }

    pub fn call_sync<'a, T: FromQueryResult<'a, R>>(
        self,
        conn: &'a mut Connection<impl SyncTransport>,
    ) -> Result<T::SyncOutput> {
        conn.query_sync::<R, T>(self.statement, self.write_params)
    }
}
