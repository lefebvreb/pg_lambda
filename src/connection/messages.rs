use std::collections::HashMap;
use std::ffi::CStr;
use std::io::{Error, ErrorKind, Result};

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
// <B>: (CommandComplete | DataRow | EmptyQueryResponse | ErrorResponse | NoticeResponse)*
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
    /// Message length if it is known, discounting eventual first byte and including itself.
    const SIZE: Option<i32> = None;
}

pub trait BackendMessage<'a>: Message {
    const PREFIX: u8;

    fn read(src: &mut &'a [u8]) -> Result<Self>;
}

pub trait FrontendMessage: Message {
    const PREFIX: Option<u8> = None;

    fn write(&self, dst: &mut Vec<u8>) -> Result<()>;
}

// COMMON

// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-ERRORRESPONSE
pub struct ErrorResponse<'a> {
    pub fields: HashMap<char, &'a CStr>,
}

impl Message for ErrorResponse<'_> {}

impl<'a> BackendMessage<'a> for ErrorResponse<'a> {
    const PREFIX: u8 = b'E';

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
    pub fields: HashMap<char, &'a CStr>,
}

impl Message for NoticeResponse<'_> {}

impl<'a> BackendMessage<'a> for NoticeResponse<'a> {
    const PREFIX: u8 = b'N';

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
    const SIZE: Option<i32> = Some(5);
}

impl BackendMessage<'_> for ReadyForQuery {
    const PREFIX: u8 = b'T';

    fn read(src: &mut &[u8]) -> Result<Self> {
        Ok(match read_u8(src)? {
            b'I' => Self::Idle,
            b'T' => Self::Transaction,
            b'E' => Self::FailedTransaction,
            n => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("unknown backend transaction status indicator: 0x{n:x}"),
                ))
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
        write_i32(PROTOCOL_VERSION, dst);
        write_cstr(c"user", dst);
        write_str(self.user, dst)?;
        write_cstr(c"database", dst);
        write_str(self.database, dst)?;
        write_u8(0, dst);
        Ok(())
    }
}

// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-AUTHENTICATIONOK
pub enum Authentication {
    Ok,
}

impl Message for Authentication {
    const SIZE: Option<i32> = Some(8);
}

impl<'a> BackendMessage<'a> for Authentication {
    const PREFIX: u8 = b'R';

    fn read(src: &mut &'a [u8]) -> Result<Self> {
        match read_i32(src)? {
            0 => Ok(Self::Ok),
            n => Err(Error::new(
                ErrorKind::Unsupported,
                format!("unsupported authentication type, code: {n}"),
            )),
        }
    }
}

// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-NEGOTIATEPROTOCOLVERSION
pub struct NegotiateProtocolVersion<'a> {
    pub min_supported: i32,
    pub unsupported: Vec<&'a CStr>,
}

impl Message for NegotiateProtocolVersion<'_> {}

impl<'a> BackendMessage<'a> for NegotiateProtocolVersion<'a> {
    const PREFIX: u8 = b'v';

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
    const SIZE: Option<i32> = Some(12);
}

impl BackendMessage<'_> for BackendKeyData {
    const PREFIX: u8 = b'K';

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

impl Message for ParameterStatus<'_> {}

impl<'a> BackendMessage<'a> for ParameterStatus<'a> {
    const PREFIX: u8 = b'S';

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

impl Message for Parse<'_> {}

impl FrontendMessage for Parse<'_> {
    const PREFIX: Option<u8> = Some(b'P');

    fn write(&self, dst: &mut Vec<u8>) -> Result<()> {
        write_cstr(c"", dst);
        write_str(self.query, dst)?;
        write_i16(0, dst);
        Ok(())
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-PARSECOMPLETE
pub struct ParseComplete;

impl Message for ParseComplete {
    const SIZE: Option<i32> = Some(4);
}

impl BackendMessage<'_> for ParseComplete {
    const PREFIX: u8 = b'1';

    fn read(_: &mut &[u8]) -> Result<Self> {
        Ok(Self)
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-BIND
pub struct Bind<F> {
    pub write_params: F,
}

impl<F> Message for Bind<F> {}

impl<F> FrontendMessage for Bind<F>
where
    F: Fn(&mut Vec<u8>) -> Result<()>,
{
    const PREFIX: Option<u8> = Some(b'B');

    fn write(&self, dst: &mut Vec<u8>) -> Result<()> {
        write_cstr(c"", dst);
        write_cstr(c"", dst);
        write_i16(1, dst);
        write_i16(1, dst);
        (self.write_params)(dst)?;
        write_i16(1, dst);
        write_i16(1, dst);
        Ok(())
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-BINDCOMPLETE
pub struct BindComplete;

impl Message for BindComplete {
    const SIZE: Option<i32> = Some(4);
}

impl BackendMessage<'_> for BindComplete {
    const PREFIX: u8 = b'2';

    fn read(_: &mut &[u8]) -> Result<Self> {
        Ok(Self)
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-EXECUTE
pub struct Execute;

impl Message for Execute {
    const SIZE: Option<i32> = Some(9);
}

impl FrontendMessage for Execute {
    const PREFIX: Option<u8> = Some(b'E');

    fn write(&self, dst: &mut Vec<u8>) -> Result<()> {
        write_cstr(c"", dst);
        write_i32(0, dst);
        Ok(())
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-EMPTYQUERYRESPONSE
pub struct EmptyQueryResponse;

impl Message for EmptyQueryResponse {
    const SIZE: Option<i32> = Some(4);
}

impl BackendMessage<'_> for EmptyQueryResponse {
    const PREFIX: u8 = b'I';

    fn read(_: &mut &[u8]) -> Result<Self> {
        Ok(Self)
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-COMMANDCOMPLETE
pub struct CommandComplete<'a> {
    pub tag: &'a CStr,
}

impl Message for CommandComplete<'_> {}

impl<'a> BackendMessage<'a> for CommandComplete<'a> {
    const PREFIX: u8 = b'C';

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

impl Message for DataRow<'_> {}

impl<'a> BackendMessage<'a> for DataRow<'a> {
    const PREFIX: u8 = b'D';

    fn read(src: &mut &'a [u8]) -> Result<Self> {
        let len = read_i16(src)?;
        let bytes = read_slice(len as i32, src)?;
        Ok(Self { len, bytes })
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-SYNC
pub struct Sync;

impl Message for Sync {
    const SIZE: Option<i32> = Some(4);
}

impl FrontendMessage for Sync {
    const PREFIX: Option<u8> = Some(b'S');

    fn write(&self, _: &mut Vec<u8>) -> Result<()> {
        Ok(())
    }
}

// TERMINATION

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-TERMINATE
pub struct Terminate;

impl Message for Terminate {
    const SIZE: Option<i32> = Some(4);
}

impl FrontendMessage for Terminate {
    const PREFIX: Option<u8> = Some(b'X');

    fn write(&self, _: &mut Vec<u8>) -> Result<()> {
        Ok(())
    }
}
