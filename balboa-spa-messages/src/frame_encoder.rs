use crate::frame_decoder::{CRC_ENGINE, END_OF_MESSAGE, START_OF_MESSAGE};
use crate::message::{EncodeError, Message};

#[derive(Default, Debug)]
pub struct FrameEncoder {
}

impl FrameEncoder {
  pub fn new() -> Self {
    Default::default()
  }

  pub fn encode(&self, message: &Message) -> Result<Vec<u8>, EncodeError> {
    let unwrapped = message.to_bytes()?;
    let mut wrapped = Vec::with_capacity(3 + unwrapped.len());
    wrapped.push(START_OF_MESSAGE);
    wrapped.extend(&unwrapped);
    wrapped.push(CRC_ENGINE.checksum(&unwrapped));
    wrapped.push(END_OF_MESSAGE);
    Ok(wrapped)
  }
}
