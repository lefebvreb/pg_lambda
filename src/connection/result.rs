use std::io::{Error, ErrorKind, Result};
use std::iter::FusedIterator;
use std::marker::PhantomData;
use std::mem::transmute;

use futures::FutureExt;

use crate::types::{FromPgValue, PgType};

use super::messages::DataRow;
use super::util::{read_i32, read_slice};
use super::{Connection, SyncTransport, Transport};

pub struct Row<'a> {
    inner: DataRow<'a>,
}

impl<'a> Row<'a> {
    pub(crate) fn new(data: DataRow<'a>) -> Self {
        Self { inner: data }
    }
}

impl<'a> Iterator for Row<'a> {
    type Item = Result<Option<&'a [u8]>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.inner.len == 0 {
            return None;
        }
        self.inner.len -= 1;
        Some(match read_i32(&mut self.inner.columns) {
            Ok(-1) => Ok(None),
            Ok(len) => read_slice(len, &mut self.inner.columns).map(Some),
            Err(err) => Err(err),
        })
    }
}

impl FusedIterator for Row<'_> {}

pub trait FromRow<'a, R>: Sized + 'a {
    fn from_row(row: Row<'a>) -> Result<Self>;
}

impl FromRow<'_, ()> for () {
    fn from_row(_: Row) -> Result<Self> {
        Ok(())
    }
}

fn next_column<'a, T: PgType, U: FromPgValue<'a, T>>(row: &mut Row<'a>) -> Result<U> {
    match row.next() {
        Some(Ok(Some(src))) => U::read(src),
        Some(Ok(None)) => U::null(),
        Some(Err(err)) => Err(err),
        None => Err(Error::new(ErrorKind::InvalidData, "unexpected end of row")),
    }
}

macro_rules! impl_from_row {
    {
        $($t:ident $u:ident)*
    } => {
        impl<'a, $($t: PgType, $u: FromPgValue<'a, $t>,)*> FromRow<'a, ($($t,)*)> for ($($u,)*) {
            fn from_row(mut row: Row<'a>) -> Result<Self> {
                Ok((
                    $(
                        next_column::<$t, $u>(&mut row)?,
                    )*
                ))
            }
        }
    };
}

macro_rules! for_all_tuples {
    ($mac:ident) => {
        $mac! { T0 U0 }
        $mac! { T0 U0 T1 U1 }
        $mac! { T0 U0 T1 U1 T2 U2 }
        $mac! { T0 U0 T1 U1 T2 U2 T3 U3 }
        $mac! { T0 U0 T1 U1 T2 U2 T3 U3 T4 U4 }
        $mac! { T0 U0 T1 U1 T2 U2 T3 U3 T4 U4 T5 U5 }
        $mac! { T0 U0 T1 U1 T2 U2 T3 U3 T4 U4 T5 U5 T6 U6 }
        $mac! { T0 U0 T1 U1 T2 U2 T3 U3 T4 U4 T5 U5 T6 U6 T7 U7 }
        $mac! { T0 U0 T1 U1 T2 U2 T3 U3 T4 U4 T5 U5 T6 U6 T7 U7 T8 U8 }
        $mac! { T0 U0 T1 U1 T2 U2 T3 U3 T4 U4 T5 U5 T6 U6 T7 U7 T8 U8 T9 U9 }
        $mac! { T0 U0 T1 U1 T2 U2 T3 U3 T4 U4 T5 U5 T6 U6 T7 U7 T8 U8 T9 U9 T10 U10 }
        $mac! { T0 U0 T1 U1 T2 U2 T3 U3 T4 U4 T5 U5 T6 U6 T7 U7 T8 U8 T9 U9 T10 U10 T11 U11 }
        $mac! { T0 U0 T1 U1 T2 U2 T3 U3 T4 U4 T5 U5 T6 U6 T7 U7 T8 U8 T9 U9 T10 U10 T11 U11 T12 U12 }
    };
}

for_all_tuples!(impl_from_row);

impl<'a, R> FromRow<'a, R> for Row<'a> {
    fn from_row(row: Row<'a>) -> Result<Self> {
        Ok(row)
    }
}

// todo: make a FromRow derive for struct types

/// Marker for queries that are supposed to return nothing.
pub struct Void;

pub trait FromQueryResult<'a, R, T: Transport>: Sized + 'a {
    #[allow(async_fn_in_trait)]
    async fn from_conn(conn: &'a mut Connection<T>) -> Result<Self>;
}

impl<T: Transport> FromQueryResult<'_, Void, T> for () {
    async fn from_conn(_: &mut Connection<T>) -> Result<Self> {
        Ok(())
    }
}

/// Marker for queries that are supposed to return a single row.
pub struct Single<R: 'static>(PhantomData<R>);

impl<'a, R, U: FromRow<'a, R>, T: Transport> FromQueryResult<'a, Single<R>, T> for U {
    async fn from_conn(conn: &'a mut Connection<T>) -> Result<Self> {
        // // todo: figure out a way to pull the first None that comes right after this
        // match stream.next().await? {
        //     Some(val) => Ok(val),
        //     None => Err(Error::new(
        //         ErrorKind::UnexpectedEof,
        //         "unexpected empty query result",
        //     )),
        // }
        todo!()
    }
}

/// Marker for queries that are supposed to return a set of rows.
pub struct SetOf<R: 'static>(PhantomData<R>);

impl<'a, R, U: FromRow<'a, R>, T: Transport> FromQueryResult<'a, SetOf<R>, T> for Vec<U> {
    async fn from_conn(conn: &'a mut Connection<T>) -> Result<Self> {
        // let mut res = Vec::new();
        // while let Some(val) = stream.next().await? {
        //     res.push(val);
        // }
        // Ok(res)
        todo!()
    }
}

/// A stream of rows produced as a result of a query.
///
/// # Why is this not a regular [`Stream`](futures::stream::Stream)?
///
/// Values returned by [`QueryStream`] hold a reference to the buffer that
/// is inside the underlying [`Connection`]. This pattern in rust is
/// called a "lending iterator", and is not well supported by the Rust ecosystem.
#[repr(transparent)]
pub struct QueryStream<R, U, T> {
    inner: Connection<T>,
    _marker: PhantomData<(R, U)>,
}

impl<'a, R, U: FromRow<'a, R>, T: Transport> QueryStream<R, U, T> {
    fn new(conn: &mut Connection<T>) -> &mut Self {
        // SAFETY: `QueryStream<R, U, T>` is a `#[repr(transparent)]` wrapper over
        // a `Connection<T>`, it is therefore safe to transmute a mutable reference
        // of one into a mutable reference of the other.
        unsafe { transmute::<&mut Connection<T>, &mut QueryStream<R, U, T>>(conn) }
    }

    pub async fn next(&'a mut self) -> Result<Option<U>> {
        self.inner.buffer.clear();
        self.inner.next_row().await
    }

    // pub async fn collect(&mut self) -> Result<Vec<U>>
    // where
    //     U: 'static,
    // {
    //     todo!()
    // }
}

impl<'a, R, U: FromRow<'a, R>, T: Transport> FromQueryResult<'a, SetOf<R>, T>
    for &'a mut QueryStream<R, U, T>
{
    async fn from_conn(conn: &'a mut Connection<T>) -> Result<Self> {
        Ok(QueryStream::new(conn))
    }
}

#[repr(transparent)]
pub struct QueryIter<R, U, T> {
    inner: QueryStream<R, U, T>,
}

impl<'a, R, U: FromRow<'a, R>, T: Transport> QueryIter<R, U, T> {
    fn new(conn: &mut Connection<T>) -> &mut Self {
        // SAFETY: `QueryIter<R, U, T>` is a `#[repr(transparent)]` wrapper over
        // a `Connection<T>`, it is therefore safe to transmute a mutable reference
        // of one into a mutable reference of the other.
        unsafe { transmute::<&mut Connection<T>, &mut QueryIter<R, U, T>>(conn) }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&'a mut self) -> Result<Option<U>> {
        self.inner
            .next()
            .now_or_never()
            .expect("future should resolve immediately")
    }
}

impl<'a, R, U: FromRow<'a, R>, T: SyncTransport> FromQueryResult<'a, SetOf<R>, T>
    for &'a mut QueryIter<R, U, T>
{
    async fn from_conn(conn: &'a mut Connection<T>) -> Result<Self> {
        Ok(QueryIter::new(conn))
    }
}
