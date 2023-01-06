//! Protocol definition as reverse engineered and documented here:
//! https://github.com/ccutrer/balboa_worldwide_app/wiki#physical-layer

use std::cmp::Ordering;
use std::io;
use std::io::{Cursor, Read};

use byteorder::ReadBytesExt;
use crc::{Algorithm, Crc};

#[derive(Debug, PartialOrd, PartialEq, Clone)]
pub struct Message {
  pub(crate) channel: Channel,
  pub message_type: u8,
  pub payload: Vec<u8>,
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum Channel {
  Reserved,
  Client(u8),
  ClientNoCTS(u8),
  MulticastRequest,
  MulticastBroadcast, // <-- I think?
  Unknown(u8),
}

const START_OF_MESSAGE: u8 = 0x7e;
const END_OF_MESSAGE: u8 = 0x7e;

const CRC_ALGORITHM: Algorithm<u8> = Algorithm {
  width: 8,
  poly: 0x07,
  init: 0x02,
  xorout: 0x02,
  refin: false,
  refout: false,
  check: 0x00,
  residue: 0x00,
};
const CRC_ENGINE: Crc<u8> = Crc::<u8>::new(&CRC_ALGORITHM);

impl Message {
  pub fn new(channel: Channel, message_type: u8, payload: Vec<u8>) -> Self {
    Self { channel, message_type, payload }
  }

  pub fn from_bytes(packet: &[u8]) -> Result<Self, ParseError> {
    Message::try_from(packet)
  }

  pub fn to_bytes(&self) -> Result<Vec<u8>, EncodeError> {
    Vec::<u8>::try_from(self)
  }
}

impl TryFrom<&[u8]> for Message {
  type Error = ParseError;

  fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
    let computed_crc = CRC_ENGINE.checksum(&value[1..value.len()-2]);

    let mut cursor = Cursor::new(value);
    if cursor.read_u8()? != START_OF_MESSAGE {
      return Err(ParseError::InvalidSof);
    }
    let length = cursor.read_u8()?;
    if length < 5 {
      return Err(ParseError::InvalidPayloadLength(length));
    }
    let channel = Channel::from(cursor.read_u8()?);
    let magic_byte = cursor.read_u8()?;
    let message_type = cursor.read_u8()?;
    let mut payload: Vec<u8> = vec![0; usize::from(length) - 5];
    cursor.read_exact(payload.as_mut_slice())?;
    let read_crc = cursor.read_u8()?;
    if cursor.read_u8()? != END_OF_MESSAGE {
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
  InvalidPayloadLength(u8),

  #[error("Crc check failed")]
  CrcError,

  #[error("Unexpected EOF (i.e. too few bytes in message)")]
  UnexpectedEof(#[from] io::Error),
}

impl TryFrom<&Message> for Vec<u8> {
  type Error = EncodeError;

  fn try_from(value: &Message) -> Result<Self, Self::Error> {
    let len = u8::try_from(5 + value.payload.len())
        .map_err(|_| EncodeError::MessageTooLong(value.payload.len()))?;

    let magic_byte = match value.channel {
      Channel::MulticastBroadcast => 0xaf,
      _ => 0xbf,
    };

    let mut result = Vec::with_capacity(7 + value.payload.len());
    result.push(START_OF_MESSAGE);
    result.push(len);
    result.push(u8::from(&value.channel));
    result.push(magic_byte);
    result.push(value.message_type);
    result.extend(&value.payload);
    result.push(CRC_ENGINE.checksum(&result[1..]));
    result.push(END_OF_MESSAGE);
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

impl From<&Channel> for u8 {
  fn from(value: &Channel) -> Self {
    match *value {
      Channel::Reserved => 0x0a,
      Channel::Client(c) => c,
      Channel::ClientNoCTS(c) => c,
      Channel::MulticastRequest => 0xfe,
      Channel::MulticastBroadcast => 0xff,
      Channel::Unknown(c) => c,
    }
  }
}

impl PartialOrd for Channel {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    u8::from(self).partial_cmp(&u8::from(other))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_encode_against_ref() {
  }

  #[test]
  fn test_decode_against_ref() {
  }

  #[test]
  fn test_reflexive_simple() {
    let original = Message::new(Channel::Client(0x10), 0x1, vec![0x2, 0x3, 0x4, 0x5]);
    let encoded = original.to_bytes().unwrap();
    let decoded = Message::from_bytes(&encoded).unwrap();
    assert_eq!(decoded, original);
  }
}