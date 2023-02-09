use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

pub fn broadcast_channel<T>(max_queue_len: usize) -> (BroadcastSender<T>, BroadcastReceiver<T>) {
  let mut inner = BroadcastInner {
    senders: VecDeque::new(),
    max_queue_len,
  };
  let rx = inner.add_sender();
  let inner = Arc::new(Mutex::new(inner));
  let brx = BroadcastReceiver { rx, inner: inner.clone() };
  let btx = BroadcastSender { inner };
  (btx, brx)
}

#[derive(Clone)]
pub struct BroadcastSender<T> {
  inner: Arc<Mutex<BroadcastInner<T>>>,
}

impl<T: Clone> BroadcastSender<T> {
  /// Non-blocking broadcast to all subscribers.  If a listener has exceeded the maximum queue length
  /// it will be forcefully removed rather than block.
  pub fn send_to_all(&mut self, event: &T) {
    self.inner.lock().unwrap().senders.retain(|listener| {
      // try_send is convenient for our purposes, it will either enqueue up to max_queue_len
      // or return Full/Disconnected as an error.  In either error case, we want to just remove
      // the listener and move on.
      listener.try_send(event.clone()).is_ok()
    });
  }
}

pub struct BroadcastReceiver<T> {
  rx: Receiver<T>,
  inner: Arc<Mutex<BroadcastInner<T>>>,
}

impl<T> BroadcastReceiver<T> {
  pub fn rx(&self) -> &Receiver<T> {
    &self.rx
  }
}

impl<T> Clone for BroadcastReceiver<T> {
  fn clone(&self) -> Self {
    let rx = self.inner.lock().unwrap().add_sender();
    BroadcastReceiver {
      rx,
      inner: self.inner.clone(),
    }
  }
}

struct BroadcastInner<T> {
  senders: VecDeque<SyncSender<T>>,
  max_queue_len: usize,
}

impl<T> BroadcastInner<T> {
  fn add_sender(&mut self) -> Receiver<T> {
    let (tx, rx) = sync_channel(self.max_queue_len);
    self.senders.push_back(tx);
    rx
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_multiple_subscribers() {
    let (mut tx, rx1) = broadcast_channel(1);
    let rx2 = rx1.clone();

    let expected = 1;
    tx.send_to_all(&expected);
    assert_eq!(rx1.rx().recv().unwrap(), expected);
    assert_eq!(rx2.rx().recv().unwrap(), expected);
  }
}