use std::collections::HashMap;
use std::fmt::Debug;
use std::io::{Read, Write};
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, SendError, SyncSender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};
use anyhow::anyhow;
use log::{debug, error, info, warn};
use lvgl::Event;
use measurements::Temperature;
use balboa_spa_messages::channel::Channel;
use balboa_spa_messages::framed_reader::FramedReader;
use balboa_spa_messages::framed_writer::FramedWriter;
use balboa_spa_messages::message::Message;
use balboa_spa_messages::message_types::{ConfigurationResponseMessage, InformationResponseMessage, MessageType, PayloadEncodeError, PayloadParseError, StatusUpdateMessage};
use balboa_spa_messages::temperature::Direction;
use common_lib::message_logger::{MessageDirection, MessageLogger};
use common_lib::transport::Transport;
use HandlingError::ShutdownRequested;
use crate::network::app_state::AppState;
use common_lib::channel_filter::ChannelFilter;
use common_lib::view_model_event_handle::{ViewEvent, ViewModelEventHandle};
use crate::network::handling_error::HandlingError;
use crate::network::handling_error::HandlingError::FatalError;
use crate::model::view_model::ViewModel;
use crate::model::button::Button;

pub struct TopsidePanelClient<R, W> {
  framed_reader: FramedReader<R>,
  framed_writer: FramedWriter<W>,
}

impl<R: Read, W: Write> TopsidePanelClient<R, W> {
  pub fn new(transport: impl Transport<R, W>) -> Self {
    let (raw_reader, raw_writer) = transport.split();
    let framed_reader = FramedReader::new(raw_reader);
    let framed_writer = FramedWriter::new(raw_writer);
    Self {
      framed_reader,
      framed_writer,
    }
  }

  pub fn into_runner(self) -> (ControlHandle, ViewModelEventHandle<ViewModel>, Runner<R, W>) {
    let (commands_tx, commands_rx) = mpsc::sync_channel(32);
    let (events_tx, events_rx) = mpsc::channel();
    let message_reader = MessageReader {
      message_tx: commands_tx.clone(),
      framed_reader: self.framed_reader,
    };

    let init_view_model = ViewModel::default();
    let _ = events_tx.send(ViewEvent::ModelUpdated(init_view_model.clone()));
    let event_handler = EventHandler {
      commands_rx,
      events_tx,
      framed_writer: self.framed_writer,
      message_logger: MessageLogger::new(module_path!()),
      last_view_model: init_view_model,
      state: AppState::default(),
    };

    let control_handle = ControlHandle { commands_tx };
    let event_handle = ViewModelEventHandle { events_rx };
    let runner = Runner { message_reader, event_handler };
    (control_handle, event_handle, runner)
  }
}

#[derive(Clone)]
pub struct ControlHandle {
  commands_tx: SyncSender<Command>,
}

impl ControlHandle {
  pub fn send_button_pressed(&self, button: Button) {
    let _ = self.commands_tx.send(Command::ButtonPressed(button));
  }

  /// Optional API to send in Wi-Fi model updates that can be rendered by the topside panel
  pub fn send_wifi_model(&self, model: wifi_module_lib::view_model::ViewModel) {
    let _ = self.commands_tx.send(Command::WifiModelUpdated(model));
  }

  pub fn request_shutdown(&self) {
    let _ = self.commands_tx.send(Command::Shutdown);
  }
}

impl Drop for ControlHandle {
  fn drop(&mut self) {
    self.request_shutdown();
  }
}

pub struct Runner<R, W> {
  message_reader: MessageReader<R>,
  event_handler: EventHandler<W>,
}

impl <R: Read + Send + 'static, W: Write + Send + 'static> Runner<R, W> {
  pub fn run_loop(mut self) -> anyhow::Result<()> {
    let message_reader = thread::Builder::new()
        .name("MessageReader".into())
        .spawn(move || {
          if let Err(e) = self.message_reader.run_loop() {
            warn!("Message reader yielded: {e}");
          }
        })
        .unwrap();

    let result = self.event_handler.run_loop();

    message_reader.join().unwrap();

    result
  }
}

struct MessageReader<R> {
  framed_reader: FramedReader<R>,
  message_tx: SyncSender<Command>,
}

impl<R: Read + Send> MessageReader<R> {
  pub fn run_loop(mut self) -> Result<(), SendError<Command>> {
    loop {
      match self.framed_reader.next_message() {
        Ok(message) => {
          self.message_tx.send(Command::ReceivedMessage(message))?;
        }
        Err(e) => {
          self.message_tx.send(Command::ReadError(anyhow!("{:?}", e)))?;
          break;
        }
      }
    }
    Ok(())
  }
}

struct EventHandler<W> {
  framed_writer: FramedWriter<W>,
  message_logger: MessageLogger,
  commands_rx: Receiver<Command>,
  events_tx: Sender<ViewEvent<ViewModel>>,
  last_view_model: ViewModel,
  state: AppState,
}

impl <W: Write + Send> EventHandler<W> {
  pub fn run_loop(mut self) -> anyhow::Result<()> {
    loop {
      let command = self.commands_rx.recv()?;

      let result = match command {
        Command::ReceivedMessage(m) => self.handle_message(m),
        Command::ReadError(e) => Err(FatalError(e.to_string())),
        Command::ButtonPressed(b) => {
          self.handle_button(b);
          Ok(())
        },
        Command::WifiModelUpdated(model) => {
          self.handle_wifi_model(model);
          Ok(())
        },
        Command::Shutdown => Err(ShutdownRequested),
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

  fn handle_message(&mut self, message: Message) -> Result<(), HandlingError> {
    self.message_logger.log(MessageDirection::Inbound, &message);

    let mt = MessageType::try_from(&message)
        .map_err(|e| HandlingError::UnexpectedPayload(e.to_string()))?;

    let state_snapshot = self.state.fast_snapshot();
    self.state.cts_state_machine.handle_message(&mut self.framed_writer, &self.message_logger, &message.channel, &mt)?;
    if let Some(channel) = self.state.cts_state_machine.take_got_channel() {
      info!("Setting channel filter for {:?}", channel);
      self.state.topside_state_machine.set_channel_filter(
          ChannelFilter::RelevantTo(vec![channel]));
    }
    self.state.topside_state_machine.handle_message(&mut self.framed_writer, &self.message_logger, &message.channel, &mt)?;
    if self.state.fast_snapshot() != state_snapshot {
      self.maybe_emit_view_model();
    }

    Ok(())
  }

  fn maybe_emit_view_model(&mut self) {
    let model = self.state.generate_view_model();
    if self.last_view_model != model {
      info!("Emitting new model: {model:?}");
      self.last_view_model = model.clone();
      let _ = self.events_tx.send(ViewEvent::ModelUpdated(model));
    }
  }

  fn handle_button(&mut self, button: Button) {
    match button {
      Button::Up => {
        let _ = self.handle_temp_updown(Direction::Up);
      },
      Button::Down => {
        let _ = self.handle_temp_updown(Direction::Down);
      },
      _ => warn!("handle_button({button:?}): not implemented!"),
    }
  }

  fn handle_temp_updown(&mut self, direction: Direction) -> Result<(), ()> {
    let (current_temp, range) = self.state.topside_state_machine.context.status
        .as_ref()
        .map(|m| {
          (&m.message.v1.set_temperature,
            &m.message.v1.temperate_range)
        })
        .ok_or(())?;
    let min_maxes = self.state.topside_state_machine.context.settings0x04
        .as_ref()
        .map(|m| &m.min_max_temps)
        .ok_or(())?;
    let temperature = match current_temp.step(direction, range, min_maxes) {
      Ok(t) => t,
      Err(e) => {
        warn!("Can't set temp: {e}");
        return Err(());
      }
    };
    info!("Setting temp to: {temperature:?}");
    let mt = MessageType::SetTemperatureRequest { temperature };
    self.enqueue_message(mt);
    Ok(())
  }

  fn enqueue_message(&mut self, message: MessageType) {
    self.state.topside_state_machine.context.outbound_messages.push_back(message);
  }

  fn handle_wifi_model(&mut self, model: wifi_module_lib::view_model::ViewModel) {
    self.state.wifi_model = Some(model);
    self.maybe_emit_view_model();
  }
}

#[derive(Debug)]
enum Command {
  ReceivedMessage(Message),
  WifiModelUpdated(wifi_module_lib::view_model::ViewModel),
  ReadError(anyhow::Error),
  ButtonPressed(Button),
  Shutdown,
}
