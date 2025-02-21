use std::marker::PhantomData;

use crate::connection::SyncConnection;
use crate::error::Result;
use crate::types::{FromQueryResult, PgType};

pub use pg_lambda_macros::pg_lambda;

pub struct PgLambda<'a, T> {
    pub(crate) statement: &'static str,
    // in the future: ptr + fn(ptr, &mut Vec<u8>)
    // pub(crate) params: Box<[&'a (dyn ToSql + Sync)]>,
    pub(crate) _marker: PhantomData<&'a T>,
}

impl<T: PgType> PgLambda<'_, T> {
    pub async fn call_async<'a, U: FromQueryResult<'a, T>>(
        self,
        conn: &'a mut SyncConnection,
    ) -> Result<U> {
        // conn.rows = conn
        //     .inner
        //     .query(self.statement, &self.params)
        //     .await
        //     .map_err(PgLambdaError::Protocol)?;
        // U::from_rows(&conn.rows)
        todo!()
    }
}
