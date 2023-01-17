use std::io::BufRead;
use anyhow::anyhow;
use balboa_spa_messages::channel::Channel;
use balboa_spa_messages::framed_reader::FramedReader;
use balboa_spa_messages::framed_writer::FramedWriter;
use balboa_spa_messages::message_types::{MessageType, PayloadParseError, SettingsRequestMessage};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::{gpio, uart};
use esp_idf_hal::delay::BLOCK;
use esp_idf_hal::gpio::PinDriver;
use esp_idf_hal::uart::config::{DataBits, StopBits};
use esp_idf_hal::uart::UartDriver;
use esp_idf_hal::units::Hertz;
use log::{error, info, warn};
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

  let (rx, tx) = transport.split();
  let mut reader = FramedReader::new(rx)
      .set_debug(true);
  let mut writer = FramedWriter::new(tx);

  loop {
    let message = reader.next_message()?;
    info!("Got {message:?}");

    let mut state = GetVersionTestState::NeedChannelWaitingCTS;
    let mut my_channel = None;

    match MessageType::try_from(&message) {
      Ok(mt) => {
        match (message.channel, mt) {
          (Channel::MulticastChannelAssignment, MessageType::NewClientClearToSend()) => {
            assert_eq!(state, GetVersionTestState::NeedChannelWaitingCTS);
            writer.write(
              &MessageType::ChannelAssignmentRequest {
                device_type: 0x0,
                client_hash: 0xcafe,
              }.to_message(Channel::MulticastChannelAssignment)?)?;
            state = GetVersionTestState::NeedChannel_WaitingAssignment;
          }
          (Channel::MulticastChannelAssignment, MessageType::ChannelAssignmentResponse { channel, .. }) => {
            assert_eq!(state, GetVersionTestState::NeedChannel_WaitingAssignment);
            my_channel = Some(channel);
            writer.write(&MessageType::ChannelAssignmentAck().to_message(channel)?)?;
            state = GetVersionTestState::NeedInfo_WaitingCTS;
          }
          (channel, MessageType::ClearToSend()) => {
            if my_channel == Some(channel) {
              match state {
                GetVersionTestState::NeedInfo_WaitingCTS => {
                  writer.write(
                    &MessageType::SettingsRequest(SettingsRequestMessage::Information)
                        .to_message(channel)?)?;
                  state = GetVersionTestState::NeedInfo_WaitingInfo;
                }
                _ => {
                  writer.write(&MessageType::NothingToSend().to_message(channel)?)?;
                }
              }
            }
          }
          (channel, MessageType::InformationResponse(info)) => {
            assert_eq!(state, GetVersionTestState::NeedInfo_WaitingInfo);
            assert_eq!(Some(channel), my_channel);
            info!("Got system model number: {}", info.system_model_number);
          }
          (channel, MessageType::StatusUpdate(status)) => {
            // Ignore...
          }
          _ => warn!("Unhandled: {message:?}"),
        }
      }
      Err(e) => error!("{e:?}"),
    }
  }
}

#[derive(Debug, PartialEq, Clone)]
enum GetVersionTestState {
  NeedChannelWaitingCTS,
  NeedChannel_WaitingAssignment,
  NeedInfo_WaitingCTS,
  NeedInfo_WaitingInfo,
}

