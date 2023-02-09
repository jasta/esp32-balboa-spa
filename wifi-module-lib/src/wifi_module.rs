use std::{io, thread};
use std::io::{Read, Write};
use std::sync::mpsc::{Receiver, SendError, sync_channel, SyncSender};
use anyhow::anyhow;
use log::{debug, error, info, warn};
use balboa_spa_messages::framed_reader::FramedReader;
use balboa_spa_messages::framed_writer::FramedWriter;
use balboa_spa_messages::message::Message;
use balboa_spa_messages::message_types::MessageType;
use common_lib::channel_filter::ChannelFilter;
use common_lib::message_logger::{MessageDirection, MessageLogger};
use common_lib::transport::Transport;
use crate::advertisement::Advertisement;
use crate::app_state::AppState;
use crate::broadcaster::{broadcast_channel, BroadcastSender};
use crate::command::Command;
use crate::discovery_handler::DiscoveryHandler;
use crate::event::Event;
use crate::event::Event::RelayMainboardMessage;
use crate::handling_error::HandlingError;
use crate::handling_error::HandlingError::{FatalError, ShutdownRequested};
use crate::tcp_handler::TcpListenerHandler;

pub struct WifiModule<R, W> {
  framed_reader: FramedReader<R>,
  framed_writer: FramedWriter<W>,
  advertisement: Advertisement,
}

impl <R: Read, W: Write> WifiModule<R, W> {
  pub fn new(transport: impl Transport<R, W>, advertisement: Advertisement) -> io::Result<Self> {
    let (raw_reader, raw_writer) = transport.split();
    let framed_reader = FramedReader::new(raw_reader);
    let framed_writer = FramedWriter::new(raw_writer);
    Ok(Self {
      framed_reader,
      framed_writer,
      advertisement,
    })
  }

  pub fn into_runner(self) -> io::Result<Runner<R, W>> {
    let (commands_tx, commands_rx) = sync_channel(32);
    let (events_tx, events_rx) = broadcast_channel(16);
    let message_reader = MessageReader {
      framed_reader: self.framed_reader,
      commands_tx: commands_tx.clone(),
    };
    let event_handler = EventHandler {
      framed_writer: self.framed_writer,
      mainboard_logger: MessageLogger::new(module_path!()),
      commands_rx,
      events_tx,
      state: AppState::default(),
    };
    let discovery_handler = DiscoveryHandler::setup(self.advertisement)?;
    let tcp_handler = TcpListenerHandler::setup(
        MessageLogger::new("ip_relay"),
        commands_tx,
        events_rx)?;
    Ok(Runner {
      message_reader,
      event_handler,
      discovery_handler,
      tcp_handler,
    })
  }
}

pub struct Runner<R, W> {
  message_reader: MessageReader<R>,
  event_handler: EventHandler<W>,
  discovery_handler: DiscoveryHandler,
  tcp_handler: TcpListenerHandler,
}

impl <R: Read + Send + 'static, W: Write + Send + 'static> Runner<R, W> {
  pub fn run_loop(mut self) -> anyhow::Result<()> {
    let reader_thread = thread::Builder::new()
        .name("MessageReader".into())
        .spawn(move || {
          if let Err(e) = self.message_reader.run_loop() {
            warn!("Message reader yielded: {e}");
          }
        })
        .unwrap();

    let discovery_thread = thread::Builder::new()
        .name("DiscoveryThread".into())
        .spawn(move || {
          self.discovery_handler.run_loop().unwrap()
        })
        .unwrap();

    let tcp_thread = thread::Builder::new()
        .name("TcpListener".into())
        .spawn(move || {
          self.tcp_handler.run_loop().unwrap()
        })
        .unwrap();

    let result = self.event_handler.run_loop();

    reader_thread.join().unwrap();
    discovery_thread.join().unwrap();
    tcp_thread.join().unwrap();

    result
  }
}

struct MessageReader<R> {
  framed_reader: FramedReader<R>,
  commands_tx: SyncSender<Command>,
}

impl<R: Read + Send> MessageReader<R> {
  pub fn run_loop(mut self) -> Result<(), SendError<Command>> {
    loop {
      match self.framed_reader.next_message() {
        Ok(message) => {
          self.commands_tx.send(Command::ReceivedMainboardMessage(message))?;
        }
        Err(e) => {
          self.commands_tx.send(Command::ReadError(anyhow!("{:?}", e)))?;
          break;
        }
      }
    }
    Ok(())
  }
}

struct EventHandler<W> {
  framed_writer: FramedWriter<W>,
  mainboard_logger: MessageLogger,
  commands_rx: Receiver<Command>,
  events_tx: BroadcastSender<Event>,
  state: AppState,
}

impl <W: Write + Send> EventHandler<W> {
  pub fn run_loop(mut self) -> anyhow::Result<()> {
    loop {
      let command = self.commands_rx.recv()?;

      let result = match command {
        Command::ReceivedMainboardMessage(m) => self.handle_mainboard_message(m),
        Command::ReadError(e) => Err(FatalError(e.to_string())),
        Command::Shutdown => Err(ShutdownRequested),
        Command::RelayIpMessage(m) => self.handle_relay_message(m),
      };

      if let Err(ref e) = result {
        match e {
          FatalError(m) => {
            error!("Fatal error: {m}");
            result?
          }
          ShutdownRequested => {
            info!("Graceful shutdown requested...");
            return Ok(())
          }
          _ => error!("Got {e}"),
        }
      }
    }
  }

  fn handle_mainboard_message(&mut self, message: Message) -> Result<(), HandlingError> {
    self.mainboard_logger.log(MessageDirection::Inbound, &message);

    let mt = MessageType::try_from(&message)
        .map_err(|e| HandlingError::UnexpectedPayload(e.to_string()))?;

    self.state.cts_state_machine.handle_message(&mut self.framed_writer, &self.mainboard_logger, &message.channel, &mt)?;
    if let Some(channel) = self.state.cts_state_machine.take_got_channel() {
      info!("Setting channel filter for {:?}", channel);
      self.state.wifi_state_machine.set_channel_filter(
        ChannelFilter::RelevantTo(vec![Channel::WifiModule, channel]));
    }
    self.state.wifi_state_machine.handle_message(&mut self.framed_writer, &self.mainboard_logger, &message.channel, &mt)?;

    for message in self.state.wifi_state_machine.context.for_relay_messages.drain(..) {
      self.events_tx.send_to_all(&RelayMainboardMessage(message));
    }

    Ok(())
  }

  fn handle_relay_message(&mut self, message: Message) -> Result<(), HandlingError> {
    // Note that we implicitly drop the channel as we will always relay with our own
    // channel instead of the one from the hot tub.  Kind of ridiculous that the channel
    // concept was copied to the IP protocol if you ask me...
    let mt = MessageType::try_from(&message)?;
    self.enqueue_message(mt);

    Ok(())
  }

  fn enqueue_message(&mut self, message: MessageType) {
    self.state.wifi_state_machine.context.outbound_messages.push_back(message);
  }
}