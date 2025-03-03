use std::io::{Error, Result};
use std::ops::Range;

use super::Transport;
use super::messages::{BackendMessage, FrontendMessage};
use super::util::closed_transport;

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

pub struct AnyMessage {
    pub prefix: u8,
    body: Range<usize>,
}

#[derive(Debug)]
enum State {
    Idle,
    Closed,
    Sending { total: i32, remaining: i32 },
    ReceivingHead { remaining: i32 },
    ReceivingBody { total: i32, remaining: i32 },
}

#[derive(Debug)]
pub struct Buffer<T> {
    transport: T,
    stack: Vec<u8>,
    state: State,
}

impl<T: Transport> Buffer<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            stack: Vec::new(),
            state: State::Idle,
        }
    }

    fn extend_buffer(&mut self, additional: i32) {
        self.stack.resize(self.stack.len() + additional as usize, 0);
    }

    fn set_closed(&mut self) -> Error {
        self.state = State::Closed;
        closed_transport()
    }

    pub fn clear(&mut self) {
        self.stack.clear();
    }

    pub fn cut(&mut self, msg: &AnyMessage) {
        self.stack.truncate(msg.body.start - 5);
    }

    pub fn is_closed(&self) -> bool {
        matches!(self.state, State::Closed)
    }

    pub fn parse<'a, M: BackendMessage<'a>>(&'a self, msg: &AnyMessage) -> Result<M> {
        M::read(&self.stack[msg.body.clone()])
    }

    async fn send_bytes(&mut self, total: i32, mut remaining: i32) -> Result<()> {
        loop {
            let range = (self.stack.len() - remaining as usize)..;
            match self.transport.write(&self.stack[range]).await? {
                0 => return Err(self.set_closed()),
                n if n == remaining => break,
                n => remaining -= n,
            }
            self.state = State::Sending { total, remaining };
        }

        self.stack.truncate(self.stack.len() - total as usize);
        self.state = State::Idle;
        Ok(())
    }

    pub async fn send_all(
        &mut self,
        write: impl FnOnce(MessageWriter) -> Result<()>,
    ) -> Result<()> {
        match self.state {
            State::Idle => (),
            State::Closed => return Err(closed_transport()),
            State::Sending { total, remaining } => self.send_bytes(total, remaining).await?,
            _ => panic!(),
        }

        let start = self.stack.len();
        let writer = MessageWriter {
            stack: &mut self.stack,
        };
        write(writer).inspect_err(|_| self.stack.truncate(start))?;

        let total = (self.stack.len() - start) as i32;
        self.state = State::Sending {
            total,
            remaining: total,
        };
        self.send_bytes(total, total).await
    }

    async fn receive_head(&mut self, mut remaining: i32) -> Result<AnyMessage> {
        loop {
            let range = (self.stack.len() - remaining as usize)..;
            match self.transport.read(&mut self.stack[range]).await? {
                0 => return Err(self.set_closed()),
                n if n == remaining => break,
                n => remaining -= n,
            }
            self.state = State::ReceivingHead { remaining };
        }

        match i32::from_be_bytes(self.stack[(self.stack.len() - 4)..].try_into().unwrap()) {
            4 => {
                self.state = State::Idle;
                Ok(AnyMessage {
                    prefix: self.stack[self.stack.len() - 5],
                    body: Range::default(),
                })
            }
            size => {
                let total = size - 4;
                self.state = State::ReceivingBody {
                    total,
                    remaining: total,
                };
                self.extend_buffer(total);
                self.receive_body(total, total).await
            }
        }
    }

    async fn receive_body(&mut self, total: i32, mut remaining: i32) -> Result<AnyMessage> {
        loop {
            let range = (self.stack.len() - remaining as usize)..;
            match self.transport.read(&mut self.stack[range]).await? {
                0 => return Err(self.set_closed()),
                n if n == remaining => break,
                n => remaining -= n,
            }
            self.state = State::ReceivingBody { total, remaining };
        }

        self.state = State::Idle;
        Ok(AnyMessage {
            prefix: self.stack[self.stack.len() - total as usize - 5],
            body: (self.stack.len() - total as usize)..self.stack.len(),
        })
    }

    pub async fn receive_any(&mut self) -> Result<AnyMessage> {
        match self.state {
            State::Idle => (),
            State::Closed => return Err(closed_transport()),
            State::Sending { total, remaining } => self.send_bytes(total, remaining).await?,
            State::ReceivingHead { remaining } => return self.receive_head(remaining).await,
            State::ReceivingBody {
                total: size,
                remaining,
            } => {
                return self.receive_body(size, remaining).await;
            }
        }

        self.state = State::ReceivingHead { remaining: 5 };
        self.extend_buffer(5);
        self.receive_head(5).await
    }
}
