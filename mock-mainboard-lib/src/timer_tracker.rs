use balboa_spa_messages::channel::CLIENT_CTS_RANGE;

/// Fancy logic encapsulated here that lets us spread the actions out a bit more across the
/// timer spectrum.  Doing this mostly to help debugging so that the two most common
/// messages to get spammed at startup aren't right next to each other in the timer ticks.
#[derive(Debug)]
pub struct TimerTracker {
  next_action: TickAction,
  clear_to_send_ticks: usize,
  half_clear_to_send_ticks: usize,
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum TickAction {
  NewClientClearToSend,
  ClearToSend { index: usize },
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
      clear_to_send_ticks: cts_ticks,
      half_clear_to_send_ticks: half_cts_ticks,
    }
  }

  pub fn total_ticks_per_cycle(&self) -> usize {
    // Unit tests verify this value is correct and kept up to date.
    2 + self.clear_to_send_ticks
  }

  pub fn next_action(&mut self) -> TickAction {
    let current = self.next_action;
    let next = match current {
      TickAction::NewClientClearToSend => TickAction::ClearToSend { index: 0 },
      TickAction::ClearToSend { index } => {
        let next_index = index + 1;
        if next_index == self.half_clear_to_send_ticks {
          TickAction::StatusUpdate
        } else if next_index == self.clear_to_send_ticks {
          TickAction::NewClientClearToSend
        } else {
          TickAction::ClearToSend { index: next_index }
        }
      },
      TickAction::StatusUpdate => TickAction::ClearToSend { index: self.half_clear_to_send_ticks },
    };
    self.next_action = next;
    current
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_everything() {
    let mut tracker = TimerTracker::with_cts_ticks(4);
    let expected_total_ticks = tracker.total_ticks_per_cycle();
    let mut ticks = Vec::new();
    loop {
      let next = tracker.next_action();
      if ticks.first() == Some(&next) {
        break;
      }
      ticks.push(next);
    }
    assert_eq!(ticks.len(), expected_total_ticks);
    assert_eq!(ticks, vec![
      TickAction::NewClientClearToSend,
      TickAction::ClearToSend { index: 0 },
      TickAction::ClearToSend { index: 1 },
      TickAction::StatusUpdate,
      TickAction::ClearToSend { index: 2 },
      TickAction::ClearToSend { index: 3 },
    ]);
  }
}