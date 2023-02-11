use std::net::{SocketAddr, TcpListener, TcpStream};
use std::{io, thread};
use std::sync::mpsc::{SyncSender};
use std::time::Duration;
use log::{debug, info, warn};
use balboa_spa_messages::framed_reader::FramedReader;
use balboa_spa_messages::framed_writer::FramedWriter;
use common_lib::message_logger::{MessageDirection, MessageLogger};
use crate::broadcaster::BroadcastReceiver;
use crate::command::Command;
use crate::event::RelayEvent;

const TCP_PORT: u16 = 4257;

const READ_TIMEOUT: Duration = Duration::from_secs(120);

pub(crate) struct TcpListenerHandler {
  logger: MessageLogger,
  listener: TcpListener,
  commands_tx: SyncSender<Command>,
  events_rx: BroadcastReceiver<RelayEvent>,
}

impl TcpListenerHandler {
  pub fn setup(
      logger: MessageLogger,
      commands_tx: SyncSender<Command>,
      events_rx: BroadcastReceiver<RelayEvent>
  ) -> io::Result<Self> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", TCP_PORT))?;
    Ok(Self {
      logger,
      listener,
      commands_tx,
      events_rx,
    })
  }

  pub fn run_loop(self) -> anyhow::Result<()> {
    loop {
      let (stream, peer) = self.listener.accept()?;
      info!("Accepted connection from: {peer}");

      stream.set_read_timeout(Some(READ_TIMEOUT))?;

      let stream_handler = TcpStreamHandler {
        stream,
        peer,
        commands_tx: self.commands_tx.clone(),
        events_rx: self.events_rx.clone(),
        logger: self.logger.clone(),
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
  events_rx: BroadcastReceiver<RelayEvent>,
  logger: MessageLogger,
}

impl TcpStreamHandler {
  pub fn run_loop(self) {
    crossbeam::thread::scope(|s| {
      let reader = TcpStreamReader {
        reader: FramedReader::new(&self.stream),
        commands_tx: self.commands_tx,
        logger: &self.logger,
      };
      let writer = TcpStreamWriter {
        writer: FramedWriter::new(&self.stream),
        events_rx: self.events_rx,
        logger: &self.logger,
      };

      let writer_thread = s.builder()
          .name(format!("TcpWriter-{}", self.peer))
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
  logger: &'a MessageLogger,
}

impl<'a> TcpStreamReader<'a> {
  pub fn run_loop(mut self) -> anyhow::Result<()> {
    loop {
      let message = self.reader.next_message()?;
      self.logger.log(MessageDirection::Inbound, &message);
      self.commands_tx.send(Command::RelayIpMessage(message))?;
    }
  }
}

struct TcpStreamWriter<'a> {
  writer: FramedWriter<&'a TcpStream>,
  events_rx: BroadcastReceiver<RelayEvent>,
  logger: &'a MessageLogger,
}

impl<'a> TcpStreamWriter<'a> {
  pub fn run_loop(mut self) -> anyhow::Result<()> {
    loop {
      match self.events_rx.rx().recv()? {
        RelayEvent::ReceivedMainboardMessage(message) => {
          self.logger.log(MessageDirection::Outbound, &message);
          self.writer.write(&message)?
        }
      }
    }
  }
}
