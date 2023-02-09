use std::net::{SocketAddr, TcpListener, TcpStream};
use std::{io, thread};
use std::sync::mpsc::{SyncSender};
use std::time::Duration;
use log::{debug, warn};
use balboa_spa_messages::framed_reader::FramedReader;
use balboa_spa_messages::framed_writer::FramedWriter;
use crate::broadcaster::BroadcastReceiver;
use crate::command::Command;
use crate::event::Event;

const TCP_PORT: u16 = 4257;

const READ_TIMEOUT: Duration = Duration::from_secs(120);

pub(crate) struct TcpListenerHandler {
  listener: TcpListener,
  commands_tx: SyncSender<Command>,
  events_rx: BroadcastReceiver<Event>,
}

impl TcpListenerHandler {
  pub fn setup(
      commands_tx: SyncSender<Command>,
      events_rx: BroadcastReceiver<Event>
  ) -> io::Result<Self> {
    let socket = TcpListener::bind(format!("0.0.0.0:{}", TCP_PORT))?;
    Ok(Self {
      listener: socket,
      commands_tx,
      events_rx,
    })
  }

  pub fn run_loop(self) -> anyhow::Result<()> {
    loop {
      let (stream, peer) = self.listener.accept()?;
      debug!("Accepted connection from: {peer}");

      stream.set_read_timeout(Some(READ_TIMEOUT))?;

      let stream_handler = TcpStreamHandler {
        stream,
        peer,
        commands_tx: self.commands_tx.clone(),
        events_rx: self.events_rx.clone(),
      };

      thread::Builder::new()
          .name(format!("TcpHandler-{peer}").to_owned())
          .spawn(move || stream_handler.run_loop())
          .unwrap();
    }
  }
}

struct TcpStreamHandler {
  stream: TcpStream,
  peer: SocketAddr,
  commands_tx: SyncSender<Command>,
  events_rx: BroadcastReceiver<Event>,
}

impl TcpStreamHandler {
  pub fn run_loop(self) {
    crossbeam::thread::scope(|s| {
      let reader = TcpStreamReader {
        reader: FramedReader::new(&self.stream),
        commands_tx: self.commands_tx,
      };
      let writer = TcpStreamWriter {
        writer: FramedWriter::new(&self.stream),
        events_rx: self.events_rx,
      };

      let writer_thread = s.builder()
          .name(format!("TcpWriter-{}", self.peer).to_owned())
          .spawn(|_| {
            if let Err(e) = writer.run_loop() {
              warn!("TcpWriter: {e}");
            }
          })
          .unwrap();

      if let Err(e) = reader.run_loop() {
        warn!("TcpReader: {e}");
      }

      writer_thread.join().unwrap()
    }).unwrap();
  }
}

struct TcpStreamReader<'a> {
  reader: FramedReader<&'a TcpStream>,
  commands_tx: SyncSender<Command>,
}

impl<'a> TcpStreamReader<'a> {
  pub fn run_loop(mut self) -> anyhow::Result<()> {
    loop {
      let message = self.reader.next_message()?;
      self.commands_tx.send(Command::RelayIpMessage(message))?;
    }
  }
}

struct TcpStreamWriter<'a> {
  writer: FramedWriter<&'a TcpStream>,
  events_rx: BroadcastReceiver<Event>,
}

impl<'a> TcpStreamWriter<'a> {
  pub fn run_loop(mut self) -> anyhow::Result<()> {
    loop {
      match self.events_rx.rx().recv()? {
        Event::RelayMainboardMessage(m) => {
          self.writer.write(&m)?
        }
      }
    }
  }
}
