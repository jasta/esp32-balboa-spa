use std::{io, thread};
use std::io::{Read, Write};
use std::sync::mpsc::{channel, Receiver, SendError, sync_channel, SyncSender};
use anyhow::anyhow;
use log::{debug, error, info, warn};
use balboa_spa_messages::channel::Channel;
use balboa_spa_messages::framed_reader::FramedReader;
use balboa_spa_messages::framed_writer::FramedWriter;
use balboa_spa_messages::message::Message;
use balboa_spa_messages::message_types::{MessageType, WifiModuleIdentificationMessage};
use common_lib::channel_filter::ChannelFilter;
use common_lib::message_logger::{MessageDirection, MessageLogger};
use common_lib::transport::Transport;
use common_lib::view_model_event_handle::ViewModelEventHandle;
use crate::app_state::AppState;
use crate::broadcaster::{broadcast_channel, BroadcastSender};
use crate::command::Command;
use crate::discovery_handler::DiscoveryHandler;
use crate::handling_error::HandlingError;
use crate::handling_error::HandlingError::{FatalError, ShutdownRequested};
use crate::relay_event::RelayEvent;
use crate::relay_event::RelayEvent::MessageForIpClient;
use crate::tcp_handler::TcpListenerHandler;
use crate::view_model::ViewModel;
use crate::wifi_handler::WifiHandler;
use crate::wifi_manager::WifiManager;

pub struct WifiModuleClient<R, W, WIFI> {
  framed_reader: FramedReader<R>,
  framed_writer: FramedWriter<W>,
  wifi_manager: WIFI,
}

impl <R: Read, W: Write, WIFI: WifiManager<'static>> WifiModuleClient<R, W, WIFI> {
  pub fn new(transport: impl Transport<R, W>, wifi_manager: WIFI) -> Self {
    let (raw_reader, raw_writer) = transport.split();
    let framed_reader = FramedReader::new(raw_reader);
    let framed_writer = FramedWriter::new(raw_writer);
    Self {
      framed_reader,
      framed_writer,
      wifi_manager,
    }
  }

  pub fn into_runner(
      self
  ) -> io::Result<(ViewModelEventHandle<ViewModel>, Runner<R, W, WIFI>)> {
    let (commands_tx, commands_rx) = sync_channel(32);
    let (relay_events_tx, relay_events_rx) =
        broadcast_channel(16);
    let message_reader = MessageReader {
      framed_reader: self.framed_reader,
      commands_tx: commands_tx.clone(),
    };
    let advertisement = self.wifi_manager.advertisement();
    let event_handler = EventHandler {
      framed_writer: self.framed_writer,
      mainboard_logger: MessageLogger::new(module_path!()),
      commands_rx,
      events_tx: relay_events_tx,
      state: AppState::new(advertisement.clone()),
    };
    let discovery_handler = DiscoveryHandler::setup(advertisement.clone())?;
    let tcp_handler = TcpListenerHandler::setup(
        MessageLogger::new("ip_relay"),
        commands_tx,
        relay_events_rx)?;
    let (view_events_tx, view_model_event_handle) =
        ViewModelEventHandle::new();
    let wifi_handler = WifiHandler::new(
        self.wifi_manager,
        view_events_tx);
    let runner = Runner {
      message_reader,
      event_handler,
      discovery_handler,
      tcp_handler,
      wifi_handler,
    };
    Ok((view_model_event_handle, runner))
  }
}

pub struct Runner<R, W, WIFI> {
  message_reader: MessageReader<R>,
  event_handler: EventHandler<W>,
  discovery_handler: DiscoveryHandler,
  tcp_handler: TcpListenerHandler,
  wifi_handler: WifiHandler<WIFI>,
}

impl <R, W, WIFI> Runner<R, W, WIFI>
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
    WIFI: WifiManager<'static> + Send + 'static
{
  pub fn run_loop(self) -> anyhow::Result<()> {
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

    let wifi_thread = thread::Builder::new()
        .name("WifiThread".into())
        .spawn(move || {
          // Don't actually panic here as we informed the model of this state
          // and have to trust the user to resolve the issue from here on.
          if let Err(e) = self.wifi_handler.run_loop() {
            error!("Wi-Fi module encountered fatal error: {e}!");
          }
        })
        .unwrap();

    let result = self.event_handler.run_loop();

    reader_thread.join().unwrap();
    discovery_thread.join().unwrap();
    tcp_thread.join().unwrap();
    wifi_thread.join().unwrap();

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
  events_tx: BroadcastSender<RelayEvent>,
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

    while let Some(for_relay) =
        self.state.wifi_state_machine.context.for_relay_messages.pop_front() {
      self.enqueue_message_to_app(for_relay);
    }

    Ok(())
  }

  fn handle_relay_message(&mut self, message: Message) -> Result<(), HandlingError> {
    let mt = MessageType::try_from(&message)?;

    match mt {
      MessageType::ExistingClientRequest() => {
        if message.channel == Channel::WifiModule {
          info!("Interpreting ExistingClientRequest as Wifi Config request...");
          self.enqueue_message_to_app(MessageType::WifiModuleConfigurationResponse(
            WifiModuleIdentificationMessage {
              mac: self.state.advertisement.mac,
            }
          ).to_message(Channel::WifiModule)?);
        } else {
          info!("Got existing channel request on channel={:?} ???", message.channel);
        }
      }
      mt => {
        self.enqueue_message_to_board(mt);
      }
    }

    Ok(())
  }

  fn enqueue_message_to_board(&mut self, message: MessageType) {
    self.state.wifi_state_machine.context.outbound_messages.push_back(message);
  }

  fn enqueue_message_to_app(&mut self, mut message: Message) {
    self.events_tx.send_to_all(&MessageForIpClient(message));
  }
}