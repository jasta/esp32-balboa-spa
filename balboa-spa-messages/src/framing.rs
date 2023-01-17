use crc::{Algorithm, Crc};
use log::{error, info, trace, warn};
use crate::message::{EncodeError, Message};

#[derive(Debug)]
pub struct FramedReader {
  state: ReaderState,
  num_bytes_expected: Option<usize>,
  current_message: Vec<u8>,
  frames_with_errors: usize,
}

#[derive(Default, Debug)]
pub struct FramedWriter {
}

#[derive(Debug, PartialOrd, PartialEq, Clone)]
pub enum ReaderState {
  Ready,
  GotStart,
  GotLength,
  GotMessage,
  GotCrc,
  LostPlace,
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

impl Default for FramedReader {
  fn default() -> Self {
    Self {
      state: ReaderState::Ready,
      num_bytes_expected: None,
      current_message: vec![],
      frames_with_errors: 0,
    }
  }
}

impl FramedReader {
  pub fn new() -> Self {
    Default::default()
  }

  pub fn accept(&mut self, byte: u8) -> Option<Message> {
    if self.handle_byte(byte) {
      if self.state == ReaderState::Ready {
        let message = Message::from_bytes(&self.current_message);
        self.current_message.clear();
        match message {
          Ok(message) => {
            return Some(message)
          },
          Err(e) => {
            error!("Failed to parse message: {e:?}");
            self.move_to_state(ReaderState::LostPlace);
          }
        };
      }
    } else {
      self.move_to_state(ReaderState::LostPlace);
    }

    None
  }

  fn handle_byte(&mut self, byte: u8) -> bool {
    match self.state {
      ReaderState::Ready => {
        match byte {
          START_OF_MESSAGE => {
            self.move_to_state(ReaderState::GotStart);
            true
          }
          _ => false,
        }
      }
      ReaderState::GotStart => {
        match byte {
          1..=250 => {
            self.num_bytes_expected = Some(usize::from(byte) - 2);
            self.current_message.push(byte);
            self.move_to_state(ReaderState::GotLength);
            true
          }
          _ => false,
        }
      }
      ReaderState::GotLength => {
        let expected_ref = self.num_bytes_expected.as_mut().unwrap();
        match expected_ref {
          0 => false,
          _ => {
            self.current_message.push(byte);
            *expected_ref -= 1;
            if *expected_ref == 0 {
              self.move_to_state(ReaderState::GotMessage)
            }
            true
          }
        }
      }
      ReaderState::GotMessage => {
        let computed_crc = CRC_ENGINE.checksum(&self.current_message);
        if byte == computed_crc {
          self.move_to_state(ReaderState::GotCrc);
          true
        } else {
          false
        }
      }
      ReaderState::GotCrc => {
        match byte {
          END_OF_MESSAGE => {
            self.move_to_state(ReaderState::Ready);
            true
          }
          _ => false,
        }
      }
      ReaderState::LostPlace => {
        match byte {
          START_OF_MESSAGE => {
            self.move_to_state(ReaderState::GotStart);
            true
          }
          _ => false,
        }
      }
    }
  }

  fn move_to_state(&mut self, new_state: ReaderState) {
    let old_state = &self.state;
    if old_state != &new_state {
      trace!("Moving from {old_state:?} to {new_state:?}...");
      if new_state == ReaderState::LostPlace {
        self.frames_with_errors += 1;
        let errors = self.frames_with_errors;
        warn!("Communication error ({errors} total so far!) in state={old_state:?}, trying to regain stream...");
        self.num_bytes_expected = None;
        self.current_message.clear();
      } else if old_state == &ReaderState::LostPlace {
        info!("Regained stream successfully!");
      }
      self.state = new_state
    }
  }

  pub fn frames_with_errors(&self) -> usize {
    self.frames_with_errors
  }

  pub fn is_in_error(&self) -> bool {
    self.state == ReaderState::LostPlace
  }
}

impl FramedWriter {
  pub fn new() -> Self {
    Default::default()
  }

  pub fn encode(&self, message: &Message) -> Result<Vec<u8>, EncodeError> {
    let unwrapped = message.to_bytes()?;
    let mut wrapped = Vec::with_capacity(3 + unwrapped.len());
    wrapped.push(START_OF_MESSAGE);
    wrapped.extend(&unwrapped);
    wrapped.push(CRC_ENGINE.checksum(&unwrapped));
    wrapped.push(END_OF_MESSAGE);
    Ok(wrapped)
  }
}

#[cfg(test)]
mod tests {
  use log::LevelFilter;
  use crate::channel::Channel;
  use super::*;

  #[test]
  fn test_precise_happy_path() {
    let encoded = b"\x7e\x08\xfe\xbf\x01\x02\xf2\x47\x0a\x7e";
    let expected_message = Message::new(Channel::MulticastChannelAssignment, 0x1, vec![0x02, 0xf2, 0x47]);
    let expected_states = vec![
      ReaderState::GotStart,
      ReaderState::GotLength,
      ReaderState::GotLength,
      ReaderState::GotLength,
      ReaderState::GotLength,
      ReaderState::GotLength,
      ReaderState::GotLength,
      ReaderState::GotMessage,
      ReaderState::GotCrc,
      ReaderState::Ready,
    ];
    let mut expected_returns = vec![None; 9];
    expected_returns.push(Some(expected_message));
    assert_eq!(expected_returns.len(), encoded.len());

    let mut reader = FramedReader::new();

    for i in 0..encoded.len() {
      let ret = reader.accept(encoded[i]);
      assert_eq!(expected_returns[i], ret);
      assert_eq!(expected_states[i], reader.state);
    }
  }

  #[test]
  fn test_crc_error() {
    let encoded = b"\x7e\x05\xfe\xbf\x01\xff";

    let mut reader = FramedReader::new();
    for byte in encoded {
      let ret = reader.accept(*byte);
      assert_eq!(ret, None);
    }

    assert_eq!(reader.state, ReaderState::LostPlace);
    assert_eq!(reader.frames_with_errors(), 1);
  }

  #[test]
  fn test_reflexive_simple() {
    let mut reader = FramedReader::new();
    let writer = FramedWriter::new();

    let message = Message::new(Channel::MulticastChannelAssignment, 0x1, vec![0x02, 0x03, 0x04]);
    let encoded = writer.encode(&message).unwrap();
    let mut last_ret = None;
    for byte in encoded {
      last_ret = reader.accept(byte);
    }

    assert_eq!(last_ret, Some(message));
  }
}