use std::fmt::Debug;
use std::io::{Read, Write};

use anyhow::anyhow;
use balboa_spa_messages::channel::Channel;
use balboa_spa_messages::framed_reader::FramedReader;
use balboa_spa_messages::framed_writer::FramedWriter;
use balboa_spa_messages::message::Message;
use balboa_spa_messages::message_types::{MessageType, SettingsRequestMessage};
use esp_idf_hal::peripherals::Peripherals;
use log::{debug, error, info, warn};
use mock_mainboard_lib::transport::Transport;
use ws2812_esp32_rmt_driver::RGB8;
use esp_app::esp32c3_devkit_m;
use esp_app::esp_uart_transport::EspUartTransport;

use esp_app::esp_ws2812_driver::EspWs2812Driver;
use esp_app::status_led::{SmartLedsStatusLed, StatusLed};

fn main() -> anyhow::Result<()> {
  esp_idf_sys::link_patches();

  esp_idf_svc::log::EspLogger::initialize_default();

  let peripherals = Peripherals::take()
      .ok_or_else(|| anyhow!("Unable to take peripherals"))?;

  let onboard_led = esp32c3_devkit_m::onboard_led!(peripherals)?;
  let status_led = SmartLedsStatusLed::new(onboard_led.into_inner());

  let transport = EspUartTransport::new(
      peripherals.uart1,
      peripherals.pins.gpio5,
      peripherals.pins.gpio4,
      Some(peripherals.pins.gpio3),
      None)?;

  // let (mut rx, tx) = transport.split();
  // let mut buf = [0u8; 1];
  // loop {
  //   rx.read_exact(&mut buf)?;
  //   info!("Got {buf:02X?}");
  // }

  let panel = TopsidePanel::new(transport, status_led);
  panel.run_loop()?;
  Ok(())
}

struct TopsidePanel<R, W, L> {
  status_led: L,
  reader: FramedReader<R>,
  writer: FramedWriter<W>,
  state: GetVersionTestState,
  my_channel: Option<Channel>,
  model_number: Option<String>,
}

const MY_CLIENT_HASH: u16 = 0xcafe;

impl<R, W, L> TopsidePanel<R, W, L>
where
    R: Read,
    W: Write,
    L: StatusLed,
{
  pub fn new(transport: impl Transport<R, W>, status_led: L) -> Self {
    let (raw_reader, raw_writer) = transport.split();
    let reader = FramedReader::new(raw_reader);
    let writer = FramedWriter::new(raw_writer);
    Self {
      status_led,
      reader,
      writer,
      state: GetVersionTestState::NeedChannelWaitingCTS,
      my_channel: None,
      model_number: None,
    }
  }

  pub fn run_read_test(mut self) -> anyhow::Result<()> {
    loop {
      let message = self.reader.next_message()?;
      info!("Got {message:?}");
    }
  }

  pub fn run_loop(mut self) -> anyhow::Result<()> {
    self.maybe_update_status_led();
    loop {
      let message = self.reader.next_message()?;
      info!("<= {message:?}");

      match MessageType::try_from(&message) {
        Ok(mt) => {
          match (message.channel, mt) {
            (Channel::MulticastChannelAssignment, MessageType::NewClientClearToSend()) => {
              if matches!(self.state,
                  GetVersionTestState::NeedChannelWaitingCTS |
                  GetVersionTestState::NeedChannelWaitingAssignment) {
                self.send_message(
                  &MessageType::ChannelAssignmentRequest {
                    device_type: 0x0,
                    client_hash: MY_CLIENT_HASH,
                  }.to_message(Channel::MulticastChannelAssignment)?)?;
                self.move_to_state(GetVersionTestState::NeedChannelWaitingAssignment);
              }
            }
            (Channel::MulticastChannelAssignment, MessageType::ChannelAssignmentResponse { channel, client_hash }) => {
              if self.state == GetVersionTestState::NeedChannelWaitingAssignment &&
                  client_hash == MY_CLIENT_HASH {
                self.my_channel = Some(channel);
                self.send_message(&MessageType::ChannelAssignmentAck().to_message(channel)?)?;
                self.move_to_state(GetVersionTestState::NeedInfoWaitingCTS);
              }
            }
            (channel, MessageType::ClearToSend()) => {
              if self.my_channel == Some(channel) {
                match self.state {
                  GetVersionTestState::NeedInfoWaitingCTS => {
                    self.send_message(
                      &MessageType::SettingsRequest(SettingsRequestMessage::Information)
                          .to_message(channel)?)?;
                    self.move_to_state(GetVersionTestState::NeedInfoWaitingInfo);
                  }
                  _ => {
                    self.send_message(&MessageType::NothingToSend().to_message(channel)?)?;
                  }
                }
              }
            }
            (channel, MessageType::InformationResponse(info)) => {
              if self.state == GetVersionTestState::NeedInfoWaitingInfo &&
                  self.my_channel == Some(channel) {
                info!("Got system model number: {}", info.system_model_number);
                self.model_number = Some(info.system_model_number);
                self.move_to_state(GetVersionTestState::GotInfoIdle);
              }
            }
            (channel, MessageType::StatusUpdate(status)) => {
              debug!("system_model_number={:?}", self.model_number);
            }
            _ => warn!("Unhandled: {message:?}"),
          }
        }
        Err(e) => error!("{e:?}"),
      }
    }
  }

  fn send_message(&mut self, message: &Message) -> anyhow::Result<()> {
    info!("=> {message:?}");
    self.writer.write(message)
  }

  fn move_to_state(&mut self, new_state: GetVersionTestState) {
    let old_state = &self.state;
    if old_state != &new_state {
      debug!("Moving from {old_state:?} to {new_state:?}");
      self.state = new_state;
      self.maybe_update_status_led();
    }
  }

  fn maybe_update_status_led(&mut self) {
    if let Err(e) = self.update_status_led() {
      error!("Failed to update status LED: {e:?}");
    }
  }

  fn update_status_led(&mut self) -> Result<(), L::Error> {
    let color_hex: u32 = match self.state {
      GetVersionTestState::NeedChannelWaitingCTS => 0x010000,
      GetVersionTestState::NeedChannelWaitingAssignment => 0x000001,
      GetVersionTestState::NeedInfoWaitingCTS => 0x020100,
      GetVersionTestState::NeedInfoWaitingInfo => 0x020100,
      GetVersionTestState::GotInfoIdle => 0x000100,
    };
    let c = color_hex.to_be_bytes();
    self.status_led.set_color(RGB8::new(c[1], c[2], c[3]))
  }
}

#[derive(Debug, PartialEq, Clone)]
enum GetVersionTestState {
  NeedChannelWaitingCTS,
  NeedChannelWaitingAssignment,
  NeedInfoWaitingCTS,
  NeedInfoWaitingInfo,
  GotInfoIdle,
}