use std::io;
use std::io::{ErrorKind, Read};
use anyhow::anyhow;
use log::debug;
use crate::frame_decoder::FrameDecoder;
use crate::message::Message;

#[derive(Debug)]
pub struct FramedReader<R> {
  raw_reader: R,
  framed_reader: FrameDecoder,
  buf: [u8; 32],
  debug: bool,
}

impl<R: Read> FramedReader<R> {
  pub fn new(raw_reader: R) -> Self {
    Self {
      raw_reader,
      framed_reader: FrameDecoder::new(),
      buf: [0u8; 32],
      debug: false,
    }
  }

  pub fn set_debug(mut self, enable: bool) -> Self {
    self.debug = enable;
    self
  }

  pub fn next_message(&mut self) -> io::Result<Message> {
    loop {
      match self.raw_reader.read(self.buf.as_mut_slice())? {
        n if n == 0 => {
          return Err(io::Error::new(ErrorKind::UnexpectedEof, anyhow!("Unexpected EOF")))
        },
        n => {
          for b in &self.buf[0..n] {
            if self.debug {
              debug!("Got {b:02X}");
            }
            if let Some(message) = self.framed_reader.accept(*b) {
              return Ok(message);
            }
          }
        }
      }
    }
  }
}
