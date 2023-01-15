use std::cmp::Ordering;
use core::ops::RangeInclusive;

const CLIENT_CTS_RANGE: RangeInclusive<u8> = 0x10 ..= 0x2f;

#[derive(Debug, Hash, PartialEq, Eq, Copy, Clone)]
pub enum Channel {
  WifiModule,
  Client(u8),
  ClientNoCTS(u8),
  MulticastChannelAssignment,
  MulticastBroadcast, // <-- I think?
  Unknown(u8),
}

impl Channel {
  pub fn new_client_channel(index: usize) -> Result<Channel, ChannelOverflow> {
    let result = u8::try_from(0x10 + index)
        .map_err(|_| ChannelOverflow)?;
    if !CLIENT_CTS_RANGE.contains(&result) {
      return Err(ChannelOverflow);
    }
    Ok(Channel::from(result))
  }

  pub fn to_magic_byte(&self) -> u8 {
    match self {
      Channel::MulticastBroadcast => 0xaf,
      _ => 0xbf,
    }
  }
}

pub struct ChannelOverflow;

impl From<u8> for Channel {
  fn from(value: u8) -> Self {
    match value {
      0x0a => Channel::WifiModule,
      c @ 0x10 ..= 0x2f => Channel::Client(c),
      c @ 0x30 ..= 0x3f => Channel::ClientNoCTS(c),
      0xfe => Channel::MulticastChannelAssignment,
      0xff => Channel::MulticastBroadcast,
      c => Channel::Unknown(c),
    }
  }
}

impl From<&Channel> for u8 {
  fn from(value: &Channel) -> Self {
    match *value {
      Channel::WifiModule => 0x0a,
      Channel::Client(c) => c,
      Channel::ClientNoCTS(c) => c,
      Channel::MulticastChannelAssignment => 0xfe,
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
