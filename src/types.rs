use std::io::{Error, ErrorKind, Read, Result, Write};
use std::marker::PhantomData;
use std::mem::transmute;
use std::str;

use crate::connection::{Row, RowsStream, Transport};

pub trait PgType {
    // Have a create type and a raw type
    const SQL_NAME: &str;
}

pub trait ToPgValue<T: PgType> {
    fn is_null(&self) -> bool {
        false
    }

    fn write(&self, dst: &mut Vec<u8>) -> Result<()>;
}

pub trait FromPgValue<'a, T: PgType>: Sized {
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

pub trait FromRow<'a, R>: Sized {
    fn from_row(row: Row<'a>) -> Result<Self>;
}

impl FromRow<'_, ()> for () {
    fn from_row(mut row: Row<'_>) -> Result<Self> {
        match row.next() {
            Some(_) => Err(Error::new(
                ErrorKind::InvalidData,
                "unexpected an empty row",
            )),
            None => Ok(()),
        }
    }
}

// Tuple impls...

impl<'a, R> FromRow<'a, R> for Row<'a> {
    fn from_row(row: Row<'a>) -> Result<Self> {
        Ok(row)
    }
}

// Derive for custom types

// See: https://github.com/rust-lang/rust/issues/87479
pub trait FromQueryResult<'a, R, T: Transport>: Sized {
    type SyncOutput: From<Self>;

    #[allow(async_fn_in_trait)]
    async fn from_stream(stream: &'a mut RowsStream<T>) -> Result<Self>;
}

/// Marker for queries that are supposed to return nothing.
pub struct Void;

impl<T: Transport> FromQueryResult<'_, Void, T> for () {
    type SyncOutput = Self;

    async fn from_stream(_: &mut RowsStream<T>) -> Result<Self> {
        Ok(())
    }
}

/// Marker for queries that are supposed to return a single row.
pub struct Single<R>(PhantomData<R>);

impl<'a, R, U: FromRow<'a, R>, T: Transport> FromQueryResult<'a, Single<R>, T> for U {
    type SyncOutput = Self;

    async fn from_stream(stream: &'a mut RowsStream<T>) -> Result<Self> {
        // todo: figure out a way to pull the first None that comes right after this
        match stream.next().await? {
            Some(row) => U::from_row(row),
            None => Err(Error::new(
                ErrorKind::UnexpectedEof,
                "unexpected empty query result",
            )),
        }
    }
}

/// Marker for queries that are supposed to return a set of rows.
pub struct SetOf<R>(PhantomData<R>);

impl<R, U: for<'x> FromRow<'x, R>, T: Transport> FromQueryResult<'_, SetOf<R>, T> for Vec<U> {
    type SyncOutput = Self;

    async fn from_stream(stream: &mut RowsStream<T>) -> Result<Self> {
        let mut res = Vec::new();
        while let Some(row) = stream.next().await? {
            res.push(U::from_row(row)?);
        }
        Ok(res)
    }
}

#[repr(transparent)]
pub struct Stream<R, U, T>(RowsStream<T>, PhantomData<(R, U)>);

impl<'a, R, U: FromRow<'a, R>, T: Transport> Stream<R, U, T> {
    fn new(stream: &mut RowsStream<T>) -> &mut Self {
        // SAFETY: `Stream<R, U, T>` is a `transparent` wrapper over a `RowsStream<T>`,
        // it is therefore safe to transmute a mutable reference of one into a
        // mutable reference of the other.
        unsafe { transmute::<&mut RowsStream<T>, &mut Stream<R, U, T>>(stream) }
    }

    pub async fn next(&'a mut self) -> Result<Option<U>> {
        self.0.next().await?.map(U::from_row).transpose()
    } 
}

impl<'a, R, U: FromRow<'a, R>, T: Transport> FromQueryResult<'a, SetOf<R>, T> for &'a mut Stream<R, U, T> {
    type SyncOutput = Self;

    async fn from_stream(stream: &'a mut RowsStream<T>) -> Result<Self> {
        Ok(Stream::new(stream))
    }
}
