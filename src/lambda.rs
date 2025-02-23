use std::io::Result;
use std::marker::PhantomData;

use futures::FutureExt;
pub use pg_lambda_macros::pg_lambda;

use crate::connection::{Connection, SyncTransport, Transport};
use crate::types::FromQueryResult;

pub struct PgLambda<F, R> {
    pub(crate) statement: &'static str,
    pub(crate) args: F,
    pub(crate) _marker: PhantomData<R>,
}

impl<F, R> PgLambda<F, R>
where
    F: Fn(&mut Vec<u8>) -> Result<()>,
{
    pub async fn call_async<'a, T: FromQueryResult<'a, R>>(
        self,
        conn: &'a mut Connection<impl Transport>,
    ) -> Result<T> {
        todo!()
    }

    pub fn call<'a, T: FromQueryResult<'a, R>>(
        self,
        conn: &'a mut Connection<impl SyncTransport>,
    ) -> Result<T> {
        self.call_async(conn)
            .now_or_never()
            .expect("transport was not async")
    }
}

// pub struct PgLambda<'a, T> {
//     pub(crate) statement: &'static str,
//     // in the future: ptr + fn(ptr, &mut Vec<u8>)
//     // pub(crate) params: Box<[&'a (dyn ToSql + Sync)]>,
//     pub(crate) _marker: PhantomData<&'a T>,
// }

// impl<T: PgType> PgLambda<'_, T> {
//     pub async fn call_async<'a, U: FromQueryResult<'a, T>>(
//         self,
//         conn: &'a mut SyncConnection,
//     ) -> Result<U> {
//         // conn.rows = conn
//         //     .inner
//         //     .query(self.statement, &self.params)
//         //     .await
//         //     .map_err(PgLambdaError::Protocol)?;
//         // U::from_rows(&conn.rows)
//         todo!()
//     }
// }
