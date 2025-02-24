use std::io::{Error, ErrorKind, Read, Result, Write};
use std::marker::PhantomData;
use std::pin::pin;
use std::str;

use futures::{Stream, StreamExt};

use crate::connection::Row;

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

// pub struct Nullable<T>(PhantomData<T>);

// impl<T: PgType> PgType for Nullable<T> {
//     const SQL_NAME: &str = "???";
// }

// See: https://github.com/rust-lang/rust/issues/87479
pub trait FromQueryResult<'a, T>: Sized {
    type Output;

    type SyncOutput;

    #[allow(async_fn_in_trait)]
    async fn from_rows<S>(rows: S) -> Result<Self::Output>
    where
        S: Stream<Item = Result<Row<'a>>>;

    /// Assumes all [`Future`](std::future::Future)s resolve immediately and produces an output that can be consumed in a sync context.
    fn unsyncify(output: Self::Output) -> Self::SyncOutput;
}

impl<'a> FromQueryResult<'a, ()> for () {
    type Output = Self;

    type SyncOutput = Self::Output;

    async fn from_rows<S>(mut rows: S) -> Result<Self::Output>
    where
        S: Stream<Item = Result<Row<'a>>>,
    {
        match pin!(rows).next().await {
            Some(res) => {
                res?;
                Err(Error::new(
                    ErrorKind::InvalidData,
                    "unexpected non-empty query result",
                ))
            }
            None => Ok(()),
        }
    }

    fn unsyncify(output: Self::Output) -> Self::SyncOutput {
        output
    }
}

impl<'a, T: PgType, U: FromPgValue<'a, T>> FromQueryResult<'a, T> for U {
    type Output = Self;

    type SyncOutput = Self::Output;

    async fn from_rows<S>(mut rows: S) -> Result<Self::Output>
    where
        S: Stream<Item = Result<Row<'a>>>,
    {
        match pin!(rows).next().await {
            Some(res) => {
                let column = res?
                    .next()
                    .ok_or_else(|| Error::new(ErrorKind::InvalidData, "unexpected empty row"))??;
                match column {
                    Some(mut bytes) => U::read(&mut bytes),
                    None => U::null(),
                }
            }
            None => Err(Error::new(
                ErrorKind::UnexpectedEof,
                "unexpected empty query result",
            )),
        }
    }

    fn unsyncify(output: Self::Output) -> Self::SyncOutput {
        output
    }
}

pub struct AnonymousTable<T>(PhantomData<T>);

impl<'a, T0: PgType, U0: FromPgValue<'a, T0>> FromQueryResult<'a, AnonymousTable<(T0,)>> for (U0,) {
    type Output = ();

    type SyncOutput = ();

    async fn from_rows<S>(rows: S) -> Result<Self::Output>
    where
        S: Stream<Item = Result<Row<'a>>>,
    {
        todo!()
    }

    fn unsyncify(output: Self::Output) -> Self::SyncOutput {
        todo!()
    }
}

trait X<'a> {
    type Gat<Y>;
}
