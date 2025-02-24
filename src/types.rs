use std::io::{Error, ErrorKind, Read, Result, Write};
use std::marker::PhantomData;
use std::str;

use crate::connection::{RowsStream, Transport};

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
    type SyncOutput;

    #[allow(async_fn_in_trait)]
    async fn from_rows(stream: &'a mut RowsStream<impl Transport>) -> Result<Self>;

    /// Assumes all [`Future`](std::future::Future)s resolve immediately and produces an output that can be consumed in a sync context.
    fn syncify(self) -> Self::SyncOutput;
}

impl FromQueryResult<'_, ()> for () {
    type SyncOutput = Self;

    async fn from_rows(stream: &mut RowsStream<impl Transport>) -> Result<Self> {
        match stream.next().await? {
            Some(_) => Err(Error::new(
                ErrorKind::InvalidData,
                "unexpected non-empty query result",
            )),
            None => Ok(()),
        }
    }

    fn syncify(self) -> Self::SyncOutput {
        self
    }
}

impl<'a, T: PgType, U: FromPgValue<'a, T>> FromQueryResult<'a, T> for U {
    type SyncOutput = Self;

    async fn from_rows(stream: &'a mut RowsStream<impl Transport>) -> Result<Self> {
        match stream.next().await? {
            Some(mut row) => {
                let column = row
                    .next()
                    .ok_or_else(|| Error::new(ErrorKind::InvalidData, "unexpected empty row"))??;

                if row.next().is_some() {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        "unexpected second column in row",
                    ));
                }

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

    fn syncify(self) -> Self::SyncOutput {
        self
    }
}

pub struct AnonymousTable<T>(PhantomData<T>);

impl<'a, T0: PgType, U0: FromPgValue<'a, T0>> FromQueryResult<'a, AnonymousTable<(T0,)>> for Vec<(U0,)> {
    type SyncOutput = Self;

    async fn from_rows(stream: &'a mut RowsStream<impl Transport>) -> Result<Self> {
        match stream.next().await? {
            Some(row) => todo!(),
            None => todo!(),
        }
    }

    fn syncify(self) -> Self::SyncOutput {
        self
    }
}

// fn from_conn<'a>(conn: &'a mut Connection<impl Transport>) -> impl futures::Stream<Item = Result<&'a [u8]>> {
//     futures::stream::try_unfold(conn, |conn| async {
//         match conn.next_row().await? {
//             Some(mut row) => Ok(Some((row.next().unwrap().unwrap().unwrap(), conn))),
//             None => Ok(None),
//         }
//     })
// }
