use balboa_spa_messages::channel::{Channel, CLIENT_CTS_RANGE};

/// Fancy logic encapsulated here that lets us spread the actions out a bit more across the
/// timer spectrum.  Doing this mostly to help debugging so that the two most common
/// messages to get spammed at startup aren't right next to each other in the timer ticks.
#[derive(Debug)]
pub struct TimerTracker {
  next_action: TickAction,
  dynamic_tick_helper: DynamicTickHelper,
  clear_to_send_ticks: usize,
  half_clear_to_send_ticks: usize,
}

#[derive(Debug, Default)]
struct DynamicTickHelper {
  available_channels: Vec<Channel>,
  next_index: usize,
  next_tick: usize,
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum TickAction {
  NewClientClearToSend,
  ClearToSend { channel: Channel },
  Nothing,
  StatusUpdate,
}

impl Default for TimerTracker {
  fn default() -> Self {
    TimerTracker::with_cts_ticks(CLIENT_CTS_RANGE.len())
  }
}

impl TimerTracker {
  pub fn new() -> Self {
    Default::default()
  }

  pub fn with_cts_ticks(cts_ticks: usize) -> Self {
    let half_cts_ticks = cts_ticks / 2;
    Self {
      next_action: TickAction::NewClientClearToSend,
      dynamic_tick_helper: Default::default(),
      clear_to_send_ticks: cts_ticks,
      half_clear_to_send_ticks: half_cts_ticks,
    }
  }

  pub fn total_ticks_per_cycle(&self) -> usize {
    // Unit tests verify this value is correct and kept up to date.
    2 + self.clear_to_send_ticks
  }

  pub fn next_action(&mut self, available_channels: impl Fn() -> Vec<Channel>) -> TickAction {
    let current = self.next_action;
    let next = match current {
      TickAction::NewClientClearToSend => {
        self.dynamic_tick_helper = DynamicTickHelper::new(available_channels());
        self.dynamic_tick_helper.next_action()
      },
      TickAction::ClearToSend { .. } |
      TickAction::Nothing => {
        let helper = &mut self.dynamic_tick_helper;
        let next_tick = helper.next_tick;
        if next_tick == self.half_clear_to_send_ticks {
          TickAction::StatusUpdate
        } else if next_tick == self.clear_to_send_ticks {
          TickAction::NewClientClearToSend
        } else {
          helper.next_action()
        }
      },
      TickAction::StatusUpdate => {
        self.dynamic_tick_helper.next_action()
      },
    };
    self.next_action = next;
    current
  }
}

impl DynamicTickHelper {
  pub fn new(available_channels: Vec<Channel>) -> Self {
    Self {
      available_channels,
      next_index: 0,
      next_tick: 0,
    }
  }

  pub fn next_action(&mut self) -> TickAction {
    let current_index = self.next_index;

    self.next_index += 1;
    if self.next_index >= self.available_channels.len() {
      self.next_index = 0;
    }
    self.next_tick += 1;

    match self.available_channels.get(current_index).copied() {
      Some(channel) => TickAction::ClearToSend { channel },
      None => TickAction::Nothing,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_with_clients() {
    let tracker = TimerTracker::with_cts_ticks(4);
    let expected_total_ticks = tracker.total_ticks_per_cycle();
    let channel0 = Channel::new_client_channel(0).unwrap();
    let channel1 = Channel::new_client_channel(1).unwrap();
    let ticks = run_one_pass(tracker, vec![channel0, channel1]);
    assert_eq!(ticks, vec![
      TickAction::NewClientClearToSend,
      TickAction::ClearToSend { channel: channel0 },
      TickAction::ClearToSend { channel: channel1 },
      TickAction::StatusUpdate,
      TickAction::ClearToSend { channel: channel0 },
      TickAction::ClearToSend { channel: channel1 },
    ]);
    assert_eq!(ticks.len(), expected_total_ticks);
  }

  #[test]
  fn test_without_clients() {
    let tracker = TimerTracker::with_cts_ticks(4);
    let expected_total_ticks = tracker.total_ticks_per_cycle();
    let ticks = run_one_pass(tracker, vec![]);
    assert_eq!(ticks, vec![
      TickAction::NewClientClearToSend,
      TickAction::Nothing,
      TickAction::Nothing,
      TickAction::StatusUpdate,
      TickAction::Nothing,
      TickAction::Nothing,
    ]);
    assert_eq!(ticks.len(), expected_total_ticks);
  }

  fn run_one_pass(mut tracker: TimerTracker, available_channels: Vec<Channel>) -> Vec<TickAction> {
    let mut ticks = Vec::new();
    loop {
      let next = tracker.next_action(|| available_channels.clone());
      if ticks.first() == Some(&next) {
        break;
      }
      ticks.push(next);
    }
    ticks
  }
}