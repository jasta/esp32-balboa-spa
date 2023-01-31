use embedded_graphics_simulator::{OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::geometry::Size;
use embedded_graphics_simulator::sdl2::Keycode;
use log::info;
use topside_panel_lib::topside_panel::Button;
use topside_panel_lib::ui_handler::{LcdDevice, UserInputEvent, WindowProxy};

#[derive(Default)]
pub struct SimulatorDevice;

impl LcdDevice for SimulatorDevice {
  type Display = SimulatorDisplay<Rgb565>;
  type Window = SimulatorWindowProxy;

  fn setup(self) -> (Self::Display, Self::Window) {
    let display = SimulatorDisplay::<Rgb565>::new(Size::new(480, 320));
    let output_settings = OutputSettingsBuilder::new().build();
    let window = Window::new("Mock Topside Panel", &output_settings);
    (display, SimulatorWindowProxy { window })
  }
}

pub struct SimulatorWindowProxy {
  window: Window,
}

impl WindowProxy<SimulatorDisplay<Rgb565>> for SimulatorWindowProxy {
  fn events(&mut self) -> Vec<UserInputEvent> {
    self.window.events()
        .filter_map(|ref e| {
          match e {
            SimulatorEvent::KeyUp { keycode, keymod, repeat } => {
              match keycode {
                Keycode::Up => Some(UserInputEvent::ButtonPressed(Button::Up)),
                Keycode::Down => Some(UserInputEvent::ButtonPressed(Button::Down)),
                Keycode::J => Some(UserInputEvent::ButtonPressed(Button::Jets1)),
                Keycode::L => Some(UserInputEvent::ButtonPressed(Button::Light)),
                _ => {
                  info!("Got: {e:?}");
                  None
                }
              }
            }
            SimulatorEvent::Quit => Some(UserInputEvent::Quit),
            _ => None,
          }
        })
        .collect::<Vec<_>>()
  }

  fn update(&mut self, display: &SimulatorDisplay<Rgb565>) {
    self.window.update(display);
  }
}
