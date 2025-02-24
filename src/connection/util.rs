use std::ffi::CStr;
use std::io::{Error, ErrorKind, Read, Result};
use std::slice;

use super::messages::{ErrorResponse, NegotiateProtocolVersion};

pub fn read_u8(src: &mut &[u8]) -> Result<u8> {
    let mut byte = 0;
    src.read_exact(slice::from_mut(&mut byte))?;
    Ok(byte)
}

pub fn read_i16(src: &mut &[u8]) -> Result<i16> {
    let mut buf = [0; 2];
    src.read_exact(&mut buf)?;
    Ok(i16::from_be_bytes(buf))
}

pub fn read_i32(src: &mut &[u8]) -> Result<i32> {
    let mut buf = [0; 4];
    src.read_exact(&mut buf)?;
    Ok(i32::from_be_bytes(buf))
}

pub fn read_cstr<'a>(src: &mut &'a [u8]) -> Result<&'a CStr> {
    let cstr = CStr::from_bytes_until_nul(src).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("could not read nul-terminated string in message: {e}"),
        )
    })?;
    *src = &src[cstr.count_bytes() + 1..];
    Ok(cstr)
}

pub fn read_slice<'a>(len: i32, src: &mut &'a [u8]) -> Result<&'a [u8]> {
    let (slice, tail) = src.split_at(len as usize);
    *src = tail;
    Ok(slice)
}

pub fn write_u8(val: u8, dst: &mut Vec<u8>) {
    dst.push(val);
}

pub fn write_i16(val: i16, dst: &mut Vec<u8>) {
    dst.extend(&val.to_be_bytes());
}

pub fn write_i32(val: i32, dst: &mut Vec<u8>) {
    dst.extend(&val.to_be_bytes());
}

pub fn write_cstr(val: &CStr, dst: &mut Vec<u8>) {
    dst.extend(val.to_bytes_with_nul());
}

pub fn write_str(val: &str, dst: &mut Vec<u8>) -> Result<()> {
    if val.contains('\0') {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "string contains a null character",
        ));
    }
    dst.extend(val.as_bytes());
    write_u8(b'\0', dst);
    Ok(())
}

impl From<ErrorResponse<'_>> for Error {
    fn from(msg: ErrorResponse) -> Self {
        Error::new(
            ErrorKind::InvalidData,
            format!("postgresql error: {:?}", msg.fields),
        )
    }
}

impl From<NegotiateProtocolVersion<'_>> for Error {
    fn from(msg: NegotiateProtocolVersion) -> Self {
        Error::new(
            ErrorKind::InvalidData,
            format!(
                "unsupported protocol version, newest minor protocol supported version is {}, the following options are unsupported: {}",
                msg.min_supported,
                msg.unsupported
                    .into_iter()
                    .flat_map(CStr::to_str)
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        )
    }
}

pub fn unexpected_message_prefix(n: u8) -> Error {
    Error::new(
        ErrorKind::InvalidData,
        format!("unknown backend message byte: 0x{n:x}"),
    )
}
