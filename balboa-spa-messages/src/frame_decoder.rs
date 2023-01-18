use std::fmt::Debug;
use crc::{Algorithm, Crc};
use log::{error, info, trace, warn};
use crate::message::Message;
use crate::ring_buffer::ByteRingBuffer;

const ERROR_BUF_SIZE: usize = 128;

#[derive(Debug)]
pub struct FrameDecoder {
  state: DecoderState,
  num_bytes_expected: Option<usize>,
  current_message: Vec<u8>,
  frames_with_errors: usize,
  latest_lost_bytes: ByteRingBuffer,
}

#[derive(Debug, PartialOrd, PartialEq, Clone)]
pub enum DecoderState {
  Ready,
  GotStart,
  GotLength,
  GotMessage,
  GotCrc,
  LostPlace,
  LostPlaceGotEnd,
}

pub(crate) const START_OF_MESSAGE: u8 = 0x7e;
pub(crate) const END_OF_MESSAGE: u8 = 0x7e;

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
pub(crate) const CRC_ENGINE: Crc<u8> = Crc::<u8>::new(&CRC_ALGORITHM);

impl Default for FrameDecoder {
  fn default() -> Self {
    Self {
      state: DecoderState::Ready,
      num_bytes_expected: None,
      current_message: vec![],
      frames_with_errors: 0,
      latest_lost_bytes: ByteRingBuffer::with_max_size(ERROR_BUF_SIZE),
    }
  }
}

impl FrameDecoder {
  pub fn new() -> Self {
    Default::default()
  }

  pub fn accept(&mut self, byte: u8) -> Option<Message> {
    if self.handle_byte(byte) {
      if self.state == DecoderState::Ready {
        let message = Message::from_bytes(&self.current_message);
        self.current_message.clear();
        match message {
          Ok(message) => {
            return Some(message)
          },
          Err(e) => {
            error!("Failed to parse message: {e:?}");
            self.move_to_state(DecoderState::LostPlace);
          }
        };
      }
    } else {
      self.move_to_state(DecoderState::LostPlace);
    }

    if self.is_in_error() {
      self.latest_lost_bytes.push(byte);
    }

    None
  }

  fn handle_byte(&mut self, byte: u8) -> bool {
    match self.state {
      DecoderState::Ready => {
        match byte {
          START_OF_MESSAGE => {
            self.move_to_state(DecoderState::GotStart);
            true
          }
          _ => false,
        }
      }
      DecoderState::GotStart => {
        match byte {
          // Maximum length set at START_OF_MESSAGE-1 so that we can better catch a
          // misaligned sequence of bytes that would cause us to get "stuck" reading for quite
          // some time.
          c @ 5..=START_OF_MESSAGE if c != START_OF_MESSAGE => {
            self.num_bytes_expected = Some(usize::from(byte) - 2);
            self.current_message.push(byte);
            self.move_to_state(DecoderState::GotLength);
            true
          }
          _ => false,
        }
      }
      DecoderState::GotLength => {
        let expected_ref = self.num_bytes_expected.as_mut().unwrap();
        match expected_ref {
          0 => false,
          _ => {
            self.current_message.push(byte);
            *expected_ref -= 1;
            if *expected_ref == 0 {
              self.move_to_state(DecoderState::GotMessage);
            }
            true
          }
        }
      }
      DecoderState::GotMessage => {
        let computed_crc = CRC_ENGINE.checksum(&self.current_message);
        if byte == computed_crc {
          self.move_to_state(DecoderState::GotCrc);
          true
        } else {
          false
        }
      }
      DecoderState::GotCrc => {
        match byte {
          END_OF_MESSAGE => {
            self.move_to_state(DecoderState::Ready);
            true
          }
          _ => false,
        }
      }
      DecoderState::LostPlace => {
        match byte {
          END_OF_MESSAGE => {
            self.move_to_state(DecoderState::LostPlaceGotEnd);
            true
          }
          _ => false,
        }
      }
      DecoderState::LostPlaceGotEnd => {
        match byte {
          START_OF_MESSAGE => {
            self.move_to_state(DecoderState::GotStart);
            true
          }
          _ => false,
        }
      }
    }
  }

  fn move_to_state(&mut self, new_state: DecoderState) -> bool {
    let old_state = self.state.clone();
    if old_state != new_state {
      trace!("Moving from {old_state:?} to {new_state:?}...");
      let was_in_error = self.is_in_error();
      self.state = new_state;
      let is_in_error = self.is_in_error();
      if is_in_error != was_in_error {
        if is_in_error {
          self.frames_with_errors += 1;
          let errors = self.frames_with_errors;
          warn!("Communication error ({errors} total so far!) in state={old_state:?}, trying to regain stream...");
          self.num_bytes_expected = None;
          self.current_message.clear();
        } else if was_in_error {
          info!("Regained stream successfully: {:?}", self.latest_lost_bytes);
          self.latest_lost_bytes.clear();
        }
      }
      return true;
    }
    false
  }

  pub fn frames_with_errors(&self) -> usize {
    self.frames_with_errors
  }

  pub fn is_in_error(&self) -> bool {
    matches!(self.state, DecoderState::LostPlace | DecoderState::LostPlaceGotEnd)
  }
}

#[cfg(test)]
mod tests {
  use log::LevelFilter;
  use crate::channel::Channel;
  use crate::frame_encoder::FrameEncoder;
  use super::*;

  #[test]
  fn test_precise_happy_path() {
    let encoded = b"\x7e\x08\xfe\xbf\x01\x02\xf2\x47\x0a\x7e";
    let expected_message = Message::new(Channel::MulticastChannelAssignment, 0x1, vec![0x02, 0xf2, 0x47]);
    let expected_states = vec![
      DecoderState::GotStart,
      DecoderState::GotLength,
      DecoderState::GotLength,
      DecoderState::GotLength,
      DecoderState::GotLength,
      DecoderState::GotLength,
      DecoderState::GotLength,
      DecoderState::GotMessage,
      DecoderState::GotCrc,
      DecoderState::Ready,
    ];
    let mut expected_returns = vec![None; 9];
    expected_returns.push(Some(expected_message));
    assert_eq!(expected_returns.len(), encoded.len());

    let mut reader = FrameDecoder::new();

    for i in 0..encoded.len() {
      let ret = reader.accept(encoded[i]);
      assert_eq!(expected_returns[i], ret);
      assert_eq!(expected_states[i], reader.state);
    }
  }

  #[test]
  fn test_crc_error() {
    let encoded = b"\x7e\x05\xfe\xbf\x01\xff";

    let mut reader = FrameDecoder::new();
    for byte in encoded {
      let ret = reader.accept(*byte);
      assert_eq!(ret, None);
    }

    assert_eq!(reader.state, DecoderState::LostPlace);
    assert_eq!(reader.frames_with_errors(), 1);
  }

  #[test]
  fn test_regained_stream() {
    let _ = env_logger::builder().filter_level(LevelFilter::Trace).is_test(true).try_init();

    let encoded_bad = b"\x4f\x00\xdb\x7e";
    let encoded_bad_twice: Vec<_> = encoded_bad.iter()
        .chain(encoded_bad.iter())
        .copied()
        .collect();
    let writer = FrameEncoder::new();
    let message = Message::new(Channel::MulticastChannelAssignment, 0x1, vec![0x02, 0x03, 0x04]);
    let encoded_correct = writer.encode(&message).unwrap();

    let mut reader = FrameDecoder::new();
    let first = decode_one(&mut reader, &encoded_correct);
    assert_eq!(first, Some(message.clone()));
    let second = decode_one(&mut reader, encoded_bad);
    assert_eq!(reader.state, DecoderState::LostPlaceGotEnd);
    assert_eq!(second, None);
    let third = decode_one(&mut reader, encoded_bad);
    assert_eq!(reader.state, DecoderState::LostPlaceGotEnd);
    assert_eq!(third, None);
    let error_buf: Vec<_> = reader.latest_lost_bytes.iter().copied().collect();
    assert_eq!(error_buf, encoded_bad_twice);

    let first_correct = reader.accept(encoded_correct[0]);
    assert_eq!(first_correct, None);
    assert_eq!(reader.state, DecoderState::GotStart);
    let third = decode_one(&mut reader, &encoded_correct[1..]);
    assert_eq!(third, Some(message));

    assert_eq!(reader.frames_with_errors, 1);
  }

  #[test]
  fn test_reflexive_simple() {
    let mut reader = FrameDecoder::new();
    let writer = FrameEncoder::new();

    let message = Message::new(Channel::MulticastChannelAssignment, 0x1, vec![0x02, 0x03, 0x04]);
    let encoded = writer.encode(&message).unwrap();
    let decoded = decode_one(&mut reader, &encoded);

    assert_eq!(decoded, Some(message));
  }

  fn decode_one(reader: &mut FrameDecoder, bytes: &[u8]) -> Option<Message> {
    let mut last_ret = None;
    for byte in bytes {
      last_ret = reader.accept(*byte);
    }
    last_ret
  }
}