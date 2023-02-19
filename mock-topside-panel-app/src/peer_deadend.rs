use std::io::{Read, Write};
use std::mem;
use common_lib::transport::{StdTransport, Transport};
use crate::peer_runner::{PeerControlHandle, PeerManager, PeerRunner};

pub fn new_peer_deadend<R, W>(transport: StdTransport<R, W>) -> PeerManager
where
    R: Read + Send + 'static,
    W: Write + 'static,
{
  let (reader, writer) = transport.split();
  PeerManager {
    control_handle: Box::new(DeadendControlHandle {
      writer: Some(writer),
    }),
    runner: Box::new(DeadendRunner(reader)),
  }
}

struct DeadendControlHandle<W> {
  writer: Option<W>,
}

impl<W: Write> PeerControlHandle for DeadendControlHandle<W> {
  fn request_shutdown(&mut self) {
    // Drop the writer, that will cause the reader on the other side to halt which will drop
    // the other writer and, then, finally, our PeerRunner will halt.
    mem::take(&mut self.writer);
  }
}

struct DeadendRunner<R>(R);
impl<R: Read> PeerRunner for DeadendRunner<R> {
  fn run_loop(mut self: Box<Self>) -> anyhow::Result<()> {
    let mut buf = [0u8; 256];
    loop {
      let _ = self.0.read(&mut buf)?;
    }
  }
}
