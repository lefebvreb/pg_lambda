use std::io::{Error, ErrorKind, Read, Result, Write};
use std::marker::PhantomData;
use std::str;

use crate::connection::{QueryStream, Row, Transport};

pub trait PgType: 'static {
    // Have a create type and a raw type
    const SQL_NAME: &str;
}

pub trait ToPgValue<T: PgType> {
    fn is_null(&self) -> bool {
        false
    }

    fn write(&self, dst: &mut Vec<u8>) -> Result<()>;
}

pub trait FromPgValue<'a, T: PgType>: Sized + 'a {
    fn null() -> Result<Self> {
        Err(Error::new(ErrorKind::InvalidData, "unexpected null value"))
    }

    fn read(src: &mut &'a [u8]) -> Result<Self>;
}

pub struct Int4;

impl PgType for Int4 {
    const SQL_NAME: &str = "INT4";
}

pub type Integer = Int4;

pub type Int = Int4;

impl ToPgValue<Int4> for i32 {
    fn write(&self, dst: &mut Vec<u8>) -> Result<()> {
        dst.write_all(&self.to_be_bytes())
    }
}

impl FromPgValue<'_, Int4> for i32 {
    fn read(src: &mut &'_ [u8]) -> Result<Self> {
        let mut buf = [0; 4];
        src.read_exact(&mut buf)?;
        Ok(i32::from_be_bytes(buf))
    }
}

pub struct Text;

impl PgType for Text {
    const SQL_NAME: &str = "TEXT";
}

impl ToPgValue<Text> for &str {
    fn write(&self, dst: &mut Vec<u8>) -> Result<()> {
        dst.write_all(self.as_bytes())
    }
}

impl<'a> FromPgValue<'a, Text> for &'a str {
    fn read(src: &mut &'a [u8]) -> Result<Self> {
        str::from_utf8(src).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("string is not valid utf8: {e}"),
            )
        })
    }
}

impl<'a> FromPgValue<'a, Text> for String {
    fn read(src: &mut &'a [u8]) -> Result<Self> {
        <&str as FromPgValue<Text>>::read(src).map(ToOwned::to_owned)
    }
}

// pub struct Nullable<T: PgType>(PhantomData<T>);

// impl<T: PgType> PgType for Nullable<T> {
//     const SQL_NAME: &str = "???";
// }

// ----------------- FromRow

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
