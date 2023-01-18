//! Mock main board handler used to integration test top panel / Wi-Fi module production code
//! and validate the overall correctness of implementations.

use std::{mem, thread};
use std::io::{Read, Write};
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, SendError, SyncSender};
use std::time::{Duration, Instant};

use anyhow::anyhow;
use log::{debug, error, info, trace, warn};
use timer::{Guard, Timer};

use balboa_spa_messages::channel::Channel;
use balboa_spa_messages::framed_reader::FramedReader;
use balboa_spa_messages::framed_writer::FramedWriter;
use balboa_spa_messages::message::{EncodeError, Message};
use balboa_spa_messages::message_types::{HeaterType, HeaterVoltage, InformationResponseMessage, MessageType, PayloadEncodeError, SettingsRequestMessage, SoftwareVersion};
use balboa_spa_messages::parsed_enum::ParsedEnum;

use crate::channel_tracker::{ChannelTracker, DeviceKey};
use crate::message_logger::{MessageDirection, MessageLogger};
use crate::mock_spa::{MockSpa, MockSpaState};
use crate::timer_tracker::{TickAction, TimerTracker};
use crate::transport::Transport;

/// Amount of time to wait when we issue NewClientClearToSend or ClearToSend for a reply
/// before we can resume sending messages.
const DEFAULT_CLEAR_TO_SEND_WINDOW: Duration = Duration::from_millis(30);

pub struct MainBoard<R, W> {
  framed_reader: FramedReader<R>,
  framed_writer: FramedWriter<W>,
  init_delay: Option<Duration>,
  clear_to_send_window: Duration,
}

impl<R, W> MainBoard<R, W>
where
    R: Read + Send,
    W: Write + Send,
{
  pub fn new(transport: impl Transport<R, W>) -> Self {
    let (raw_reader, raw_writer) = transport.split();
    let framed_reader = FramedReader::new(raw_reader);
    let framed_writer = FramedWriter::new(raw_writer);
    Self {
      framed_reader,
      framed_writer,
      init_delay: None,
      clear_to_send_window: DEFAULT_CLEAR_TO_SEND_WINDOW,
    }
  }

  pub fn set_init_delay(mut self, init_delay: Duration) -> Self {
    self.init_delay = Some(init_delay);
    self
  }

  pub fn set_clear_to_send_window(mut self, window: Duration) -> Self {
    self.clear_to_send_window = window;
    self
  }

  pub fn into_runner(self) -> (ControlHandle, Runner<R, W>) {
    let (tx, rx) = mpsc::sync_channel(32);
    let state = MainBoardState::default();
    let message_reader = MessageReader {
      message_tx: tx.clone(),
      framed_reader: self.framed_reader,
    };
    let timer_setup = TimerSetup {
      timer_tx: tx.clone(),
      init_delay: self.init_delay,
      main_tick_hz: state.timer_tracker.total_ticks_per_cycle(),
    };
    let event_handler = EventHandler {
      event_rx: rx,
      framed_writer: self.framed_writer,
      message_logger: MessageLogger::default(),
      state,
      clear_to_send_window: self.clear_to_send_window,
    };

    let shutdown_handle = ControlHandle { tx };
    let runner = Runner { message_reader, timer_setup, event_handler };
    (shutdown_handle, runner)
  }
}

pub struct ControlHandle {
  tx: SyncSender<Event>,
}

impl ControlHandle {
  pub fn complete_init(&self) {
    let _ = self.tx.send(Event::InitFinished);
  }

  pub fn request_shutdown(&self) {
    let _ = self.tx.send(Event::Shutdown);
  }
}

impl Drop for ControlHandle {
  fn drop(&mut self) {
    self.request_shutdown();
  }
}

pub struct Runner<R, W> {
  message_reader: MessageReader<R>,
  timer_setup: TimerSetup,
  event_handler: EventHandler<W>,
}

impl<R: Read + Send + 'static, W: Write + Send + 'static> Runner<R, W> {
  pub fn run_loop(self) -> anyhow::Result<()> {
    let timer_hold = self.timer_setup.setup()?;

    // Order of the handles matters as this determines which loop will be prioritized to yield
    // the error from the main run_loop function.  EventHandler is strongly preferred as it has
    // more interesting handling logic and errors.
    let handles = [
      thread::Builder::new()
          .name("EventHandler".into())
          .spawn(move || {
            debug!("EventHandler starting up...");
            self.event_handler.run_loop()
          })
          .unwrap(),
      thread::Builder::new()
          .name("MessageReader".into())
          .spawn(move || {
            debug!("MessageReader starting up...");
            if let Err(e) = self.message_reader.run_loop() {
              // Don't forward these errors to the caller, the event handler will have already
              // converted it into something coherent.
              warn!("Message reader yielded: {e}");
            }
            Ok(())
          })
          .unwrap(),
    ];

    debug!("MainBoard run loop active...");
    let results: Vec<_> = handles.into_iter()
        .map(|h| h.join())
        .collect();

    drop(timer_hold);

    for result in results {
      result.unwrap()?;
    }

    Ok(())
  }
}

struct MessageReader<R> {
  framed_reader: FramedReader<R>,
  message_tx: SyncSender<Event>,
}

impl<R: Read + Send> MessageReader<R> {
  pub fn run_loop(mut self) -> Result<(), SendError<Event>> {
    loop {
      match self.framed_reader.next_message() {
        Ok(message) => {
          self.message_tx.send(Event::ReceivedMessage(TimeSensitiveMessage::from_now(message)))?;
        }
        Err(e) => {
          self.message_tx.send(Event::ReadError(anyhow!("{:?}", e)))?;
          break;
        }
      }
    }
    Ok(())
  }
}

struct TimerSetup {
  timer_tx: SyncSender<Event>,
  init_delay: Option<Duration>,
  main_tick_hz: usize,
}

impl TimerSetup {
  pub fn setup(self) -> anyhow::Result<TimerHold> {
    let timer = Timer::new();
    let mut guards = Vec::new();

    let main_tick_tx = self.timer_tx.clone();
    let main_tick_hz = u64::try_from(self.main_tick_hz)?;
    let main_tick_duration = Duration::from_millis(1000 / main_tick_hz);
    info!("Scheduling main timer @ {main_tick_hz} Hz...");
    let guard = timer.schedule_repeating(
        chrono::Duration::from_std(main_tick_duration)?, move || {
      let _ = main_tick_tx.send(Event::TimerTick(TimerId::SendTickMessage));
    });
    guards.push(guard);

    if let Some(init_delay) = self.init_delay {
      let init_tx = self.timer_tx.clone();
      let guard = timer.schedule_with_delay(
          chrono::Duration::from_std(init_delay)?, move || {
        let _ = init_tx.send(Event::InitFinished);
      });
      guards.push(guard);
    }

    Ok(TimerHold { _timer: timer, _guards: guards })
  }
}

struct TimerHold {
  _guards: Vec<Guard>,
  _timer: Timer,
}

struct EventHandler<W> {
  framed_writer: FramedWriter<W>,
  event_rx: Receiver<Event>,
  message_logger: MessageLogger,
  state: MainBoardState,
  clear_to_send_window: Duration,
}

#[derive(Default)]
struct MainBoardState {
  mock_spa: MockSpa,
  channel_tracker: ChannelTracker,
  authorized_sender: Option<AuthorizedSender>,
  timer_tracker: TimerTracker,
}

#[derive(Debug)]
struct AuthorizedSender {
  authorized_at: Instant,
  channel: Channel,
}

impl<W: Write + Send> EventHandler<W> {
  pub fn run_loop(mut self) -> anyhow::Result<()> {
    loop {
      let event = self.event_rx.recv()?;

      self.log_event(&event);

      if let Err(e) = self.handle_event(event) {
        match e {
          HandlingError::ShutdownRequested => {
            info!("Graceful shutdown requested...");
            break
          },
          HandlingError::FatalError(e) => {
            error!("Fatal error: {e}");
            return Err(anyhow!("Fatal error: {e}"));
          }
          _ => error!("Got {e:?}"),
        }
      }
    }

    Ok(())
  }

  /// Log a received event, deciding which log level to use based on verbosity in practice in
  /// the protocol.
  fn log_event(&self, event: &Event) {
    match event {
      Event::ReceivedMessage(bundle) => {
        self.message_logger.log(MessageDirection::Inbound, &bundle.message);
      }
      Event::ReadError(_) => error!("{event:?}"),
      Event::InitFinished => info!("{event:?}"),
      Event::TimerTick(_) => trace!("{event:?}"),
      Event::Shutdown => debug!("{event:?}"),
    }
  }

  fn handle_event(&mut self, event: Event) -> Result<(), HandlingError> {
    match event {
      Event::ReceivedMessage(bundle) => self.handle_message(bundle)?,
      Event::ReadError(e) => {
        return Err(HandlingError::FatalError(format!("Read error: {e:?}")))
      }
      Event::TimerTick(timer_id) => self.handle_timer(timer_id)?,
      Event::InitFinished => {
        self.state.mock_spa.run_state = MockSpaState::Heating;
      },
      Event::Shutdown => return Err(HandlingError::ShutdownRequested),
    }
    Ok(())
  }

  fn handle_message(&mut self, bundle: TimeSensitiveMessage) -> Result<(), HandlingError> {
    self.validate_message(&bundle)?;
    let message = &bundle.message;
    match MessageType::try_from(message) {
      Ok(parsed) => {
        if let Some(reply) = self.handle_and_generate_response(message.channel, parsed)? {
          self.send_message(reply)?;
        }
        Ok(())
      }
      Err(e) => Err(HandlingError::ClientUnsupported(format!("Payload parse error: {e:?}"))),
    }
  }

  fn handle_and_generate_response(&mut self, src_channel: Channel, parsed: MessageType) -> Result<Option<SendMessage>, HandlingError> {
    let reply = match parsed {
      MessageType::ChannelAssignmentRequest { device_type, client_hash } => {
        let key = DeviceKey { device_type, client_hash };
        let selected_channel = self.state.channel_tracker.select_channel(key.clone())?;
        info!("Assigned {key:?} to {selected_channel:?}");
        Some(SendMessage::expect_reply_on_channel(MessageType::ChannelAssignmentResponse {
          channel: selected_channel,
          client_hash,
        }.to_message(Channel::MulticastChannelAssignment)?, selected_channel))
      }
      MessageType::ChannelAssignmentAck() => {
        // Do nothing, we assume success with the potential side effect of accidentally
        // running out of slots if we get too many missed channel assignment messages.
        info!("Got channel assignment ack on channel={src_channel:?}");
        None
      }
      MessageType::NothingToSend() => {
        // Do nothing, general handling already removed the authorized sender state.
        None
      }
      MessageType::ToggleItemRequest { item_code, dummy1 } => {
        info!("Got request to toggle {item_code:?}, dummy1={dummy1}");
        None
      }
      MessageType::SetTemperatureRequest { temperature } => {
        info!("Got set temp request: temperature={temperature:?}");
        None
      }
      MessageType::SetTimeRequest { time } => {
        info!("Got set time request: time={time:?}");
        None
      }
      MessageType::SettingsRequest(settings) => {
        info!("Got settings request: message={settings:?}");
        match settings {
          SettingsRequestMessage::Information => {
            Some(SendMessage::no_reply(MessageType::InformationResponse(InformationResponseMessage {
              software_version: SoftwareVersion { version: [100, 210, 6, 0] },
              system_model_number: "Mock Spa".to_owned(),
              current_configuration_setup: 0,
              configuration_signature: [ 1, 2, 3, 4 ],
              heater_voltage: ParsedEnum::new(HeaterVoltage::V240),
              heater_type: ParsedEnum::new(HeaterType::Standard),
              dip_switch_settings: 0,
            }).to_message(src_channel)?))
          }
          SettingsRequestMessage::Configuration => {
            Some(SendMessage::no_reply(MessageType::ConfigurationResponse(
              self.state.mock_spa.as_configuration()
            ).to_message(src_channel)?))
          }
          SettingsRequestMessage::FaultLog { entry_num } => {
            Some(SendMessage::no_reply(MessageType::FaultLogResponse(
              self.state.mock_spa.as_fault_log(entry_num)
            ).to_message(src_channel)?))
          }
          n => {
            error!("Unhandled settings request: {n:?}");
            None
          }
        }
      }
      MessageType::FilterCycles { cycles } => {
        info!("Got filter cycles: cycles={cycles:?}");
        None
      }
      MessageType::SetPreferenceRequest(prefs) => {
        info!("Got set preference request: prefs={prefs:?}");
        None
      }
      MessageType::ChangeSetupRequest { setup_number } => {
        info!("Got change setup request: setup_number={setup_number}");
        None
      }
      MessageType::LockRequest(lock) => {
        info!("Got lock request: lock={lock:?}");
        None
      }
      MessageType::ToggleTestSettingRequest(test_setting) => {
        info!("Got toggle test setting request: test_setting={test_setting:?}");
        None
      }
      n => {
        return Err(HandlingError::ClientUnsupported(
          format!("Received unsupported message: {n:?}")));
      }
    };
    Ok(reply)
  }

  fn validate_message(&mut self, bundle: &TimeSensitiveMessage) -> Result<(), HandlingError> {
    // Note that this means a denial of service is trivially possible if an unauthorized
    // sender spams the signal line.  That's already going to break RS485 communication though,
    // so nothing we can do about it.
    let authorized_sender = mem::take(&mut self.state.authorized_sender);

    let channel = &bundle.message.channel;
    match channel {
      channel @ Channel::Client(_) => {
        if !self.state.channel_tracker.is_allocated(channel) {
          return Err(HandlingError::ClientNeedsReconnect(
            format!("Received message on unassigned channel={channel:?}, ignoring...")));
        }
      }
      Channel::MulticastChannelAssignment => {}
      _ => {
        return Err(HandlingError::ClientUnsupported(
          format!("Received message on unexpected channel={channel:?}, ignoring...")));
      }
    }

    match authorized_sender {
      Some(authorized_sender) => {
        if &authorized_sender.channel != channel {
          return Err(HandlingError::ClientNeedsReconnect(
            format!("Received message on non-CTS channel={channel:?}, ignoring...")));
        }
        let elapsed = authorized_sender.authorized_at.elapsed();
        if elapsed > self.clear_to_send_window {
          let action = self.state.channel_tracker.record_cts_failure(channel.to_owned());
          return Err(HandlingError::ClientNeedsReconnect(
            format!("Received message on channel={channel:?} after {}s, maximum allowed is {}s, {action:?}...",
              elapsed.as_secs(), self.clear_to_send_window.as_secs())));
        } else {
          self.state.channel_tracker.record_cts_success(channel.to_owned());
        }
      }
      None => {
        return Err(HandlingError::ClientNeedsReconnect(
          format!("Received message on channel={channel:?} without active authorized sender, ignoring...")
        ));
      }
    }

    Ok(())
  }

  fn handle_timer(&mut self, timer_id: TimerId) -> Result<(), HandlingError> {
    match timer_id {
      TimerId::SendTickMessage => {
        if self.can_send_now() {
          let tick_action = self.state.timer_tracker.next_action();
          let message = match tick_action {
            TickAction::NewClientClearToSend => {
              // Sort of odd to say we expect a reply but for our purposes this just means
              // that we won't perform any writes until we timeout hearing back from any
              // new clients.  This is important since if we try to write again right after
              // sending this we might clobber data and get stuck in a starvation loop.
              Some(SendMessage::expect_reply(
                MessageType::NewClientClearToSend()
                    .to_message(Channel::MulticastChannelAssignment)?))
            },
            TickAction::StatupUpdate => {
              Some(SendMessage::no_reply(
                MessageType::StatusUpdate(self.state.mock_spa.as_status())
                    .to_message(Channel::MulticastBroadcast)?))
            },
            TickAction::ClearToSend { index } => {
              let num_channels = self.state.channel_tracker.len();
              if num_channels == 0 {
                None
              } else {
                let client_index = index % num_channels;
                let target = Channel::new_client_channel(client_index)
                    .map_err(|_| {
                      HandlingError::FatalError("Inconsistent channel overflow behaviour!".to_owned())
                    })?;
                Some(SendMessage::expect_reply(MessageType::ClearToSend().to_message(target)?))
              }
            }
          };
          if let Some(message) = message {
            self.send_message(message)?;
          }
        }
      }
    }
    Ok(())
  }

  fn can_send_now(&mut self) -> bool {
    match &self.state.authorized_sender {
      Some(authorized) => {
        if authorized.authorized_at.elapsed() > self.clear_to_send_window {
          // Only log if we genuinely expected a reply...
          if authorized.channel != Channel::MulticastChannelAssignment {
            error!(
                "Authorized sender on channel={:?} timed out, clearing...",
                authorized.channel);
            self.state.channel_tracker.record_cts_failure(authorized.channel);
          }
          true
        } else {
          warn!("Skipping timer tick, active authorized sender on {:?}", authorized.channel);
          false
        }
      }
      None => true,
    }
  }

  fn send_message(&mut self, send: SendMessage) -> Result<(), HandlingError> {
    self.message_logger.log(MessageDirection::Outbound, &send.message);

    if let Some(authorized) = &self.state.authorized_sender {
      if authorized.channel != Channel::MulticastChannelAssignment {
        warn!("Existing authorized sender on channel={:?} dropped implicitly!", authorized.channel);
      }
    }

    let authorized_sender = send.expect_reply_on.map(|channel| AuthorizedSender {
      authorized_at: Instant::now(),
      channel,
    });
    self.state.authorized_sender = authorized_sender;

    // Note that this is a blocking write, meaning that we don't have to worry about
    // clear-to-send timing if it takes too long since our timer simply won't tick until we
    // finish!
    let err_mapper = |e| {
      HandlingError::FatalError(format!("Line write failure: {e:?}"))
    };

    self.framed_writer.write(&send.message)
        .map_err(err_mapper)?;

    Ok(())
  }
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum HandlingError {
  #[error("Main board fatal error, must halt: {0}")]
  FatalError(String),

  #[error("Client-specific connection error, try renegotiating: {0}")]
  ClientNeedsReconnect(String),

  #[error("Client-specific non-fatal error, peer likely can recover by retrying failed message: {0}")]
  ClientRecoverable(String),

  #[error("Client-specific fatal error, may never be able to fully communicate without software updates: {0}")]
  ClientUnsupported(String),

  #[error("Graceful shutdown requested")]
  ShutdownRequested,
}

impl From<PayloadEncodeError> for HandlingError {
  fn from(value: PayloadEncodeError) -> Self {
    match value {
      PayloadEncodeError::GenericError(e) =>
        HandlingError::ClientUnsupported(format!("{e:?})")),
      PayloadEncodeError::GenericIoError(e) =>
        HandlingError::ClientRecoverable(format!("{e:?}")),
      PayloadEncodeError::NotSupported =>
        HandlingError::ClientUnsupported("Not supported".to_owned()),
    }
  }
}

impl From<EncodeError> for HandlingError {
  fn from(value: EncodeError) -> Self {
    match value {
      EncodeError::MessageTooLong(size) =>
        HandlingError::ClientUnsupported(format!("Reply too long, size={size}")),
    }
  }
}

#[derive(Debug)]
enum Event {
  ReceivedMessage(TimeSensitiveMessage),
  ReadError(anyhow::Error),
  InitFinished,
  TimerTick(TimerId),
  Shutdown,
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct SendMessage {
  message: Message,
  expect_reply_on: Option<Channel>,
}

impl SendMessage {
  pub fn expect_reply(message: Message) -> Self {
    let channel = message.channel;
    Self { message, expect_reply_on: Some(channel) }
  }

  pub fn expect_reply_on_channel(message: Message, channel: Channel) -> Self {
    Self { message, expect_reply_on: Some(channel) }
  }

  pub fn no_reply(message: Message) -> Self {
    Self { message, expect_reply_on: None }
  }
}

#[derive(Debug)]
enum TimerId {
  SendTickMessage,
}
