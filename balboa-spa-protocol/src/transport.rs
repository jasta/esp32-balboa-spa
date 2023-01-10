use std::io::{Read, Write};

pub trait Transport<R: Read, W: Write> {
  fn split(self) -> (R, W);
}
