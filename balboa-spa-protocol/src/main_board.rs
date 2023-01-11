//! Mock main board handler used to integration test top panel / Wi-Fi module production code
//! and validate the overall correctness of implementations.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::error::Error;
use std::io::{Read, Write};
use std::sync::{Arc, mpsc, Mutex};
use std::sync::mpsc::{channel, Receiver, SendError, SyncSender};
use std::thread;
use std::time::{Duration, Instant};
use anyhow::anyhow;
use bimap::BiMap;
use log::{error, info, warn};
use timer::{Guard, Timer};
use balboa_spa_messages::channel::Channel;
use balboa_spa_messages::framing::{FramedReader, FramedWriter};
use balboa_spa_messages::message::{EncodeError, Message};
use balboa_spa_messages::message_types::{ItemCode, MessageType, PayloadEncodeError, PayloadParseError, SpaState, StatusUpdateResponseV1};
use balboa_spa_messages::message_types::SpaState::Running;
use balboa_spa_messages::parsed_enum::ParsedEnum;
use crate::mock_spa::{InternalSpaState, MockSpa, MockSpaState};
use crate::timer::{PeriodicTimer, SimpleTimerService};
use crate::transport::Transport;

const DEFAULT_INIT_DELAY: Duration = Duration::from_millis(5000);

/// Amount of time before removing a client that refuses to acknowledge ClearToSend messages.
const CLEAR_TO_SEND_WINDOW: Duration = Duration::from_millis(30);

pub struct MainBoard<TS, R, W> {
  timer_service: TS,
  framed_reader: FramedReader,
  framed_writer: FramedWriter,
  raw_reader: R,
  raw_writer: W,
  init_delay: Duration,
}

impl<TS, R, W> MainBoard<TS, R, W>
where
    TS: SimpleTimerService + Send + Sync,
    R: Read + Send + Sync,
    W: Write + Send + Sync,
{
  pub fn new(timer_service: TS, transport: impl Transport<R, W>) -> Self {
    let framed_reader = FramedReader::new();
    let framed_writer = FramedWriter::new();
    let (raw_reader, raw_writer) = transport.split();
    Self {
      timer_service,
      framed_reader,
      framed_writer,
      raw_reader,
      raw_writer,
      init_delay: DEFAULT_INIT_DELAY,
    }
  }

  pub fn set_init_delay(mut self, init_delay: Duration) -> Self {
    self.init_delay = init_delay;
    self
  }

  pub fn run_loop(mut self) {
    let (tx, rx) = mpsc::sync_channel(32);
    let message_reader = MessageReader {
      message_tx: tx.clone(),
      framed_reader: self.framed_reader,
      raw_reader: self.raw_reader,
    };
    let timer_setup = TimerSetup {
      timer_tx: tx.clone(),
      init_delay: self.init_delay,
    };
    let event_handler = EventHandler {
      event_rx: rx,
      framed_writer: self.framed_writer,
      raw_writer: self.raw_writer,
      state: MainBoardState::default(),
    };

    let timer_hold = timer_setup.setup();

    let handles = [
      thread::spawn(move || { let _ = message_reader.run_loop(); }),
      thread::spawn(move || event_handler.run_loop()),
    ];

    for handle in handles {
      handle.join().unwrap();
    }

    drop(timer_hold);
  }
}

struct MessageReader<R> {
  framed_reader: FramedReader,
  raw_reader: R,
  message_tx: SyncSender<Event>,
}

impl<R: Read + Send> MessageReader<R> {
  pub fn run_loop(mut self) -> Result<(), SendError<Event>> {
    let mut buf = [0u8; 256];
    loop {
      match self.raw_reader.read(buf.as_mut_slice()) {
        Ok(n) => self.handle_data(&buf[0..n])?,
        Err(e) => self.message_tx.send(Event::ReadError(anyhow!("{:?}", e)))?,
      }
    }
  }

  fn handle_data(&mut self, data: &[u8]) -> Result<(), SendError<Event>> {
    for b in data {
      match self.framed_reader.accept(*b) {
        None => {}
        Some(message) => {
          self.message_tx.send(Event::ReceivedMessage(TimeSensitiveMessage::from_now(message)))?
        },
      }
    }
    Ok(())
  }
}

struct TimerSetup {
  timer_tx: SyncSender<Event>,
  init_delay: Duration,
}

impl TimerSetup {
  pub fn setup(mut self) -> anyhow::Result<TimerHold> {
    let ticks = vec![
      (Duration::from_millis(1000 / 66), Event::TimerTick(TimerId::Update66Hz)),
      (self.init_delay, Event::InitFinished),
    ];

    let timer = Timer::with_capacity(ticks.len());
    let mut timer_hold = TimerHold { timer, guards: Vec::with_capacity(ticks.len()) };
    for (interval, event) in ticks {
      let timer_tx = self.timer_tx.clone();
      let converted_interval = chrono::Duration::try_from(interval)?;
      let guard = timer.schedule_repeating(converted_interval, move || {
        let _ = timer_tx.send(event);
      });
      timer_hold.guards.push(guard);
    }

    Ok(timer_hold)
  }
}

struct TimerHold {
  guards: Vec<Guard>,
  timer: Timer,
}

struct EventHandler<W> {
  framed_writer: FramedWriter,
  raw_writer: W,
  event_rx: Receiver<Event>,
  state: MainBoardState,
}

#[derive(Default)]
struct MainBoardState {
  mock_spa: MockSpa,
  channels: BiMap<DeviceRecord, Channel>,
  authorized_sender: Option<AuthorizedSender>,
  timer_tick: usize,
}

#[derive(Debug, Hash, Eq, Clone)]
struct DeviceRecord {
  device_type: u8,
  client_hash: u16,
}

#[derive(Debug)]
struct AuthorizedSender {
  authorized_at: Instant,
  channel: Channel,
}

impl<W: Write + Send> EventHandler<W> {
  pub fn run_loop(mut self) -> anyhow::Result<()> {
    for event in self.event_rx {
      match event {
        Event::ReceivedMessage(bundle) => self.handle_message(bundle)?,
        Event::ReadError(e) => return Err(e),
        Event::TimerTick(timer_id) => self.handle_timer(timer_id)?,
        Event::InitFinished => {
          self.state.mock_spa.run_state = MockSpaState::Heating;
        },
      }
    }

    Ok(())
  }

  fn handle_message(&mut self, bundle: TimeSensitiveMessage) -> Result<(), HandlingError> {
    self.validate_message(&bundle)?;
    let message = &bundle.message;
    match MessageType::try_from(message) {
      Ok(parsed) => {
        let reply = self.handle_and_generate_response(parsed)?;
        self.send_message(reply)?;
        Ok(())
      }
      Err(e) => Err(HandlingError::ClientUnsupported(format!("Payload parse error: {e:?}"))),
    }
  }

  fn handle_and_generate_response(&mut self, parsed: MessageType) -> Result<Message, HandlingError> {
    let reply = match parsed {
      MessageType::ChannelAssignmentRequest { device_type, client_hash } => {
        let record = DeviceRecord { device_type, client_hash };
        let mut channels = &self.state.channels;
        let selected_channel = *match channels.get_by_left(&record) {
          Some(entry) => entry,
          None => {
            let channel = Channel::new_client_channel(channels.len())
                .map_err(HandlingError::ClientNeedsReconnect)?;
            channels.insert(record, channel);
          }
        };
        MessageType::ChannelAssignmentResponse {
          channel: selected_channel,
          client_hash,
        }.to_message(Channel::MulticastChannelAssignment)?
      }
      MessageType::ChannelAssignmentAck() => {
        // Do nothing, we assume success with the potential side effect of accidentally
        // running out of slots if we get too many missed channel assignment messages.
      }
      MessageType::NothingToSend() => {
        // Do nothing, general handling already removed the authorized sender state.
      }
      MessageType::ToggleItemRequest { item_code, dummy1 } => {
        info!("Got request to toggle {item_code:?}, dummy1={dummy1}");
      }
      MessageType::SetTemperatureRequest { temperature } => {
        info!("Got set temp request: temperature={temperature:?}");
      }
      MessageType::SetTimeRequest { time } => {
        info!("Got set time request: time={time:?}");
      }
      MessageType::SettingsRequest(settings) => {
        info!("Got settings request: message={settings:?}");
      }
      MessageType::FilterCycles { cycles } => {
        info!("Got filter cycles: cycles={cycles:?}");
      }
      MessageType::SetPreferenceRequest(prefs) => {
        info!("Got set preference request: prefs={prefs:?}");
      }
      MessageType::ChangeSetupRequest { setup_number } => {
        info!("Got change setup request: setup_number={setup_number}");
      }
      MessageType::LockRequest(lock) => {
        info!("Got lock request: lock={lock:?}");
      }
      MessageType::ToggleTestSettingRequest(test_setting) => {
        info!("Got toggle test setting request: test_setting={test_setting:?}");
      }
      _ => {
        return Err(HandlingError::ClientUnsupported(
          format!("Received unsupported message: {message}")));
      }
    };
    Ok(reply)
  }

  fn validate_message(&self, bundle: &TimeSensitiveMessage) -> Result<(), HandlingError> {
    match bundle.message.channel {
      channel @ Channel::Client(channel) => {
        if self.state.channels.get_by_right(&channel).is_none() {
          return Err(HandlingError::ClientNeedsReconnect(
            format!("Received message on unassigned channel={channel:?}, ignoring...")));
        }
        match &self.state.authorized_sender {
          Some(authorized_sender) => {
            if authorized_sender.channel != channel {
              return Err(HandlingError::ClientNeedsReconnect(
                format!("Received message on non-CTS channel={channel:?}, ignoring...")));
            }
            let elapsed = authorized_sender.authorized_at.elapsed();
            if elapsed > CLEAR_TO_SEND_WINDOW {
              return Err(HandlingError::ClientNeedsReconnect(
                format!("Received message on channel={channel:?} after {}s, maximum allowed is {}s, dropping client...",
                    elapsed.as_secs(), CLEAR_TO_SEND_WINDOW.as_secs())));
            }
          }
          None => {
            return Err(HandlingError::ClientNeedsReconnect(
              format!("Received message when ")
            ))
          }
        }
      }
      Channel::MulticastChannelAssignment => {}
      channel => {
        return Err(HandlingError::ClientUnsupported(
          format!("Received message on unexpected channel={channel:?}, ignoring...")));
      }
    }
    Ok(())
  }

  fn handle_timer(&mut self, timer_id: TimerId) -> Result<(), HandlingError> {
    match timer_id {
      TimerId::Update66Hz => {
        if self.state.timer_tick >= 67 {
          self.state.timer_tick = 0;
        }
        self.state.timer_tick += 1;

        let message = match self.state.timer_tick {
          1 => MessageType::NewClientClearToSend().to_message(Channel::MulticastChannelAssignment)?,
          2 => MessageType::StatusUpdate(self.state.mock_spa.as_status()).to_message(Channel::MulticastBroadcast),
          tick => {
            let adjusted_tick = tick - 2;
            let client_index = adjusted_tick % self.state.channels.len();
            let target = Channel::new_client_channel(client_index)
                .map_err(|e| {
                  HandlingError::FatalError("Overflowed total channels!".to_owned())
                })?;
            MessageType::ClearToSend().to_message(target)
          }
        };
        self.send_message(message)?;
      }
    }
    Ok(())
  }

  fn send_message(&mut self, message: Message) -> Result<(), HandlingError> {
    let encoded_reply = self.framed_writer.encode(&reply)?;

    // Note that this is a blocking write, meaning that we don't have to worry about
    // clear-to-send timing if it takes too long since our timer simply won't tick until we
    // finish!
    self.raw_writer.write_all(&encoded_reply)
        .map_err(|e| HandlingError::FatalError(format!("Line write failure: {e:?}")))?;

    Ok(())
  }
}

#[derive(thiserror::Error, Debug)]
enum HandlingError {
  #[error("Main board fatal error, must halt: {}")]
  FatalError(String),

  #[error("Client-specific connection error, try renegotiating: {}")]
  ClientNeedsReconnect(String),

  #[error("Client-specific non-fatal error, peer likely can recover by retrying failed message: {}")]
  ClientRecoverable(String),

  #[error("Client-specific fatal error, may never be able to fully communicate without software updates: {}")]
  ClientUnsupported(String),
}

impl From<PayloadEncodeError> for HandlingError {
  fn from(value: PayloadEncodeError) -> Self {
    match value {
      PayloadEncodeError::GenericError(e) =>
        HandlingError::ClientUnsupported(format!("{e:?})")),
      PayloadEncodeError::GenericIoError(e) =>
        HandlingError::ClientRecoverable(format!("{e:?}")),
      PayloadEncodeError::NotSupported =>
        HandlingError::ClientUnsupported(format!("{e:?}")),
    }
  }
}

impl From<EncodeError> for HandlingError {
  fn from(value: EncodeError) -> Self {
    match value {
      EncodeError::MessageTooLong(size) =>
        HandlingError::ClientUnsupported(format!("Reply too long")),
    }
  }
}

enum Event {
  ReceivedMessage(TimeSensitiveMessage),
  ReadError(anyhow::Error),
  InitFinished,
  TimerTick(TimerId),
}

struct TimeSensitiveMessage {
  creation: Instant,
  message: Message,
}

impl TimeSensitiveMessage {
  #[inline]
  pub fn from_now(message: Message) -> Self {
    Self {
      creation: Instant::now(),
      message,
    }
  }
}

enum TimerId {
  Update66Hz,
}
