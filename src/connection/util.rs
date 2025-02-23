use std::ffi::CStr;
use std::io::{Error, Read, Result, Write};
use std::slice;

pub fn read_u8(src: &mut &[u8]) -> Result<u8> {
    let mut byte = 0;
    src.read_exact(slice::from_mut(&mut byte))?;
    Ok(byte)
}

pub fn write_u8(val: u8, dst: &mut Vec<u8>) -> Result<()> {
    dst.write_all(slice::from_ref(&val))
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
    let cstr = CStr::from_bytes_until_nul(src).map_err(|e| {
        Error::other(format!(
            "could not read nul-terminated string in message: {e}",
        ))
    })?;
    *src = &src[cstr.count_bytes() + 1..];
    Ok(cstr)
}

pub fn write_cstr(val: &str, dst: &mut Vec<u8>) -> Result<()> {
    dst.write_all(val.as_bytes())?;
    write_u8(b'\0', dst)
}

pub fn read_slice<'a>(len: i32, src: &mut &'a [u8]) -> Result<&'a [u8]> {
    let (slice, tail) = src.split_at(len as usize);
    *src = tail;
    Ok(slice)
}
