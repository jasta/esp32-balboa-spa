use topside_panel_lib::view::lcd_device::{BacklightBrightness, BacklightControl};
use embedded_hal::digital::v2::{OutputPin, PinState};
use std::fmt::Display;
use log::{info, warn};

pub struct HalBacklightControl<O> {
  pin: O,
}

impl<O> HalBacklightControl<O> {
  pub fn new(pin: O) -> Self {
    Self { pin }
  }
}

impl<O> BacklightControl for HalBacklightControl<O>
where
    O: OutputPin,
    O::Error: Display,
{
  fn set_brightness(&mut self, value: BacklightBrightness) {
    let state = match value {
      BacklightBrightness::Off => PinState::Low,
      BacklightBrightness::FullOn => PinState::High,
    };
    info!("Setting backlight to: {value:?}");
    if let Err(e) = self.pin.set_state(state) {
      warn!("Could not set backlight state: {e}");
    }
  }
}
