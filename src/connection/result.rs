use std::io::{Error, ErrorKind, Result};
use std::iter::FusedIterator;
use std::marker::PhantomData;

use crate::types::{FromPgValue, PgType};

use super::messages::DataRow;
use super::util::{read_i32, read_slice};
use super::{QueryStream, Transport};

pub struct Row<'a> {
    pub(crate) inner: DataRow<'a>,
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
    fn from_row(mut row: Row) -> Result<Self> {
        match row.next() {
            Some(_) => Err(Error::new(
                ErrorKind::InvalidData,
                "unexpected an empty row",
            )),
            None => Ok(()),
        }
    }
}

impl<'a, T0: PgType, U0: FromPgValue<'a, T0>> FromRow<'a, (T0,)> for (U0,) {
    fn from_row(mut row: Row<'a>) -> Result<Self> {
        let u0 = match row.next() {
            Some(Ok(Some(mut src))) => U0::read(&mut src),
            Some(Ok(None)) => U0::null(),
            Some(Err(err)) => return Err(err),
            None => panic!(),
        }?;
        Ok((u0,))
    }
}

// more tuple impls...

impl<'a, R> FromRow<'a, R> for Row<'a> {
    fn from_row(row: Row<'a>) -> Result<Self> {
        Ok(row)
    }
}

// Derive for custom types

// ----------------- QueryResult

pub trait QueryResult: 'static {
    type Row;
}

/// Marker for queries that are supposed to return nothing.
pub struct Void;

impl QueryResult for Void {
    type Row = ();
}

/// Marker for queries that are supposed to return a single row.
pub struct Single<R: 'static>(PhantomData<R>);

impl<R> QueryResult for Single<R> {
    type Row = R;
}

/// Marker for queries that are supposed to return a set of rows.
pub struct SetOf<R: 'static>(PhantomData<R>);

impl<R> QueryResult for SetOf<R> {
    type Row = R;
}

// ----------------- FromQueryResult

pub trait FromQueryResult<'a, R: QueryResult, T: Transport>: Sized + 'a {
    type Row: FromRow<'a, R::Row>;

    #[allow(async_fn_in_trait)]
    async fn from_stream(stream: &'a mut QueryStream<R::Row, Self::Row, T>) -> Result<Self>;
}

impl<T: Transport> FromQueryResult<'_, Void, T> for () {
    type Row = ();

    async fn from_stream(_: &mut QueryStream<(), (), T>) -> Result<Self> {
        Ok(())
    }
}

impl<'a, R, U: FromRow<'a, R>, T: Transport> FromQueryResult<'a, Single<R>, T> for U {
    type Row = U;

    async fn from_stream(stream: &'a mut QueryStream<R, U, T>) -> Result<Self> {
        // todo: figure out a way to pull the first None that comes right after this
        match stream.next().await? {
            Some(val) => Ok(val),
            None => Err(Error::new(
                ErrorKind::UnexpectedEof,
                "unexpected empty query result",
            )),
        }
    }
}

impl<R, U: for<'x> FromRow<'x, R>, T: Transport> FromQueryResult<'_, SetOf<R>, T> for Vec<U> {
    type Row = U;

    async fn from_stream(stream: &mut QueryStream<R, U, T>) -> Result<Self> {
        let mut res = Vec::new();
        while let Some(val) = stream.next().await? {
            res.push(val);
        }
        Ok(res)
    }
}

impl<'a, R, U: FromRow<'a, R>, T: Transport> FromQueryResult<'a, SetOf<R>, T>
    for &'a mut QueryStream<R, U, T>
{
    type Row = U;

    async fn from_stream(stream: &'a mut QueryStream<R, U, T>) -> Result<Self> {
        Ok(stream)
    }
}
