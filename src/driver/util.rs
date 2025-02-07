use std::ffi::CStr;
use std::fmt::Debug;
use std::io::{Error, Read, Result, Write};
use std::slice;

pub fn ensure_eq<T: PartialEq + Debug>(lhs: T, rhs: T) -> Result<()> {
    if lhs == rhs {
        Ok(())
    } else {
        Err(Error::other(format!(
            "could not validate message: found `{lhs:?}`, expected `{rhs:?}`",
        )))
    }
}

pub fn read_u8(src: &mut &[u8]) -> Result<u8> {
    let mut byte = 0;
    src.read_exact(slice::from_mut(&mut byte))?;
    Ok(byte)
}

pub fn write_u8(val: u8, dst: &mut Vec<u8>) -> Result<()> {
    dst.write_all(slice::from_ref(&val))
}

pub fn read_i8(src: &mut &[u8]) -> Result<i8> {
    let mut buf = [0; 1];
    src.read_exact(&mut buf)?;
    Ok(i8::from_be_bytes(buf))
}

pub fn write_i8(val: i8, dst: &mut Vec<u8>) -> Result<()> {
    dst.write_all(&val.to_be_bytes())
}

pub fn read_i16(src: &mut &[u8]) -> Result<i16> {
    let mut buf = [0; 2];
    src.read_exact(&mut buf)?;
    Ok(i16::from_be_bytes(buf))
}

pub fn write_i16(val: i16, dst: &mut Vec<u8>) -> Result<()> {
    dst.write_all(&val.to_be_bytes())
}

pub fn read_i32(src: &mut &[u8]) -> Result<i32> {
    let mut buf = [0; 4];
    src.read_exact(&mut buf)?;
    Ok(i32::from_be_bytes(buf))
}

pub fn write_i32(val: i32, dst: &mut Vec<u8>) -> Result<()> {
    dst.write_all(&val.to_be_bytes())
}

pub fn read_cstr<'a>(src: &mut &'a [u8]) -> Result<&'a CStr> {
    let cstr = CStr::from_bytes_until_nul(*src).map_err(|e| {
        Error::other(format!(
            "could not read nul-terminated string in message: {e}",
        ))
    })?;
    *src = &src[cstr.count_bytes() + 1..];
    Ok(cstr)
}

pub fn write_cstr(val: &CStr, dst: &mut Vec<u8>) -> Result<()> {
    dst.write_all(val.to_bytes_with_nul())
}

pub fn read_slice<'a>(n: i32, src: &mut &'a [u8]) -> Result<&'a [u8]> {
    let (slice, tail) = src.split_at(n as usize);
    *src = tail;
    Ok(slice)
}

pub fn write_slice(val: &[u8], dst: &mut Vec<u8>) -> Result<()> {
    dst.write_all(val)
}

#[derive(Copy, Clone)]
pub struct LazyI16Array<'a>(&'a [u8]);

impl<'a> LazyI16Array<'a> {
    pub fn len(&self) -> usize {
        self.0.len() / 2
    }

    pub fn get(&self, i: usize) -> Option<i16> {
        let buf = self.0.get(i * 2..(i + 1) * 2)?;
        Some(i16::from_be_bytes(buf.try_into().unwrap()))
    }
}

pub fn read_i16_array<'a>(n: i32, src: &mut &'a [u8]) -> Result<LazyI16Array<'a>> {
    Ok(LazyI16Array(read_slice(n * 2, src)?))
}

#[derive(Copy, Clone)]
pub struct CStrList<'a>(&'a [u8]);

impl<'a> Iterator for CStrList<'a> {
    type Item = &'a CStr;

    fn next(&mut self) -> Option<Self::Item> {
        if matches!(self.0.first(), None | Some(0)) {
            None
        } else {
            read_cstr(&mut self.0).ok()
        }
    }
}

pub fn read_cstr_list<'a>(src: &mut &'a [u8]) -> Result<CStrList<'a>> {
    Ok(CStrList(src))
}
