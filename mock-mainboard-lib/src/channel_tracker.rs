use std::collections::hash_map::Entry;
use std::collections::HashMap;
use balboa_spa_messages::channel::Channel;
use crate::main_board::HandlingError;

/// Number of _consecutive_ failures to respond to ClearToSend before we remove the channel and
/// allow it to be reallocated.  Note that the official main board does not support this feature
/// but it's very useful for testing.
const DEFAULT_MAX_CLEAR_TO_SEND_FAILURES: usize = 20;

#[derive(Debug)]
pub(crate) struct ChannelTracker {
  lookup_by_device: HashMap<DeviceKey, Channel>,
  records: HashMap<Channel, ChannelRecord>,
  max_cts_failures: usize,
}

#[derive(Debug, Hash, PartialEq, Eq, Copy, Clone)]
pub(crate) struct DeviceKey {
  pub device_type: u8,
  pub client_hash: u16,
}

#[derive(Debug)]
struct ChannelRecord {
  device_key: DeviceKey,
  channel: Channel,
  consecutive_cts_failures: usize,
}

impl Default for ChannelTracker {
  fn default() -> Self {
    Self {
      lookup_by_device: HashMap::new(),
      records: HashMap::new(),
      max_cts_failures: DEFAULT_MAX_CLEAR_TO_SEND_FAILURES,
    }
  }
}

impl ChannelTracker {
  pub fn new(max_cts_failures: usize) -> Self {
    Self {
      max_cts_failures,
      ..Default::default()
    }
  }

  pub fn len(&self) -> usize {
    self.records.len()
  }

  pub fn select_channel(&mut self, key: DeviceKey) -> Result<Channel, HandlingError> {
    let len = self.lookup_by_device.len();
    let channel = match self.lookup_by_device.entry(key) {
      Entry::Occupied(o) => o.get().to_owned(),
      Entry::Vacant(v) => {
        let new_channel = Channel::new_client_channel(len)
            .map_err(|_| HandlingError::ClientNeedsReconnect("channel overflow".to_owned()))?;
        let record = ChannelRecord::new(key, new_channel);

        v.insert(new_channel);
        self.records.insert(new_channel, record);
        new_channel
      }
    };
    Ok(channel)
  }

  pub fn is_allocated(&self, channel: &Channel) -> bool {
    self.records.contains_key(channel)
  }

  pub fn record_cts_success(&mut self, channel: Channel) {
    if let Some(record) = self.records.get_mut(&channel) {
      record.consecutive_cts_failures = 0;
    }
  }

  pub fn record_cts_failure(&mut self, channel: Channel) -> CtsFailureAction {
    match self.records.entry(channel) {
      Entry::Occupied(mut o) => {
        let record = o.get_mut();
        record.consecutive_cts_failures += 1;
        if record.consecutive_cts_failures >= self.max_cts_failures {
          self.lookup_by_device.remove(&record.device_key);
          o.remove();
          CtsFailureAction::ChannelRemoved
        } else {
          CtsFailureAction::Tolerated
        }
      },
      Entry::Vacant(_) => CtsFailureAction::ChannelNotFound,
    }
  }
}

#[derive(Debug, PartialEq)]
pub enum CtsFailureAction {
  ChannelNotFound,
  ChannelRemoved,
  Tolerated,
}

impl ChannelRecord {
  pub fn new(device: DeviceKey, channel: Channel) -> Self {
    Self { device_key: device, channel, consecutive_cts_failures: 0 }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_cts_failures() {
    let mut tracker = ChannelTracker::new(2);
    let channel = tracker.select_channel(DeviceKey { device_type: 0, client_hash: 0 }).unwrap();
    let expected_actions = [
      CtsFailureAction::Tolerated,
      CtsFailureAction::ChannelRemoved,
      CtsFailureAction::ChannelNotFound,
    ];

    for expected in expected_actions {
      let action = tracker.record_cts_failure(channel);
      assert_eq!(action, expected);
    }

    assert_eq!(tracker.records.len(), 0);
    assert_eq!(tracker.lookup_by_device.len(), 0);
  }
}