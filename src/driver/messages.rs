use std::collections::HashMap;
use std::ffi::CString;
use std::io::{Error, Result};

use super::util::*;
use super::{BackendMessage, FrontendMessage, Message};

// Start-Up
// https://www.postgresql.org/docs/current/protocol-flow.html#PROTOCOL-FLOW-START-UP

pub enum ReplicationMode {
    True,
    False,
    Database,
}

pub struct Startup {
    pub user: CString,
    pub database: Option<CString>,
    pub replication: Option<ReplicationMode>,
    pub parameters: HashMap<CString, CString>,
}

impl Message for Startup {}

impl FrontendMessage for Startup {
    fn write(&self, dst: &mut Vec<u8>) -> Result<()> {
        write_cstr(c"user", dst)?;
        write_cstr(&self.user, dst)?;
        if let Some(database) = &self.database {
            write_cstr(c"database", dst)?;
            write_cstr(database, dst)?;
        }
        if let Some(replication) = &self.replication {
            write_cstr(c"replication", dst)?;
            match replication {
                ReplicationMode::True => write_cstr(c"true", dst)?,
                ReplicationMode::False => write_cstr(c"false", dst)?,
                ReplicationMode::Database => write_cstr(c"database", dst)?,
            }
        }
        for (name, value) in &self.parameters {
            write_cstr(name, dst)?;
            write_cstr(value, dst)?;
        }
        write_u8(0, dst)
    }
}

pub struct ErrorResponse {
    
}

pub enum Authentication<'a> {
    Ok,
    KerberosV5,
    CleartextPassword,
    Md5Password {
        salt: [u8; 4],
    },
    Gss,
    GSSContinue {
        data: &'a [u8],
    },
    Sspi,
    Sasl {
        authentication_mechanisms: CStrList<'a>,
    },
    SaslContinue {
        data: &'a [u8],
    },
    SaslFinal {
        outcome: &'a [u8],
    },
}

impl Message for Authentication<'_> {
    const START_BYTE: Option<u8> = Some('R' as u8);
}

impl<'a> BackendMessage<'a> for Authentication<'a> {
    fn read(src: &mut &'a [u8]) -> Result<Self> {
        Ok(match read_i32(src)? {
            0 => Self::Ok,
            2 => Self::KerberosV5,
            3 => Self::CleartextPassword,
            5 => Self::Md5Password {
                salt: read_slice(4, src)?.try_into().unwrap(),
            },
            7 => Self::Gss,
            8 => Self::GSSContinue { 
                data: src,
            },
            9 => Self::Sspi,
            10 => Self::Sasl { 
                authentication_mechanisms: read_cstr_list(src)?,
            },
            11 => Self::SaslContinue {
                data: src,
            },
            12 => Self::SaslFinal {
                outcome: src,
            },
            n => return Err(Error::other(format!("unknown authentication message type: {n}"))),
        })
    }
}

pub struct BackendKeyData {
    pub secret_key: i32,
}

impl Message for BackendKeyData {
    const START_BYTE: Option<u8> = Some('K' as u8);
    const MESSAGE_LENGTH: Option<i32> = Some(12);
}

impl BackendMessage<'_> for BackendKeyData {
    fn read(src: &mut &'_ [u8]) -> Result<Self> {
        Ok(Self {
            secret_key: read_i32(src)?,
        })
    }
}

// Extended Query

pub struct Bind {
    pub destination_portal: CString,
    pub source_statement: CString,
    pub parameters_codes: Vec<i16>,
}

impl Message for Bind {
    const START_BYTE: Option<u8> = Some('B' as u8);
}
