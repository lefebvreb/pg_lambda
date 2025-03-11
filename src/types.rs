use std::io::{Error, ErrorKind, Result, Write};
use std::str;

use crate::util::{read_i32, read_u8, write_i32};

pub trait PgType: 'static {
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

    fn read(src: &'a [u8]) -> Result<Self>;
}

pub struct Boolean;

impl PgType for Boolean {
    const SQL_NAME: &str = "BOOLEAN";
}

impl ToPgValue<Boolean> for bool {
    fn write(&self, dst: &mut Vec<u8>) -> Result<()> {
        dst.push(*self as u8);
        Ok(())
    }
}

impl FromPgValue<'_, Boolean> for bool {
    fn read(mut src: &'_ [u8]) -> Result<Self> {
        let byte = read_u8(&mut src)?;
        Ok(byte != 0)
    }
}

pub struct Int4;

impl PgType for Int4 {
    const SQL_NAME: &str = "INT4";
}

pub type Integer = Int4;

pub type Int = Int4;

impl ToPgValue<Int4> for i32 {
    fn write(&self, dst: &mut Vec<u8>) -> Result<()> {
        write_i32(*self, dst);
        Ok(())
    }
}

impl FromPgValue<'_, Int4> for i32 {
    fn read(mut src: &'_ [u8]) -> Result<Self> {
        read_i32(&mut src)
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
    fn read(src: &'a [u8]) -> Result<Self> {
        str::from_utf8(src).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("string is not valid utf8: {e}"),
            )
        })
    }
}

impl<'a> FromPgValue<'a, Text> for String {
    fn read(src: &'a [u8]) -> Result<Self> {
        <&str as FromPgValue<Text>>::read(src).map(ToOwned::to_owned)
    }
}

// pub struct Nullable<T: PgType>(PhantomData<T>);

// impl<T: PgType> PgType for Nullable<T> {
//     const SQL_NAME: &str = "???";
// }
