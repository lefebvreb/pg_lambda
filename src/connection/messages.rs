use std::collections::HashMap;
use std::ffi::CStr;
use std::io::{Error, Result};

use super::util::*;

// https://www.postgresql.org/docs/current/protocol.html
//
// It goes...
//
// <F> is Frontend
// <B> is Frontend
//
// STARTUP
// <F>: StartupMessage
// <B>: AuthenticationOk | ErrorResponse | NegotiateProtocolVersion
// <B>: (BackendKeyData | ParameterStatus | ErrorResponse | NoticeResponse)*
// <B>: ReadyForQuery
//
// EXTENDED QUERY
// <F>: Parse
// <B>: ParseComplete | ErrorResponse
// <F>: Bind
// <B>: BindComplete | ErrorResponse
// <F>: Execute
// <B>: (CommandComplete | DataRow | ErrorResponse | NoticeResponse)*
//
// alternatively, if batching with a nonzero row-count in Execute:
// <B>: PortalSuspended
// <F>: Execute
// <B>: (CommandComplete | DataRow | ErrorResponse | NoticeResponse)*
// <B>: CommandComplete
//
// finally:
// <F>: Sync
// <B>: ReadyForQuery
//
// ASYNCHRONOUS OPERATIONS
// <B>: NoticeResponse | ParameterStatus
//
// TERMINATION
// <F>: Terminate
// <F>: EOF

pub const PROTOCOL_VERSION: i32 = 196608;

pub trait Message: Sized {
    /// First byte, if there is one.
    const FIRST_BYTE: Option<u8> = None;
    /// Message length if it is known, discounting eventual first byte and including itself.
    const CONTENT_LENGTH: Option<i32> = None;
}

pub trait BackendMessage<'a>: Message {
    fn read(src: &mut &'a [u8]) -> Result<Self>;
}

pub trait FrontendMessage: Message {
    fn write(&self, dst: &mut Vec<u8>) -> Result<()>;
}

// COMMON

// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-ERRORRESPONSE
pub struct ErrorResponse<'a> {
    fields: HashMap<char, &'a CStr>,
}

impl Message for ErrorResponse<'_> {
    const FIRST_BYTE: Option<u8> = Some(b'E');
}

impl<'a> BackendMessage<'a> for ErrorResponse<'a> {
    fn read(src: &mut &'a [u8]) -> Result<Self> {
        let mut this = Self {
            fields: HashMap::new(),
        };
        loop {
            let field = read_u8(src)?;
            if field == 0 {
                break;
            }
            this.fields.insert(field as char, read_cstr(src)?);
        }
        Ok(this)
    }
}

// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-NOTICERESPONSE
pub struct NoticeResponse<'a> {
    fields: HashMap<char, &'a CStr>,
}

impl Message for NoticeResponse<'_> {
    const FIRST_BYTE: Option<u8> = Some(b'N');
}

impl<'a> BackendMessage<'a> for NoticeResponse<'a> {
    fn read(src: &mut &'a [u8]) -> Result<Self> {
        let mut this = Self {
            fields: HashMap::new(),
        };
        loop {
            let field = read_u8(src)?;
            if field == 0 {
                break;
            }
            this.fields.insert(field as char, read_cstr(src)?);
        }
        Ok(this)
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-READYFORQUERY
pub enum ReadyForQuery {
    Idle,
    Transaction,
    FailedTransaction,
}

impl Message for ReadyForQuery {
    const FIRST_BYTE: Option<u8> = Some(b'T');
    const CONTENT_LENGTH: Option<i32> = Some(5);
}

impl BackendMessage<'_> for ReadyForQuery {
    fn read(src: &mut &[u8]) -> Result<Self> {
        Ok(match read_u8(src)? {
            b'I' => Self::Idle,
            b'T' => Self::Transaction,
            b'E' => Self::FailedTransaction,
            n => {
                return Err(Error::other(format!(
                    "unknown backend transaction status indicator: 0x{n:x}"
                )))
            }
        })
    }
}

// STARTUP

// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-STARTUPMESSAGE
pub struct StartupMessage<'a> {
    pub user: &'a str,
    pub database: &'a str,
}

impl Message for StartupMessage<'_> {}

impl FrontendMessage for StartupMessage<'_> {
    fn write(&self, dst: &mut Vec<u8>) -> Result<()> {
        write_i32(PROTOCOL_VERSION, dst)?;
        write_cstr("user", dst)?;
        write_cstr(self.user, dst)?;
        write_cstr("database", dst)?;
        write_cstr(self.database, dst)?;
        write_u8(0, dst)
    }
}

// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-AUTHENTICATIONOK
pub struct AuthenticationOk;

impl Message for AuthenticationOk {
    const FIRST_BYTE: Option<u8> = Some(b'R');
    const CONTENT_LENGTH: Option<i32> = Some(8);
}

impl<'a> BackendMessage<'a> for AuthenticationOk {
    fn read(src: &mut &'a [u8]) -> Result<Self> {
        Ok(match read_i32(src)? {
            0 => Self,
            n => {
                return Err(Error::other(format!(
                    "unknown authentication message type: {n}"
                )))
            }
        })
    }
}

// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-NEGOTIATEPROTOCOLVERSION
pub struct NegotiateProtocolVersion<'a> {
    pub min_supported: i32,
    pub unsupported: Vec<&'a CStr>,
}

impl Message for NegotiateProtocolVersion<'_> {
    const FIRST_BYTE: Option<u8> = Some(b'v');
}

impl<'a> BackendMessage<'a> for NegotiateProtocolVersion<'a> {
    fn read(src: &mut &'a [u8]) -> Result<Self> {
        let min_supported = read_i32(src)?;
        let n = read_i32(src)?;
        let mut unsupported = Vec::with_capacity(n as usize);
        for _ in 0..n {
            unsupported.push(read_cstr(src)?);
        }
        Ok(Self {
            min_supported,
            unsupported,
        })
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-BACKENDKEYDATA
pub struct BackendKeyData {
    pub proc_id: i32,
    pub secret_key: i32,
}

impl Message for BackendKeyData {
    const FIRST_BYTE: Option<u8> = Some(b'K');
    const CONTENT_LENGTH: Option<i32> = Some(12);
}

impl BackendMessage<'_> for BackendKeyData {
    fn read(src: &mut &'_ [u8]) -> Result<Self> {
        Ok(Self {
            proc_id: read_i32(src)?,
            secret_key: read_i32(src)?,
        })
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-PARAMETERSTATUS
pub struct ParameterStatus<'a> {
    pub name: &'a CStr,
    pub value: &'a CStr,
}

impl Message for ParameterStatus<'_> {
    const FIRST_BYTE: Option<u8> = Some(b'S');
}

impl<'a> BackendMessage<'a> for ParameterStatus<'a> {
    fn read(src: &mut &'a [u8]) -> Result<Self> {
        Ok(Self {
            name: read_cstr(src)?,
            value: read_cstr(src)?,
        })
    }
}

// Extended Query

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-PARSE
pub struct Parse<'a> {
    pub query: &'a str,
}

impl Message for Parse<'_> {
    const FIRST_BYTE: Option<u8> = Some(b'P');
}

impl FrontendMessage for Parse<'_> {
    fn write(&self, dst: &mut Vec<u8>) -> Result<()> {
        write_cstr("", dst)?;
        write_cstr(self.query, dst)?;
        write_i16(0, dst)?;
        Ok(())
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-PARSECOMPLETE
pub struct ParseComplete;

impl Message for ParseComplete {
    const FIRST_BYTE: Option<u8> = Some(b'1');
    const CONTENT_LENGTH: Option<i32> = Some(4);
}

impl BackendMessage<'_> for ParseComplete {
    fn read(_: &mut &[u8]) -> Result<Self> {
        Ok(Self)
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-BIND
pub struct Bind<'a> {
    pub parameters: Vec<&'a [u8]>,
}

impl Message for Bind<'_> {
    const FIRST_BYTE: Option<u8> = Some(b'B');
}

impl FrontendMessage for Bind<'_> {
    fn write(&self, dst: &mut Vec<u8>) -> Result<()> {
        write_cstr("", dst)?;
        write_cstr("", dst)?;
        write_i16(1, dst)?;
        write_i16(1, dst)?;
        write_i32(self.parameters.len() as i32, dst)?;
        for param in &self.parameters {
            write_i32(param.len() as i32, dst)?;
            write_slice(param, dst)?;
        }
        write_i16(1, dst)?;
        write_i16(1, dst)?;
        Ok(())
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-BINDCOMPLETE
pub struct BindComplete;

impl Message for BindComplete {
    const FIRST_BYTE: Option<u8> = Some(b'2');
    const CONTENT_LENGTH: Option<i32> = Some(4);
}

impl BackendMessage<'_> for BindComplete {
    fn read(_: &mut &[u8]) -> Result<Self> {
        Ok(Self)
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-EXECUTE
pub struct Execute;

impl Message for Execute {
    const FIRST_BYTE: Option<u8> = Some(b'E');
    const CONTENT_LENGTH: Option<i32> = Some(9);
}

impl FrontendMessage for Execute {
    fn write(&self, dst: &mut Vec<u8>) -> Result<()> {
        write_cstr("", dst)?;
        write_i32(0, dst)?;
        Ok(())
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-COMMANDCOMPLETE
pub struct CommandComplete<'a> {
    pub tag: &'a CStr,
}

impl Message for CommandComplete<'_> {
    const FIRST_BYTE: Option<u8> = Some(b'C');
}

impl<'a> BackendMessage<'a> for CommandComplete<'a> {
    fn read(src: &mut &'a [u8]) -> Result<Self> {
        Ok(Self {
            tag: read_cstr(src)?,
        })
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-DATAROW
pub struct DataRow<'a> {
    pub len: i16,
    pub bytes: &'a [u8],
}

impl Message for DataRow<'_> {
    const FIRST_BYTE: Option<u8> = Some(b'D');
}

impl<'a> BackendMessage<'a> for DataRow<'a> {
    fn read(src: &mut &'a [u8]) -> Result<Self> {
        let len = read_i16(src)?;
        let bytes = read_slice(len as i32, src)?;
        Ok(Self { len, bytes })
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-SYNC
pub struct Sync;

impl Message for Sync {
    const FIRST_BYTE: Option<u8> = Some(b'S');
    const CONTENT_LENGTH: Option<i32> = Some(4);
}

impl FrontendMessage for Sync {
    fn write(&self, _: &mut Vec<u8>) -> Result<()> {
        Ok(())
    }
}

// TERMINATION

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-TERMINATE
pub struct Terminate;

impl Message for Terminate {
    const FIRST_BYTE: Option<u8> = Some(b'X');
    const CONTENT_LENGTH: Option<i32> = Some(4);
}

impl FrontendMessage for Terminate {
    fn write(&self, _: &mut Vec<u8>) -> Result<()> {
        Ok(())
    }
}
