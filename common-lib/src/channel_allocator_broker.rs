use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use lazy_static::lazy_static;

lazy_static! {
  pub(crate) static ref GLOBAL_BROKER: Arc<ChannelAllocatorBroker> =
    Arc::new(ChannelAllocatorBroker::new());
}

/// Mechanism to allow only a single CtsStateMachine to acquire a new channel at a time.  This
/// is particularly a problem for us with the BusTransport because the WiFi and Topside panel
/// modules are receiving the NewClientClearToSend message at almost precisely the same time.
#[derive(Debug)]
pub struct ChannelAllocatorBroker {
  active_token: Arc<AtomicBool>,
}

impl ChannelAllocatorBroker {
  pub fn new() -> Self {
    Self {
      active_token: Arc::new(AtomicBool::new(false)),
    }
  }

  pub fn try_allocate(&self) -> Option<AllocatorToken> {
    match self.active_token.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst) {
      Ok(_) => Some(AllocatorToken { inner: self.active_token.clone() }),
      Err(_) => None
    }
  }
}

#[derive(Debug)]
pub struct AllocatorToken {
  inner: Arc<AtomicBool>,
}

impl AllocatorToken {
  pub fn release(self) {
    // Just drop...
  }
}

impl Drop for AllocatorToken {
  fn drop(&mut self) {
    self.inner.store(false, Ordering::SeqCst);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_allocate_only_once() {
    let allocator = ChannelAllocatorBroker::new();
    let token = allocator.try_allocate().unwrap();
    assert!(allocator.try_allocate().is_none());
    drop(token);
    assert!(allocator.try_allocate().is_some());
  }
}