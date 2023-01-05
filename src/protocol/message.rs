//! Protocol definition as reverse engineered and documented here:
//! https://github.com/ccutrer/balboa_worldwide_app/wiki#physical-layer

use std::io;
use std::io::{Cursor, Read, Seek};

use byteorder::ReadBytesExt;
use crc::{Algorithm, Crc};
use num_traits::{FromPrimitive, ToPrimitive};

#[derive(Debug, Clone)]
pub struct Message {
  pub(crate) channel: Channel,
  pub message_type: MessageTypeHolder,
  pub payload: Vec<u8>,
}

#[derive(Debug, Copy, Clone)]
pub enum Channel {
  Reserved,
  Client(u8),
  ClientNoCTS(u8),
  MulticastRequest,
  MulticastBroadcast, // <-- I think?
  Unknown(u8),
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum MessageTypeHolder {
  Known(MessageType),
  Unknown(u8),
}

#[derive(num_derive::FromPrimitive, num_derive::ToPrimitive, Debug, Copy, Clone)]
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

impl Message {
  pub fn new(channel: Channel, message_type: MessageType, payload: Vec<u8>) -> Self {
    Self {
      channel,
      message_type: MessageTypeHolder::Known(message_type),
      payload
    }
  }
}

impl TryFrom<&[u8]> for Message {
  type Error = ParseError;

  fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
    let computed_crc = CrcEngine.checksum(&value[1..value.len()-1]);

    let mut cursor = Cursor::new(value);
    if cursor.read_u8()? != StartOfMessage {
      return Err(ParseError::InvalidSof);
    }
    let length = cursor.read_u8()?;
    if length < 5 {
      return Err(ParseError::InvalidLength(length));
    }
    let channel = Channel::from(cursor.read_u8()?);
    let magic_byte = cursor.read_u8()?;
    let message_type = MessageTypeHolder::from(cursor.read_u8()?);
    let mut payload: Vec<u8> = Vec::with_capacity(usize::from(length) - 5);
    let _ = cursor.read_exact(payload.as_mut_slice())?;
    let read_crc = cursor.read_u8()?;
    if cursor.read_u8()? != EndOfMessage {
      return Err(ParseError::InvalidEof);
    }
    if read_crc != computed_crc {
      return Err(ParseError::CrcError);
    }
    Ok(Message::new(channel, message_type, payload))
  }
}

#[derive(thiserror::Error, Debug)]
pub enum ParseError {
  #[error("Invalid StartOfMessage marker")]
  InvalidSof,

  #[error("Invalid EndOfMessage marker")]
  InvalidEof,

  #[error("Invalid length provided: {0}")]
  InvalidLength(u8),

  #[error("Crc check failed")]
  CrcError,

  #[error("Unexpected EOF (i.e. too few bytes in message)")]
  UnexpectedEof(#[from] io::Error),
}

impl TryFrom<Message> for Vec<u8> {
  type Error = EncodeError;

  fn try_from(value: Message) -> Result<Self, Self::Error> {
    let len = u8::try_from(5 + value.payload.len())
        .map_err(|_| EncodeError::MessageTooLong(value.payload.len()))?;

    let magic_byte = match value.channel {
      Channel::MulticastBroadcast => 0xaf,
      _ => 0xbf,
    };

    let mut result = Vec::new();
    result.push(StartOfMessage);
    result.push(len);
    result.push(value.channel.into());
    result.push(magic_byte);
    result.push(value.message_type.into());
    result.extend(value.payload);
    result.push(CrcEngine.checksum(&result[1..]));
    result.push(EndOfMessage);
    Ok(result)
  }
}

#[derive(thiserror::Error, Debug)]
pub enum EncodeError {
  #[error("Payload size={0} exceeds maximum size of 251")]
  MessageTooLong(usize),
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

impl From<u8> for MessageTypeHolder {
  fn from(value: u8) -> Self {
    match MessageType::from_u8(value) {
      Some(raw) => MessageTypeHolder::Known(raw),
      None => MessageTypeHolder::Unknown(value),
    }
  }
}

impl From<MessageTypeHolder> for u8 {
  fn from(value: MessageTypeHolder) -> Self {
    match value {
      MessageTypeHolder::Known(raw) => raw.to_u8().unwrap(),
      MessageTypeHolder::Unknown(t) => t,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_encode_decode_simple() {
    Message::new(Channel::MulticastRequest, MessageType::ChannelAssignmentRequest,
  }
}