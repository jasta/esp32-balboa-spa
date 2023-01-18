use std::collections::vec_deque::Iter;
use std::fmt::{Debug, Formatter};
use std::collections::VecDeque;

pub struct ByteRingBuffer {
  data: VecDeque<u8>,
  max_size: usize,
  dropped_count: usize,
}

impl ByteRingBuffer {
  pub fn with_max_size(max_size: usize) -> Self {
    Self {
      data: VecDeque::with_capacity(max_size),
      max_size,
      dropped_count: 0,
    }
  }

  pub fn clear(&mut self) {
    self.dropped_count = 0;
    self.data.clear();
  }

  pub fn push(&mut self, byte: u8) {
    while self.data.len() >= self.max_size {
      self.data.pop_front();
      self.dropped_count += 1;
    }
    self.data.push_back(byte);
  }

  pub fn iter(&self) -> Iter<'_, u8> {
    self.data.iter()
  }
}

impl Debug for ByteRingBuffer {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    if self.dropped_count > 0 {
      write!(f, "[missing {} bytes...] ", self.dropped_count)?;
    }
    for b in &self.data {
      write!(f, "{b:02X} ")?;
    }
    Ok(())
  }
}
