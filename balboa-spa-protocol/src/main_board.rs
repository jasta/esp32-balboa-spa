use std::collections::hash_map::{Entry, OccupiedError};
use std::collections::HashMap;
use std::error::Error;
use std::io::{Read, Write};
use std::sync::{Arc, mpsc, Mutex};
use std::sync::mpsc::{channel, Receiver, SendError, SyncSender};
use std::thread;
use std::time::{Duration, Instant};
use anyhow::anyhow;
use log::{error, info, warn};
use timer::{Guard, Timer};
use balboa_spa_messages::channel::Channel;
use balboa_spa_messages::framing::{FramedReader, FramedWriter};
use balboa_spa_messages::message::Message;
use balboa_spa_messages::message_types::{MessageType, PayloadParseError};
use crate::timer::{PeriodicTimer, SimpleTimerService};
use crate::transport::Transport;

const DEFAULT_INIT_DELAY: Duration = Duration::from_millis(5000);
const CLEAR_TO_SEND_WINDOW: Duration = Duration::from_millis(16);

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

  pub fn run_loop2(mut self) {
    let (tx, rx) = mpsc::sync_channel(32);
    let message_reader = MessageReader {
      message_tx: tx.clone(),
      framed_reader: self.framed_reader,
      raw_reader: self.raw_reader,
    };
    let timer_setup = TimerSetup {
      timer_tx: tx.clone(),
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
          self.message_tx.send(Event::ReceivedMessage(MessageBundle::from_now(message)))?
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
      (Duration::from_millis(1000), Event::TimerTick(TimerId::Update1Hz)),
      (Duration::from_millis(303), Event::TimerTick(TimerId::Update3_3Hz)),
      (Duration::from_millis(100), Event::TimerTick(TimerId::Update10Hz)),
      (Duration::from_millis(16), Event::TimerTick(TimerId::Update60Hz)),
      (self.init_delay, Event::InitFinished),
    ];

    let timer = Timer::with_capacity(4);
    let mut timer_hold = TimerHold { timer, guards: vec![] };
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
  init_finished: bool,
  channels: HashMap<DeviceRecord, Channel>
}

#[derive(Debug, Hash, Eq, Clone)]
struct DeviceRecord {
  device_type: u8,
  client_hash: u16,
}

impl<W: Write + Send> EventHandler<W> {
  pub fn run_loop(mut self) -> anyhow::Result<()> {
    for event in self.event_rx {
      match event {
        Event::ReceivedMessage(bundle) => self.handle_message(bundle),
        Event::ReadError(e) => return Err(e),
        Event::TimerTick(timer_id) => self.handle_timer(timer_id),
        Event::InitFinished => self.state.init_finished = true,
      }
    }

    Ok(())
  }

  fn handle_message(&mut self, bundle: MessageBundle) -> Result<(), HandlingError> {
    let time_since_recv = bundle.time.elapsed();
    if time_since_recv > CLEAR_TO_SEND_WINDOW {
      warn!("Received message later than CTS window, ignoring: {:?}", bundle.message);
    }
    let message = &bundle.message;
    match MessageType::try_from(message) {
      Ok(parsed) => {
        let reply = match parsed {
          MessageType::ChannelAssignmentRequest { device_type, client_hash } => {
            let record = DeviceRecord { device_type, client_hash };
            let selected_channel = *match self.state.channels.entry(record) {
              Entry::Occupied(o) => o.get()
              Entry::Vacant(v) => {
                let channel = Channel::new_client_channel(self.state.channels.len())
                    .map_err(HandlingError::ClientFatal)?;
                v.insert(channel)
              }
            };
            MessageType::ChannelAssignmentResponse {
              channel: selected_channel,
              client_hash,
            }.to_message(selected_channel)
          }
          MessageType::ChannelAssignmentAck() => {}
          MessageType::ExistingClientRequest() => {}
          MessageType::ExistingClientResponse { .. } => {}
          MessageType::ClearToSend() => {}
          MessageType::NothingToSend() => {}
          MessageType::ToggleItemRequest { .. } => {}
          MessageType::StatusUpdate { .. } => {}
          MessageType::SetTemperatureRequest { .. } => {}
          MessageType::SetTimeRequest { .. } => {}
          MessageType::SettingsRequest(_) => {}
          MessageType::FilterCycles { .. } => {}
          MessageType::InformationResponse { .. } => {}
          MessageType::PreferencesResponse { .. } => {}
          MessageType::SetPreferenceRequest(_) => {}
          MessageType::FaultLogResponse { .. } => {}
          MessageType::ChangeSetupRequest { .. } => {}
          MessageType::GfciTestResponse { .. } => {}
          MessageType::LockRequest(_) => {}
          MessageType::ConfigurationResponse { .. } => {}
          MessageType::WifiModuleConfigurationResponse { .. } => {}
          MessageType::ToggleTestSettingRequest(_) => {}
          MessageType::UnknownError1 |
            MessageType::UnknownError2 => {

          }
          _ => {
            info!("Received unsupported message: {message}");
          }
        };
        Ok(())
      }
      Err(e) => Err(HandlingError::ClientFatal),
    }
  }

  fn handle_timer(&self, timer_id: TimerId) {
    match timer_id {
      TimerId::Update1Hz => {
        todo!()
      }
      TimerId::Update3_3Hz => {
        todo!()
      }
      TimerId::Update10Hz => {
        todo!()
      }
      TimerId::Update60Hz => {
        todo!()
      }
    }
  }
}

#[derive(thiserror::Error, Debug)]
enum HandlingError {
  #[error("Main board fatal error, must halt")]
  FatalError,

  #[error("Client-specific fatal error, peer may never be able to recover")]
  ClientFatal,

  #[error("Client-specific non-fatal error, peer likely can recover by retrying")]
  ClientRecoverable,
}

enum Event {
  ReceivedMessage(MessageBundle),
  ReadError(anyhow::Error),
  InitFinished,
  TimerTick(TimerId),
}

struct MessageBundle {
  time: Instant,
  message: Message,
}

impl MessageBundle {
  #[inline]
  pub fn from_now(message: Message) -> Self {
    Self {
      time: Instant::now(),
      message,
    }
  }
}

enum TimerId {
  Update1Hz,
  Update3_3Hz,
  Update10Hz,
  Update60Hz,
}
