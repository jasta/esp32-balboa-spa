use embedded_graphics::draw_target::DrawTarget;
use crate::view::window_proxy::WindowProxy;

pub trait LcdDevice {
  type Display: DrawTarget;
  type Window: WindowProxy<Self::Display>;

  fn setup(self) -> (Self::Display, Self::Window);
}
