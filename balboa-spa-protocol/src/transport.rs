use std::io::{Read, Write};

pub trait Transport<R: Read, W: Write> {
  fn split(self) -> (R, W);
}

pub struct StdTransport<R, W> {
  reader: R,
  writer: W,
}

impl<R, W> StdTransport<R, W> {
  pub fn new(reader: R, writer: W) -> Self {
    Self { reader, writer }
  }
}

impl <R: Read, W: Write> Transport<R, W> for StdTransport<R, W> {
  fn split(self) -> (R, W) {
    (self.reader, self.writer)
  }
}