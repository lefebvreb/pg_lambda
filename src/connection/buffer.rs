use std::io::{Error, Result};
use std::ops::Range;

use super::Transport;
use super::messages::{BackendMessage, FrontendMessage};
use crate::util::closed_transport;

#[derive(Debug)]
pub struct MessageWriter<'a> {
    stack: &'a mut Vec<u8>,
}

impl MessageWriter<'_> {
    pub fn add<M: FrontendMessage>(&mut self, msg: M) -> Result<()> {
        if let Some(byte) = M::PREFIX {
            self.stack.push(byte);
        }

        let start = self.stack.len();
        self.stack.extend([0; 4]);
        msg.write(self.stack)?;
        let size = (self.stack.len() - start) as i32;
        self.stack[start..start + 4].copy_from_slice(&size.to_be_bytes());

        Ok(())
    }
}

#[derive(Debug)]
pub struct AnyMessage {
    pub prefix: u8,
    body: Range<usize>,
}

#[derive(Debug)]
enum State {
    Idle,
    Closed,
    Sending { total: usize },
    ReceivingHeader,
    ReceivingBody { total: usize },
}

#[derive(Debug)]
pub struct BufTransport<T> {
    transport: T,
    stack: Vec<u8>,
    state: State,
    partial: usize,
}

impl<T: Transport> BufTransport<T> {
    /// Creates a new buffered transport.
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            stack: Vec::new(),
            state: State::Idle,
            partial: 0,
        }
    }

    /// Clears the entire stack.
    ///
    /// Calling this if `self.has_partial_data()` will lead to issues.
    pub fn clear(&mut self) {
        self.stack.clear();
    }

    /// Cuts this message and all subsequent ones from the stack.
    ///
    /// Calling this if `self.has_partial_data()` will lead to issues.
    pub fn cut(&mut self, msg: &AnyMessage) {
        self.stack.truncate(msg.body.start - 5);
    }

    /// Returns `true` iff the transport was closed.
    pub fn is_closed(&self) -> bool {
        matches!(self.state, State::Closed)
    }

    /// Returns `true` iff the stack contains partial data that need to be sent or received at its end.
    pub fn has_partial_data(&self) -> bool {
        self.partial != 0
    }

    /// Parses the given message from the stack.
    pub fn parse<'a, M: BackendMessage<'a>>(&'a self, msg: &AnyMessage) -> Result<M> {
        M::read(&self.stack[msg.body.clone()])
    }

    /// Appends `additional` zeroed bytes at the end of the stack.
    ///
    /// Calling this if `self.has_partial_data()` will lead to issues.
    fn extend_buffer(&mut self, additional: usize) {
        self.partial = additional;
        self.stack.resize(self.stack.len() + additional, 0);
    }

    /// Mark this buffered transport as closed and returns the corresponding error.
    ///
    /// The database closing the connection instead of us is always an error.
    fn set_closed(&mut self) -> Error {
        self.state = State::Closed;
        self.partial = 0;
        self.clear();
        closed_transport()
    }

    /// Send some bytes from the end of the stack.
    ///
    /// Will truncate them once it succeeds. Can be resumed.
    async fn send_bytes(&mut self, total: usize) -> Result<()> {
        while self.has_partial_data() {
            let range = (self.stack.len() - self.partial)..;
            match self.transport.write(&self.stack[range]).await? {
                0 => return Err(self.set_closed()),
                n => self.partial -= n,
            }
        }

        self.stack.truncate(self.stack.len() - total);
        self.state = State::Idle;
        Ok(())
    }

    /// Write some messages to the end of the stack then attempts to send them.
    ///
    /// If writing fails halfway through, truncated bytes will be cleared.
    pub async fn send_all(
        &mut self,
        write: impl FnOnce(MessageWriter) -> Result<()>,
    ) -> Result<()> {
        match self.state {
            State::Idle => (),
            State::Closed => return Err(closed_transport()),
            State::Sending { total } => self.send_bytes(total).await?,
            _ => panic!(),
        }

        let start = self.stack.len();
        let writer = MessageWriter {
            stack: &mut self.stack,
        };
        write(writer).inspect_err(|_| self.stack.truncate(start))?;

        let total = self.stack.len() - start;
        self.state = State::Sending { total };
        self.partial = total;
        self.send_bytes(total).await
    }

    /// Receives the first 5 bytes of a message, its prefix and length.
    ///
    /// On success, calls `receive_body` to receive the rest of the message.
    /// Can be resumed.
    async fn receive_header(&mut self) -> Result<AnyMessage> {
        while self.has_partial_data() {
            let range = (self.stack.len() - self.partial)..;
            match self.transport.read(&mut self.stack[range]).await? {
                0 => return Err(self.set_closed()),
                n => self.partial -= n,
            }
        }

        match i32::from_be_bytes(self.stack[(self.stack.len() - 4)..].try_into().unwrap()) {
            4 => {
                self.state = State::Idle;
                Ok(AnyMessage {
                    prefix: self.stack[self.stack.len() - 5],
                    body: self.stack.len()..self.stack.len(),
                })
            }
            size => {
                let total = (size - 4) as usize;
                self.state = State::ReceivingBody { total };
                self.extend_buffer(total);
                self.receive_body(total).await
            }
        }
    }

    /// Receives the rest of a message.
    ///
    /// Can be resumed.
    async fn receive_body(&mut self, total: usize) -> Result<AnyMessage> {
        while self.has_partial_data() {
            let range = (self.stack.len() - self.partial)..;
            match self.transport.read(&mut self.stack[range]).await? {
                0 => return Err(self.set_closed()),
                n => self.partial -= n,
            }
        }

        self.state = State::Idle;
        Ok(AnyMessage {
            prefix: self.stack[self.stack.len() - total - 5],
            body: (self.stack.len() - total)..self.stack.len(),
        })
    }

    /// Receives a new message.
    ///
    /// Can be resumed.
    pub async fn receive_any(&mut self) -> Result<AnyMessage> {
        match self.state {
            State::Idle => (),
            State::Closed => return Err(closed_transport()),
            State::Sending { total } => self.send_bytes(total).await?,
            State::ReceivingHeader => return self.receive_header().await,
            State::ReceivingBody { total } => return self.receive_body(total).await,
        }

        self.state = State::ReceivingHeader;
        self.extend_buffer(5);
        self.receive_header().await
    }
}
