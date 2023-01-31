use std::collections::HashMap;
use std::fmt::Debug;
use std::io::{Read, Write};
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender, SendError, SyncSender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};
use anyhow::anyhow;
use log::{debug, error, info, warn};
use measurements::Temperature;
use balboa_spa_messages::channel::Channel;
use balboa_spa_messages::framed_reader::FramedReader;
use balboa_spa_messages::framed_writer::FramedWriter;
use balboa_spa_messages::message::Message;
use balboa_spa_messages::message_types::{ConfigurationResponseMessage, InformationResponseMessage, MessageType, PayloadEncodeError, PayloadParseError, StatusUpdateMessage};
use common_lib::transport::Transport;
use HandlingError::ShutdownRequested;
use crate::app_state::AppState;
use crate::topside_state_machine::{TopsideStateKind, TopsideStateMachine};
use crate::cts_state_machine::{CtsStateKind, CtsStateMachine};
use crate::handling_error::HandlingError;
use crate::handling_error::HandlingError::FatalError;
use crate::message_state_machine::MessageHandlingError;
use crate::view_model::ViewModel;

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

  pub fn into_runner(self) -> (ControlHandle, ViewModelEventHandle, Runner<R, W>) {
    let (commands_tx, commands_rx) = mpsc::sync_channel(32);
    let (events_tx, events_rx) = mpsc::channel();
    let message_reader = MessageReader {
      message_tx: commands_tx.clone(),
      framed_reader: self.framed_reader,
    };
    let event_handler = EventHandler {
      commands_rx,
      events_tx,
      framed_writer: self.framed_writer,
      last_view_model: None,
      state: AppState::default(),
    };

    let control_handle = ControlHandle { commands_tx };
    let event_handle = ViewModelEventHandle { events_rx };
    let runner = Runner { message_reader, event_handler };
    (control_handle, event_handle, runner)
  }
}

pub struct ControlHandle {
  commands_tx: SyncSender<Command>,
}

impl ControlHandle {
  pub fn send_button_pressed(&self, button: Button) {
    let _ = self.commands_tx.send(Command::ButtonPressed(button));
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

pub struct ViewModelEventHandle {
  events_rx: Receiver<Event>,
}

impl ViewModelEventHandle {
  pub fn try_recv_latest(&self) -> Result<Option<ViewModel>, TryRecvError> {
    let mut latest = None;
    loop {
      match self.events_rx.try_recv() {
        Ok(Event::ModelUpdated(model)) => {
          latest = Some(model);
        },
        Err(TryRecvError::Empty) => return Ok(latest),
        Err(e) => return Err(e),
      }
    }
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
  commands_rx: Receiver<Command>,
  events_tx: Sender<Event>,
  last_view_model: Option<ViewModel>,
  state: AppState,
}

impl <W: Write + Send> EventHandler<W> {
  pub fn run_loop(mut self) -> anyhow::Result<()> {
    loop {
      let command = self.commands_rx.recv()?;

      let result = match command {
        Command::ReceivedMessage(m) => self.handle_message(m),
        Command::ReadError(e) => Err(FatalError(e.to_string())),
        Command::ButtonPressed(b) => Ok(self.handle_button(b)),
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
    let mt = MessageType::try_from(&message)
        .map_err(|e| HandlingError::UnexpectedPayload(e.to_string()))?;

    let state_snapshot = self.state.fast_snapshot();
    self.state.cts_state_machine.handle_message(&mut self.framed_writer, &message.channel, &mt)?;
    if self.state.cts_state_machine.take_current_message_for_us() {
      self.state.topside_state_machine.handle_message(&mut self.framed_writer, &message.channel, &mt)?;
    }

    if self.state.fast_snapshot() != state_snapshot {
      let model = self.state.generate_view_model();
      if self.last_view_model.as_ref() != Some(&model) {
        info!("Emitting new model: {model:?}");
        self.last_view_model = Some(model.clone());
        let _ = self.events_tx.send(Event::ModelUpdated(model));
      }
    }

    Ok(())
  }

  fn handle_button(&mut self, button: Button) {
    match button {
      _ => warn!("handle_button({button:?}): not implemented!"),
    }
  }
}

#[derive(Debug)]
pub enum Command {
  ReceivedMessage(Message),
  ReadError(anyhow::Error),
  ButtonPressed(Button),
  Shutdown,
}

#[derive(Debug, Clone)]
pub enum Button {
  Up,
  Down,
  Jets1,
  Light,
}

pub enum Event {
  ModelUpdated(ViewModel),
}

impl From<MessageHandlingError> for HandlingError {
  fn from(value: MessageHandlingError) -> Self {
    match value {
      MessageHandlingError::FatalError(m) => FatalError(m),
    }
  }
}

impl From<PayloadEncodeError> for HandlingError {
  fn from(value: PayloadEncodeError) -> Self {
    match value {
      PayloadEncodeError::GenericError(e) => FatalError(format!("{e:?})")),
      PayloadEncodeError::GenericIoError(e) => FatalError(format!("{e:?}")),
      PayloadEncodeError::NotSupported => FatalError("Not supported".to_owned()),
    }
  }
}
