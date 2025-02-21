use std::ffi::NulError;
use std::io::{self, Error, Read, Write};
use std::str;

use crate::connection::Row;

pub trait PgType {
    const SQL_NAME: &str;
}

pub trait ToPgValue<T: PgType> {    
    fn write(&self, dst: &mut Vec<u8>) -> io::Result<()>;
}

pub trait FromPgValue<'a, T: PgType>: Sized {
    fn read(&self, src: &mut &'a [u8]) -> io::Result<Self>;
}

// pub struct Void;

// impl PgType for Void {
//     const SQL_NAME: &str = "VOID";
// }

// impl ToPgValue<Void> for () {
//     fn write(&self, _: &mut Vec<u8>) -> io::Result<()> {
//         Ok(())
//     }
// }

// impl FromPgValue<'_, Void> for () {
//     fn read(&self, _: &[u8]) -> io::Result<Self> {
//         Ok(())
//     }
// }

pub struct Int4;

impl PgType for Int4 {
    const SQL_NAME: &str = "INT4";
}

pub type Integer = Int4;

pub type Int = Int4;

impl ToPgValue<Int4> for i32 {
    fn write(&self, dst: &mut Vec<u8>) -> io::Result<()> {
        dst.write_all(&self.to_be_bytes())
    }
}

impl FromPgValue<'_, Int4> for i32 {
    fn read(&self, src: &mut &'_ [u8]) -> io::Result<Self> {
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
    fn write(&self, dst: &mut Vec<u8>) -> io::Result<()> {
        dst.write_all(self.as_bytes())
    }
}

// Maybe this shouldn't be an io::Error, rather a validation error.
impl<'a> FromPgValue<'a, Text> for &'a str {
    fn read(&self, src: &mut &'a [u8]) -> io::Result<Self> {
        str::from_utf8(*src).map_err(|e| Error::other(format!("string is not valid UTF8: {e}")))
    }
}

pub trait FromQueryResult<'a, T>: Sized {
    fn from_row(row: Row<'a>) -> io::Result<Self>;
}

impl<'a, T: PgType, U: FromPgValue<'a, T>> FromQueryResult<'a, T> for U {
    fn from_row(row: Row<'a>) -> io::Result<Self> {
        todo!()
    }
}

impl<'a, T0: PgType, U0: FromPgValue<'a, T0>> FromQueryResult<'a, (T0,)> for Vec<(U0,)> {
    fn from_row(row: Row<'a>) -> io::Result<Self> {
        todo!()
    }
}

impl<'a, T0: PgType, T1: PgType, U0: FromPgValue<'a, T0>, U1: FromPgValue<'a, T1>>
    FromQueryResult<'a, (T0, T1)> for Vec<(U0, U1)>
{
    fn from_row(row: Row<'a>) -> io::Result<Self> {
        todo!()
    }
}

// // etc.
