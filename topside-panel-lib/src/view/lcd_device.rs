use embedded_graphics::draw_target::DrawTarget;
use log::info;
use crate::view::window_proxy::WindowProxy;

pub trait LcdDevice {
  type Display: DrawTarget;
  type Window: WindowProxy<Self::Display>;
  type Backlight: BacklightControl;

  fn setup(self) -> (Self::Display, Self::Window, Self::Backlight);
}

pub trait BacklightControl {
  fn set_brightness(&mut self, value: BacklightBrightness);
}

#[derive(Debug, PartialEq, Eq)]
pub enum BacklightBrightness {
  Off,
  FullOn,
}
