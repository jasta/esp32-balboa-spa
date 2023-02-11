use std::time::Duration;
use crate::view::user_input_event::UserInputEvent;

pub trait WindowProxy<D> {
  /// Approximate duration to wait between each call to [Self::events].  This is especially
  /// important to respect for physical interfaces which require active polling in order to
  /// sense button pushes.
  fn event_update_interval(&self) -> Duration;

  /// Get events from the window proxy, such as button clicks or window quit.  Should be called
  /// approximately once ever [Self::event_update_interval].
  fn events(&mut self) -> Vec<UserInputEvent>;

  /// Give the proxy a chance to spy the display after it is drawn.  This is generally only used
  /// by the simulator to implement things like screenshot-based integration testing.
  fn update(&mut self, display: &D);
}
