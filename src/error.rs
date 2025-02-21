use std::io;
use std::str::Utf8Error;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("protocol error: {0}")]
    Protocol(#[from] io::Error),
    #[error("validation error: {0}")]
    Validation(#[from] ValidationError)
}

#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("string is not valid utf8: {0}")]
    Utf8(#[from] Utf8Error),
}

pub type Result<T> = std::result::Result<T, Error>;
