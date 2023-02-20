use topside_panel_lib::view::lcd_device::LcdDevice;
use embedded_graphics::draw_target::DrawTarget;
use embedded_hal::digital::v2::{InputPin, OutputPin};
use std::fmt::Display;
use topside_panel_lib::view::ui_handler::UiDelayMs;
use esp_idf_hal::delay::Ets;
use crate::backlight_control::HalBacklightControl;
use crate::membrane_switch::MembraneSwitchWindowProxy;

pub struct TftAndMembraneSwitchDevice<DISP, BUTTON: InputPin, BACKLIGHT> {
  display: DISP,
  buttons: MembraneSwitchWindowProxy<BUTTON, DISP>,
  backlight: HalBacklightControl<BACKLIGHT>,
}

impl<DISP, BUTTON: InputPin, BACKLIGHT> TftAndMembraneSwitchDevice<DISP, BUTTON, BACKLIGHT> {
  pub fn new(
    display: DISP,
    buttons: MembraneSwitchWindowProxy<BUTTON, DISP>,
    backlight: HalBacklightControl<BACKLIGHT>,
  ) -> Self {
    Self {
      display,
      buttons,
      backlight,
    }
  }
}

impl<DISP, BUTTON, BACKLIGHT> LcdDevice for TftAndMembraneSwitchDevice<DISP, BUTTON, BACKLIGHT>
where
    DISP: DrawTarget,
    BUTTON: InputPin,
    BUTTON::Error: Display,
    BACKLIGHT: OutputPin,
    BACKLIGHT::Error: Display,
{
  type Display = DISP;
  type Window = MembraneSwitchWindowProxy<BUTTON, DISP>;
  type Backlight = HalBacklightControl<BACKLIGHT>;

  fn setup(self) -> (Self::Display, Self::Window, Self::Backlight) {
    (self.display, self.buttons, self.backlight)
  }
}

pub struct EtsUiDelay;

impl UiDelayMs for EtsUiDelay {
  fn delay_ms(&mut self, ms: u32) {
    Ets::delay_ms(ms);
  }
}

