/// Protocol definition as reverse engineered and documented here:
/// https://github.com/ccutrer/balboa_worldwide_app/wiki#physical-layer

use crc::{Algorithm, Crc};
use byteorder::ReadBytesExt;

pub struct Message {
  channel: Channel,
  unknown: u8,
  message_type: MessageType,
  payload: Vec<u8>,
}

pub enum Channel {
  Reserved,
  Client(u8),
  ClientNoCTS(u8),
  MulticastRequest,
  MulticastBroadcast, // <-- I think?
  Unknown(u8),
}

#[repr(u8)]
pub enum MessageType {
  NewClientClearToSend = 0x00,
  ChannelAssignmentRequest = 0x01,
  ChannelAssignmentResponse = 0x02,
  ChannelAssignmentAck = 0x03,
  ExistingClientRequest = 0x04,
  ExistingClientResponse = 0x05,
  ClearToSend = 0x06,
  NothingToSend = 0x07,
  ToggleItemRequest = 0x11,
  StatusUpdate = 0x13,
  SetTemperatureRequest = 0x20,
  SetTimeRequest = 0x21,
  SettingsRequest = 0x22,
  FilterCycles = 0x23,
  InformationResponse = 0x24,
  PreferencesResponse = 0x26,
  SetPreferenceRequest = 0x27,
  FaultLogResponse = 0x28,
  ChangeSetupRequest = 0x2a,
  GfciTestResponse = 0x2b,
  LockRequest = 0x2d,
  ConfigurationResponse = 0x2e,
  SetWifiSettingsRequest = 0x92,
  WifiModuleConfigurationResponse = 0x94,
  ToggleTestSettingRequest = 0xe0,
  UnknownError1 = 0xe1,
  UnknownError2 = 0xf0,
}

const StartOfMessage: u8 = 0x7e;
const EndOfMessage: u8 = 0x7e;

const CrcAlgorithm: Algorithm<u8> = Algorithm {
  width: 8,
  poly: 0x07,
  init: 0x02,
  xorout: 0x02,
  refin: false,
  refout: false,
  check: 0x00,
  residue: 0x00,
};
const CrcEngine: Crc<u8> = Crc::<u8>::new(&CrcAlgorithm);

impl TryFrom<&[u8]> for Message {
  type Error = ParseError;

  fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
  }
}

pub enum ParseError {
  InvalidSof,
  InvalidEof,
  CrcError,
}

impl TryFrom<Message> for Vec<u8> {
  type Error = EncodeError;

  fn try_from(value: Message) -> Result<Self, Self::Error> {
    let len = u8::try_from(5 + value.payload.len())
        .map_err(|_| EncodeError::MessageTooLong)?;

    let magic_byte = match value.channel {
      Channel::MulticastBroadcast => 0xaf,
      _ => 0xbf,
    };

    let mut result = Vec::new();
    result.push(StartOfMessage);
    result.push(len);
    result.push(value.channel.into());
    result.push(magic_byte);
    result.extend(value.payload);
    result.push(CrcEngine.checksum(&result[1..]));
    result.push(EndOfMessage);
    Ok(result)
  }
}

pub enum EncodeError {
  MessageTooLong,
}

impl From<u8> for Channel {
  fn from(value: u8) -> Self {
    match value {
      0x0a => Channel::Reserved,
      c @ 0x10 ..= 0x2f => Channel::Client(c),
      c @ 0x30 ..= 0x3f => Channel::ClientNoCTS(c),
      0xfe => Channel::MulticastRequest,
      0xff => Channel::MulticastBroadcast,
      c => Channel::Unknown(c),
    }
  }
}

impl From<Channel> for u8 {
  fn from(value: Channel) -> Self {
    match value {
      Channel::Reserved => 0x0a,
      Channel::Client(c) => c,
      Channel::ClientNoCTS(c) => c,
      Channel::MulticastRequest => 0xfe,
      Channel::MulticastBroadcast => 0xff,
      Channel::Unknown(c) => c,
    }
  }
}
