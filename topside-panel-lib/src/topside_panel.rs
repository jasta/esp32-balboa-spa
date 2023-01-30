use std::io::{BufRead, Read, Write};
use balboa_spa_messages::framed_reader::FramedReader;
use balboa_spa_messages::framed_writer::FramedWriter;
use common_lib::transport::Transport;

pub struct TopsidePanel<R, W> {
  framed_reader: FramedReader<R>,
  framed_writer: FramedWriter<W>,
}

impl<R: Read, W: Write> TopsidePanel<R, W> {
  pub fn new(transport: impl Transport<R, W>) -> Self {
    let (raw_reader, raw_writer) = transport.split();
    let framed_reader = FramedReader::new(raw_reader);
    let framed_writer = FramedWriter::new(raw_writer);
    Self {
      framed_reader,
      framed_writer,
    }
  }
}
