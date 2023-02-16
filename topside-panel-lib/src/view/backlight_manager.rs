use std::time::{Duration, Instant};
use crate::view::lcd_device::{BacklightBrightness, BacklightControl};

/// Amount of time to keep the backlight on without user interaction.
const BACKLIGHT_USER_WAIT: Duration = Duration::from_secs(30);

pub struct BacklightManager<B> {
  backlight: B,
  current_value: BacklightBrightness,
  last_user_interaction: Instant,
}

impl<B: BacklightControl> BacklightManager<B> {
  pub fn init(mut backlight: B) -> Self {
    let current_value = BacklightBrightness::FullOn;
    backlight.set_brightness(current_value);
    Self {
      backlight,
      current_value,
      last_user_interaction: Instant::now(),
    }
  }

  pub fn mark_user_activity(&mut self, at_time: Instant) {
    self.last_user_interaction = at_time;
    self.maybe_set_brightness(BacklightBrightness::FullOn);

  }

  pub fn detect_inactivity(&mut self, now: Instant) {
    if self.current_value != BacklightBrightness::Off {
      let elapsed = now - self.last_user_interaction;
      if elapsed > BACKLIGHT_USER_WAIT {
        self.maybe_set_brightness(BacklightBrightness::Off);
      }
    }
  }

  fn maybe_set_brightness(&mut self, value: BacklightBrightness) {
    if self.current_value != value {
      self.current_value = value;
      self.backlight.set_brightness(value);
    }
  }
}