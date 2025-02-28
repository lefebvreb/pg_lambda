use std::io::{Error, ErrorKind, Read, Result, Write};
use std::str;

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
