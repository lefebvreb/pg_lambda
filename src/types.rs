use std::io::{Error, ErrorKind, Read, Result, Write};
use std::str;

use crate::connection::Row;

pub trait PgType {
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
        return Err(Error::new(ErrorKind::InvalidData, "unexpected null value"));
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
        Ok(str::from_utf8(*src).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("string is not valid utf8: {e}"),
            )
        })?)
    }
}

impl<'a> FromPgValue<'a, Text> for String {
    fn read(src: &mut &'a [u8]) -> Result<Self> {
        <&str as FromPgValue<Text>>::read(src).map(ToOwned::to_owned)
    }
}

// IDEA: have multiple separate traits, one for void, one for single value and another for sets.
pub trait FromQueryResult<'a, T>: Sized {
    fn from_row(row: Row<'a>) -> Result<Self>;
}

impl<'a, T: PgType, U: FromPgValue<'a, T>> FromQueryResult<'a, T> for U {
    fn from_row(mut row: Row<'a>) -> Result<Self> {
        let column = row
            .next()
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "unexpected empty row"))??;
        match column {
            Some(mut bytes) => U::read(&mut bytes),
            None => U::null(),
        }
    }
}

impl<'a, T0: PgType, U0: FromPgValue<'a, T0>> FromQueryResult<'a, (T0,)> for Vec<(U0,)> {
    fn from_row(row: Row<'a>) -> Result<Self> {
        todo!()
    }
}

impl<'a, T0: PgType, T1: PgType, U0: FromPgValue<'a, T0>, U1: FromPgValue<'a, T1>>
    FromQueryResult<'a, (T0, T1)> for Vec<(U0, U1)>
{
    fn from_row(row: Row<'a>) -> Result<Self> {
        todo!()
    }
}
