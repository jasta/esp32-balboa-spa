use std::io::Write;
use log::debug;
use crate::frame_encoder::FrameEncoder;
use crate::message::Message;

#[derive(Debug)]
pub struct FramedWriter<W> {
  raw_writer: W,
  framed_writer: FrameEncoder,
}

impl<W: Write> FramedWriter<W> {
  pub fn new(raw_writer: W) -> Self {
    Self {
      raw_writer,
      framed_writer: FrameEncoder::new(),
    }
  }

  pub fn write(&mut self, message: &Message) -> anyhow::Result<()> {
    let encoded = self.framed_writer.encode(message)?;
    self.raw_writer.write_all(&encoded)?;
    self.raw_writer.flush()?;
    debug!("Wrote {} bytes...", encoded.len());
    Ok(())
  }
}
