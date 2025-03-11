use std::ffi::CStr;
use std::io::{Error, ErrorKind, Read, Result};
use std::slice;

use rsasl::prelude::SessionError;
use unicode_xid::UnicodeXID;

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

pub fn write_slice(val: &[u8], dst: &mut Vec<u8>) {
    dst.extend(val);
}

pub fn closed_transport() -> Error {
    Error::new(ErrorKind::UnexpectedEof, "transport was closed")
}

pub fn from_session_error(e: SessionError) -> Error {
    Error::new(
        ErrorKind::InvalidData,
        format!("sasl authentication error: {e}"),
    )
}

pub fn unexpected_message_prefix(prefix: u8) -> Error {
    Error::new(
        ErrorKind::InvalidData,
        format!("unknown backend message byte: 0x{prefix:x}"),
    )
}

pub fn unexpected_authentication_message() -> Error {
    Error::new(
        ErrorKind::InvalidData,
        "unexpected authentication message at this point",
    )
}

pub fn sanitize_ident(name: &str) -> Result<()> {
    let mut chars = name.chars();

    match chars.next() {
        Some('_') => match chars.next() {
            Some(c) if c.is_xid_continue() => (),
            _ => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "invalid identifier: \"{name}\": identifiers starting with an underscore '_' must be followed by at least one XID_CONTINUE character",
                    ),
                ));
            }
        },
        Some(c) if c.is_xid_start() => (),
        _ => {
            return Err(Error::new(
                ErrorKind::UnexpectedEof,
                format!(
                    "invalid identifier: \"{name}\": identifiers must start with either an underscore '_' or a XID_START character",
                ),
            ));
        }
    };

    if !chars.all(char::is_xid_continue) {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "invalid identifier \"{name}\": tail characters must all be XID_CONTINUE characters",
            ),
        ));
    }

    Ok(())
}
