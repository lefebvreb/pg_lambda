use std::io::{Error, ErrorKind, Result};
use std::iter::{FusedIterator, from_fn};
use std::marker::PhantomData;
use std::mem::transmute;

use futures::stream::unfold;
use futures::{FutureExt, Stream};

use crate::types::{FromPgValue, PgType};

use super::messages::DataRow;
use super::{Connection, SyncTransport, Transport};
use crate::util::{read_i32, read_slice};

pub struct Row<'a> {
    inner: DataRow<'a>,
}

impl<'a> Row<'a> {
    pub(crate) fn new(data: DataRow<'a>) -> Self {
        Self { inner: data }
    }

    fn next_parsed<T, U>(&mut self) -> Result<U>
    where
        T: PgType,
        U: FromPgValue<'a, T>,
    {
        match self.next() {
            Some(Ok(Some(src))) => U::read(src),
            Some(Ok(None)) => U::null(),
            Some(Err(err)) => Err(err),
            None => Err(Error::new(ErrorKind::InvalidData, "unexpected end of row")),
        }
    }
}

impl<'a> Iterator for Row<'a> {
    type Item = Result<Option<&'a [u8]>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.len() == 0 {
            return None;
        }
        self.inner.len -= 1;
        Some(match read_i32(&mut self.inner.columns) {
            Ok(-1) => Ok(None),
            Ok(len) => read_slice(len, &mut self.inner.columns).map(Some),
            Err(err) => Err(err),
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl ExactSizeIterator for Row<'_> {
    fn len(&self) -> usize {
        self.inner.len as usize
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

impl<'a, T, U> FromRow<'a, T> for U
where
    T: PgType,
    U: FromPgValue<'a, T>,
{
    fn from_row(mut row: Row<'a>) -> Result<Self> {
        row.next_parsed()
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
                        row.next_parsed::<$t, $u>()?,
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

// todo: make a FromRow derive for struct types

/// Marker for queries that are supposed to return nothing.
pub struct Void;

pub trait FromQueryResult<'a, R, T: Transport>: Sized + 'a {
    #[allow(async_fn_in_trait)]
    async fn from_conn(conn: &'a mut Connection<T>) -> Result<Self>;
}

impl<T: Transport> FromQueryResult<'_, Void, T> for () {
    async fn from_conn(conn: &mut Connection<T>) -> Result<Self> {
        conn.next_row()
            .await?
            .is_none()
            .then_some(())
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "unexpected non-empty query result"))
    }
}

/// Marker for queries that are supposed to return a single row.
pub struct Single<R>(PhantomData<R>);

impl<'a, R, U: FromRow<'a, R>, T: Transport> FromQueryResult<'a, Single<R>, T> for U {
    async fn from_conn(conn: &'a mut Connection<T>) -> Result<Self> {
        let msg = conn
            .next_row()
            .await?
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "unexpected empty query result"))?;
        if conn.next_row().await?.is_some() {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "unexpected second row in query result",
            ));
        }
        conn.parse_row(&msg)
    }
}

/// Marker for queries that are supposed to return a set of rows.
pub struct SetOf<R>(PhantomData<R>);

impl<'a, R, U: FromRow<'a, R>, T: Transport> FromQueryResult<'a, SetOf<R>, T> for Vec<U> {
    async fn from_conn(conn: &'a mut Connection<T>) -> Result<Self> {
        let mut res = Vec::new();
        while let Some(msg) = conn.next_row().await? {
            res.push(msg);
        }
        res.into_iter().map(|msg| conn.parse_row(&msg)).collect()
    }
}

/// A stream of rows produced as a result of a query.
///
/// # Why is this not a regular [`Stream`]?
///
/// Values returned by [`QueryStream`] hold a reference to the buffer that
/// is inside the underlying [`Connection`]. This pattern in rust is
/// called a "lending iterator", and is not well supported by the Rust ecosystem.
#[repr(transparent)]
pub struct QueryStream<R, U, T> {
    inner: Connection<T>,
    _marker: PhantomData<(R, U)>,
}

impl<R, U, T: Transport> QueryStream<R, U, T> {
    fn new(conn: &mut Connection<T>) -> &mut Self {
        // SAFETY: `QueryStream<R, U, T>` is a `#[repr(transparent)]` wrapper over
        // a `Connection<T>`, it is therefore safe to transmute a mutable reference
        // of one into a mutable reference of the other.
        unsafe { transmute::<&mut Connection<T>, &mut QueryStream<R, U, T>>(conn) }
    }

    pub async fn next<'a>(&'a mut self) -> Option<Result<U>>
    where
        U: FromRow<'a, R>,
    {
        if !self.inner.transport.has_partial_data() {
            self.inner.transport.clear();
        }

        match self.inner.next_row().await {
            Ok(Some(msg)) => Some(self.inner.parse_row(&msg)),
            Ok(None) => None,
            Err(err) => Some(Err(err)),
        }
    }

    pub fn into_std_stream(&mut self) -> impl Stream<Item = Result<U>>
    where
        U: for<'x> FromRow<'x, R>,
    {
        unfold(self, move |this| async {
            this.next().await.map(|val| (val, this))
        })
    }
}

impl<'a, R, U: FromRow<'a, R>, T: Transport> FromQueryResult<'a, SetOf<R>, T>
    for &'a mut QueryStream<R, U, T>
{
    async fn from_conn(conn: &'a mut Connection<T>) -> Result<Self> {
        Ok(QueryStream::new(conn))
    }
}

/// An iterator over rows produced as a result of a query.
///
/// # Why is this not a regular [`Iterator`]?
///
/// Values returned by [`QueryIter`] hold a reference to the buffer that
/// is inside the underlying [`Connection`]. This pattern in rust is
/// called a "lending iterator", and is not well supported by the Rust ecosystem.
#[repr(transparent)]
pub struct QueryIter<R, U, T> {
    inner: QueryStream<R, U, T>,
}

impl<R, U, T: SyncTransport> QueryIter<R, U, T> {
    fn new(conn: &mut Connection<T>) -> &mut Self {
        // SAFETY: `QueryIter<R, U, T>` is a `#[repr(transparent)]` wrapper over
        // a `Connection<T>`, it is therefore safe to transmute a mutable reference
        // of one into a mutable reference of the other.
        unsafe { transmute::<&mut Connection<T>, &mut QueryIter<R, U, T>>(conn) }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next<'a>(&'a mut self) -> Option<Result<U>>
    where
        U: FromRow<'a, R>,
    {
        self.inner
            .next()
            .now_or_never()
            .expect("future should resolve immediately")
    }

    pub fn into_std_iter(&mut self) -> impl Iterator<Item = Result<U>>
    where
        U: for<'x> FromRow<'x, R>,
    {
        from_fn(|| self.next())
    }
}

impl<'a, R, U: FromRow<'a, R>, T: SyncTransport> FromQueryResult<'a, SetOf<R>, T>
    for &'a mut QueryIter<R, U, T>
{
    async fn from_conn(conn: &'a mut Connection<T>) -> Result<Self> {
        Ok(QueryIter::new(conn))
    }
}
