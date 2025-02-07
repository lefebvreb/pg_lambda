use std::io::Result;

pub mod messages;
mod util;

pub const PROTOCOL_VERSION: i32 = 196608;

pub trait Message: Sized {
    /// Start byte if there is one.
    const START_BYTE: Option<u8> = None;

    /// Message length if it is known.
    const MESSAGE_LENGTH: Option<i32> = None;
}

pub trait BackendMessage<'a>: Message {
    fn read(src: &mut &'a [u8]) -> Result<Self>;
}

pub trait FrontendMessage: Message {
    fn write(&self, dst: &mut Vec<u8>) -> Result<()>;
}
