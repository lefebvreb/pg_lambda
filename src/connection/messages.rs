use std::collections::HashMap;
use std::ffi::CStr;
use std::io::{Error, ErrorKind, Result};

use super::params::QueryParams;
use super::util::{
    read_cstr, read_i16, read_i32, read_u8, write_cstr, write_i16, write_i32, write_slice,
    write_str, write_u8,
};

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

pub trait BackendMessage<'a>: Sized {
    const PREFIX: u8;

    fn read(src: &'a [u8]) -> Result<Self>;
}

pub trait FrontendMessage {
    const PREFIX: Option<u8> = None;

    fn write(self, dst: &mut Vec<u8>) -> Result<()>;
}

// COMMON

// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-ERRORRESPONSE
pub struct ErrorResponse<'a> {
    pub fields: HashMap<char, &'a CStr>,
}

impl<'a> BackendMessage<'a> for ErrorResponse<'a> {
    const PREFIX: u8 = b'E';

    fn read(mut src: &'a [u8]) -> Result<Self> {
        let mut this = Self {
            fields: HashMap::new(),
        };
        loop {
            let field = read_u8(&mut src)?;
            if field == 0 {
                break;
            }
            this.fields.insert(field as char, read_cstr(&mut src)?);
        }
        Ok(this)
    }
}

// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-NOTICERESPONSE
pub struct NoticeResponse<'a> {
    pub fields: HashMap<char, &'a CStr>,
}

impl<'a> BackendMessage<'a> for NoticeResponse<'a> {
    const PREFIX: u8 = b'N';

    fn read(mut src: &'a [u8]) -> Result<Self> {
        let mut this = Self {
            fields: HashMap::new(),
        };
        loop {
            let field = read_u8(&mut src)?;
            if field == 0 {
                break;
            }
            this.fields.insert(field as char, read_cstr(&mut src)?);
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

impl BackendMessage<'_> for ReadyForQuery {
    const PREFIX: u8 = b'Z';

    fn read(mut src: &[u8]) -> Result<Self> {
        Ok(match read_u8(&mut src)? {
            b'I' => Self::Idle,
            b'T' => Self::Transaction,
            b'E' => Self::FailedTransaction,
            n => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("unknown backend transaction status indicator: 0x{n:x}"),
                ));
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

impl FrontendMessage for StartupMessage<'_> {
    fn write(self, dst: &mut Vec<u8>) -> Result<()> {
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
pub enum Authentication<'a> {
    Ok,
    Sasl { mechanisms: Vec<&'a CStr> },
    SaslContinue { data: &'a [u8] },
    SaslFinal,
}

impl<'a> BackendMessage<'a> for Authentication<'a> {
    const PREFIX: u8 = b'R';

    fn read(mut src: &'a [u8]) -> Result<Self> {
        match read_i32(&mut src)? {
            0 => Ok(Self::Ok),
            10 => {
                let mut mechanisms = Vec::new();
                loop {
                    let cstr = read_cstr(&mut src)?;
                    if cstr == c"" {
                        break;
                    }
                    mechanisms.push(cstr);
                }
                Ok(Self::Sasl { mechanisms })
            }
            11 => Ok(Self::SaslContinue { data: src }),
            12 => Ok(Self::SaslFinal),
            n => Err(Error::new(
                ErrorKind::Unsupported,
                format!("unsupported authentication type, code: {n}"),
            )),
        }
    }
}

// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-SASLINITIALRESPONSE
pub struct SaslInitialResponse<'a> {
    pub mechanism: &'a str,
    pub data: Option<&'a [u8]>,
}

impl FrontendMessage for SaslInitialResponse<'_> {
    const PREFIX: Option<u8> = Some(b'p');

    fn write(self, dst: &mut Vec<u8>) -> Result<()> {
        write_str(self.mechanism, dst)?;
        match self.data {
            Some(data) => {
                write_i32(data.len() as i32, dst);
                write_slice(data, dst);
            }
            None => write_i32(-1, dst),
        }
        Ok(())
    }
}

// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-SASLRESPONSE
pub struct SaslResponse<'a> {
    pub data: &'a [u8],
}

impl FrontendMessage for SaslResponse<'_> {
    const PREFIX: Option<u8> = Some(b'p');

    fn write(self, dst: &mut Vec<u8>) -> Result<()> {
        write_slice(self.data, dst);
        Ok(())
    }
}

// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-NEGOTIATEPROTOCOLVERSION
pub struct NegotiateProtocolVersion<'a> {
    pub min_supported: i32,
    pub unsupported: Vec<&'a CStr>,
}

impl<'a> BackendMessage<'a> for NegotiateProtocolVersion<'a> {
    const PREFIX: u8 = b'v';

    fn read(mut src: &'a [u8]) -> Result<Self> {
        let min_supported = read_i32(&mut src)?;
        let n = read_i32(&mut src)?;
        let mut unsupported = Vec::with_capacity(n as usize);
        for _ in 0..n {
            unsupported.push(read_cstr(&mut src)?);
        }
        Ok(Self {
            min_supported,
            unsupported,
        })
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-BACKENDKEYDATA
pub struct BackendKeyData;

impl BackendMessage<'_> for BackendKeyData {
    const PREFIX: u8 = b'K';

    fn read(_: &[u8]) -> Result<Self> {
        Ok(Self)
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-PARAMETERSTATUS
pub struct ParameterStatus;

impl BackendMessage<'_> for ParameterStatus {
    const PREFIX: u8 = b'S';

    fn read(_: &[u8]) -> Result<Self> {
        Ok(Self)
    }
}

// Extended Query

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-PARSE
pub struct Parse<'a> {
    pub statement: &'a str,
}

impl FrontendMessage for Parse<'_> {
    const PREFIX: Option<u8> = Some(b'P');

    fn write(self, dst: &mut Vec<u8>) -> Result<()> {
        write_cstr(c"", dst);
        write_str(self.statement, dst)?;
        write_i16(0, dst);
        Ok(())
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-PARSECOMPLETE
pub struct ParseComplete;

impl BackendMessage<'_> for ParseComplete {
    const PREFIX: u8 = b'1';

    fn read(_: &[u8]) -> Result<Self> {
        Ok(Self)
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-BIND
pub struct Bind<P> {
    pub params: P,
}

impl<P> FrontendMessage for Bind<P>
where
    P: QueryParams,
{
    const PREFIX: Option<u8> = Some(b'B');

    fn write(self, dst: &mut Vec<u8>) -> Result<()> {
        write_cstr(c"", dst);
        write_cstr(c"", dst);
        write_i16(1, dst);
        write_i16(1, dst);
        self.params.write(dst)?;
        write_i16(1, dst);
        write_i16(1, dst);
        Ok(())
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-BINDCOMPLETE
pub struct BindComplete;

impl BackendMessage<'_> for BindComplete {
    const PREFIX: u8 = b'2';

    fn read(_: &[u8]) -> Result<Self> {
        Ok(Self)
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-EXECUTE
pub struct Execute;

impl FrontendMessage for Execute {
    const PREFIX: Option<u8> = Some(b'E');

    fn write(self, dst: &mut Vec<u8>) -> Result<()> {
        write_cstr(c"", dst);
        write_i32(0, dst);
        Ok(())
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-EMPTYQUERYRESPONSE
pub struct EmptyQueryResponse;

impl BackendMessage<'_> for EmptyQueryResponse {
    const PREFIX: u8 = b'I';

    fn read(_: &[u8]) -> Result<Self> {
        Ok(Self)
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-COMMANDCOMPLETE
pub struct CommandComplete;

impl BackendMessage<'_> for CommandComplete {
    const PREFIX: u8 = b'C';

    fn read(_: &[u8]) -> Result<Self> {
        Ok(Self)
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-DATAROW
pub struct DataRow<'a> {
    pub len: i16,
    pub columns: &'a [u8],
}

impl<'a> BackendMessage<'a> for DataRow<'a> {
    const PREFIX: u8 = b'D';

    fn read(mut src: &'a [u8]) -> Result<Self> {
        let len = read_i16(&mut src)?;
        Ok(Self { len, columns: src })
    }
}

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-SYNC
pub struct Sync;

impl FrontendMessage for Sync {
    const PREFIX: Option<u8> = Some(b'S');

    fn write(self, _: &mut Vec<u8>) -> Result<()> {
        Ok(())
    }
}

// TERMINATION

/// https://www.postgresql.org/docs/current/protocol-message-formats.html#PROTOCOL-MESSAGE-FORMATS-TERMINATE
pub struct Terminate;

impl FrontendMessage for Terminate {
    const PREFIX: Option<u8> = Some(b'X');

    fn write(self, _: &mut Vec<u8>) -> Result<()> {
        Ok(())
    }
}
