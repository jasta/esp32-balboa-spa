//! Mock main board handler used to integration test top panel / Wi-Fi module production code
//! and validate the overall correctness of implementations.

use std::{mem, thread};
use std::borrow::{Borrow, BorrowMut};
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
use balboa_spa_messages::message_types::{HeaterType, HeaterVoltage, InformationResponseMessage, MessageType, PayloadEncodeError, Settings0x04ResponseMessage, SettingsRequestMessage, SoftwareVersion};
use balboa_spa_messages::parsed_enum::ParsedEnum;

use crate::channel_tracker::{ChannelTracker, CtsFailureAction, DeviceKey};
use crate::channel_manager::{ChannelManager, CtsEnforcementPolicy};
use crate::clear_to_send_tracker::{ClearToSendTracker, NoCtsReason, SendMessage, SendMessageFactory, TrySendMessageError};
use common_lib::message_logger::{MessageDirection, MessageLogger};
use crate::mock_spa::{MockSpa, MockSpaState};
use crate::timer_tracker::{TickAction, TimerTracker};
use common_lib::transport::Transport;

pub struct MainBoard<R, W> {
  framed_reader: FramedReader<R>,
  framed_writer: FramedWriter<W>,
  init_delay: Option<Duration>,
  channel_manager: Option<ChannelManager>,
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
      channel_manager: None,
    }
  }

  pub fn set_init_delay(mut self, init_delay: Duration) -> Self {
    self.init_delay = Some(init_delay);
    self
  }

  pub fn set_clear_to_send_policy(mut self, cts_policy: CtsEnforcementPolicy, cts_window: Duration) -> Self {
    self.channel_manager = Some(ChannelManager::with_policy(cts_policy, cts_window));
    self
  }

  pub fn into_runner(self) -> (ControlHandle, Runner<R, W>) {
    let (tx, rx) = mpsc::sync_channel(32);
    let state = MainBoardState {
      channel_manager: self.channel_manager.unwrap_or_default(),
      ..Default::default()
    };
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
      message_logger: MessageLogger::new(module_path!()),
      state,
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
          self.message_tx.send(Event::ReceivedMessage(message))?;
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
      let init_tx = self.timer_tx;
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
}

#[derive(Default)]
struct MainBoardState {
  mock_spa: MockSpa,
  channel_manager: ChannelManager,
  timer_tracker: TimerTracker,
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
      Event::ReceivedMessage(message) => {
        self.message_logger.log(MessageDirection::Inbound, message);
      }
      Event::ReadError(_) => error!("{event:?}"),
      Event::InitFinished => info!("{event:?}"),
      Event::TimerTick(_) => trace!("{event:?}"),
      Event::Shutdown => debug!("{event:?}"),
    }
  }

  fn handle_event(&mut self, event: Event) -> Result<(), HandlingError> {
    match event {
      Event::ReceivedMessage(message) => self.handle_message(message)?,
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

  fn handle_message(&mut self, message: Message) -> Result<(), HandlingError> {
    self.channel_manager_mut().validate_message(&message)?;
    match MessageType::try_from(&message) {
      Ok(parsed) => {
        match self.channel_manager_mut().start_send_message()? {
          None => {
            Err(HandlingError::ClientNeedsReconnect(
                format!("Can't send reply on {:?} due to CTS errors!", message.channel)))
          }
          Some(smf) => {
            match self.handle_and_generate_response(message.channel, smf, parsed) {
              Ok(Some(reply)) => self.send_message(reply),
              Ok(None) => Ok(()),
              Err(e) => Err(e),
            }
          }
        }
      }
      Err(e) => Err(HandlingError::ClientUnsupported(format!("Payload parse error: {e:?}"))),
    }
  }

  fn handle_and_generate_response(
      &mut self,
      src_channel: Channel,
      smf: SendMessageFactory,
      parsed: MessageType
  ) -> Result<Option<SendMessage>, HandlingError> {
    let reply = match parsed {
      MessageType::ChannelAssignmentRequest { device_type, client_hash } => {
        let key = DeviceKey { device_type, client_hash };
        let selected_channel = self.channel_manager_mut().select_channel(key)?;
        info!("Assigned {key:?} to {selected_channel:?}");
        Some(smf.expect_reply_on_channel(MessageType::ChannelAssignmentResponse {
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
            Some(smf.no_reply(MessageType::InformationResponse(InformationResponseMessage {
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
            Some(smf.no_reply(MessageType::ConfigurationResponse(
              self.state.mock_spa.as_configuration()
            ).to_message(src_channel)?))
          }
          SettingsRequestMessage::FaultLog { entry_num } => {
            Some(smf.no_reply(MessageType::FaultLogResponse(
              self.state.mock_spa.as_fault_log(entry_num)
            ).to_message(src_channel)?))
          }
          SettingsRequestMessage::Settings0x04 => {
            // No clue...
            let unknown = vec![0x02, 0x02, 0x32, 0x63, 0x50, 0x68, 0x20, 0x07, 0x01];
            Some(smf.no_reply(MessageType::Settings0x04Response(Settings0x04ResponseMessage {
              unknown,
            }).to_message(src_channel)?))
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

  fn handle_timer(&mut self, timer_id: TimerId) -> Result<(), HandlingError> {
    match timer_id {
      TimerId::SendTickMessage => {
        if let Some(smf) = self.channel_manager_mut().start_send_message()? {
          let tick_action = self.state.timer_tracker.next_action();
          let message = match tick_action {
            TickAction::NewClientClearToSend => {
              Some(smf.maybe_expect_reply(
                MessageType::NewClientClearToSend()
                    .to_message(Channel::MulticastChannelAssignment)?))
            },
            TickAction::StatupUpdate => {
              Some(smf.no_reply(
                MessageType::StatusUpdate(self.state.mock_spa.as_status())
                    .to_message(Channel::MulticastBroadcast)?))
            },
            TickAction::ClearToSend { index } => {
              let num_channels = self.channel_manager().num_channels();
              if num_channels == 0 {
                None
              } else {
                let client_index = index % num_channels;
                let target = Channel::new_client_channel(client_index)
                    .map_err(|_| {
                      HandlingError::FatalError("Inconsistent channel overflow behaviour!".to_owned())
                    })?;
                Some(smf.expect_reply(MessageType::ClearToSend().to_message(target)?))
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

  fn send_message(&mut self, send: SendMessage) -> Result<(), HandlingError> {
    self.message_logger.log(MessageDirection::Outbound, &send.message);

    self.channel_manager_mut().handle_presend(&send);

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

  fn channel_manager(&self) -> &ChannelManager {
    self.state.channel_manager.borrow()
  }

  fn channel_manager_mut(&mut self) -> &mut ChannelManager {
    self.state.channel_manager.borrow_mut()
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
  ReceivedMessage(Message),
  ReadError(anyhow::Error),
  InitFinished,
  TimerTick(TimerId),
  Shutdown,
}

#[derive(Debug)]
enum TimerId {
  SendTickMessage,
}