use std::io::{Read, Write};
use anyhow::anyhow;
use balboa_spa_messages::channel::Channel;
use balboa_spa_messages::framed_reader::FramedReader;
use balboa_spa_messages::framed_writer::FramedWriter;
use balboa_spa_messages::message_types::{MessageType, SettingsRequestMessage};
use esp_idf_hal::peripherals::Peripherals;
use log::{debug, error, info, warn};
use mock_mainboard_lib::transport::Transport;
use esp_app::esp_uart_transport::EspUartTransport;

fn main() -> anyhow::Result<()> {
  esp_idf_sys::link_patches();

  esp_idf_svc::log::EspLogger::initialize_default();

  let peripherals = Peripherals::take()
      .ok_or_else(|| anyhow!("Unable to take peripherals"))?;

  let transport = EspUartTransport::new(
      peripherals.uart2,
      peripherals.pins.gpio14,
      peripherals.pins.gpio27,
      Some(peripherals.pins.gpio13))?;

  // let (mut rx, _tx) = transport.split();
  //
  // let mut buf = [0u8; 14];
  // loop {
  //   let n = rx.read(&mut buf)?;
  //   info!("Got {:02X?}", &buf[0..n]);
  // }

  let panel = TopsidePanel::new(transport);
  panel.run_loop()?;
  Ok(())
}

struct TopsidePanel<R, W> {
  reader: FramedReader<R>,
  writer: FramedWriter<W>,
}

impl<R: Read, W: Write> TopsidePanel<R, W> {
  pub fn new(transport: impl Transport<R, W>) -> Self {
    let (raw_reader, raw_writer) = transport.split();
    let reader = FramedReader::new(raw_reader);
    let writer = FramedWriter::new(raw_writer);
    Self { reader, writer }
  }

  pub fn run_read_test(mut self) -> anyhow::Result<()> {
    loop {
      let message = self.reader.next_message()?;
      info!("Got {message:?}");
    }
  }

  pub fn run_loop(mut self) -> anyhow::Result<()> {
    loop {
      let message = self.reader.next_message()?;
      info!("Got {message:?}");

      let mut state = GetVersionTestState::NeedChannelWaitingCTS;
      let mut my_channel = None;
      let mut system_model_number = None;

      match MessageType::try_from(&message) {
        Ok(mt) => {
          match (message.channel, mt) {
            (Channel::MulticastChannelAssignment, MessageType::NewClientClearToSend()) => {
              if state == GetVersionTestState::NeedChannelWaitingCTS {
                self.writer.write(
                  &MessageType::ChannelAssignmentRequest {
                    device_type: 0x0,
                    client_hash: 0xcafe,
                  }.to_message(Channel::MulticastChannelAssignment)?)?;
                state = GetVersionTestState::NeedChannelWaitingAssignment;
              }
            }
            (Channel::MulticastChannelAssignment, MessageType::ChannelAssignmentResponse { channel, .. }) => {
              if state == GetVersionTestState::NeedChannelWaitingAssignment {
                my_channel = Some(channel);
                self.writer.write(&MessageType::ChannelAssignmentAck().to_message(channel)?)?;
                state = GetVersionTestState::NeedInfoWaitingCTS;
              }
            }
            (channel, MessageType::ClearToSend()) => {
              if my_channel == Some(channel) {
                match state {
                  GetVersionTestState::NeedInfoWaitingCTS => {
                    self.writer.write(
                      &MessageType::SettingsRequest(SettingsRequestMessage::Information)
                          .to_message(channel)?)?;
                    state = GetVersionTestState::NeedInfoWaitingInfo;
                  }
                  _ => {
                    self.writer.write(&MessageType::NothingToSend().to_message(channel)?)?;
                  }
                }
              }
            }
            (channel, MessageType::InformationResponse(info)) => {
              if state == GetVersionTestState::NeedInfoWaitingInfo &&
                  my_channel == Some(channel) {
                info!("Got system model number: {}", info.system_model_number);
                system_model_number = Some(info.system_model_number);
              }
            }
            (channel, MessageType::StatusUpdate(status)) => {
              debug!("system_model_number={system_model_number:?}");
            }
            _ => warn!("Unhandled: {message:?}"),
          }
        }
        Err(e) => error!("{e:?}"),
      }
    }
  }
}

struct StateMachine {
  state: GetVersionTestState,
}

impl StateMachine {
  pub fn move_to_state(&mut self, new_state: GetVersionTestState) {
    let old_state = &self.state;
    if old_state != &new_state {
      debug!("Moving from {old_state:?} to {new_state:?}");
      self.state = new_state;
    }
  }
}

#[derive(Debug, PartialEq, Clone)]
enum GetVersionTestState {
  NeedChannelWaitingCTS,
  NeedChannelWaitingAssignment,
  NeedInfoWaitingCTS,
  NeedInfoWaitingInfo,
}

