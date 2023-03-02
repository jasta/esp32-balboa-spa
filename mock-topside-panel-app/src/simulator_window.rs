use std::thread;
use std::time::Duration;
use embedded_graphics_simulator::{OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::geometry::Size;
use embedded_graphics_simulator::sdl2::Keycode;
use log::info;
use topside_panel_lib::model::key_event::{Key, KeyEvent};
use topside_panel_lib::view::lcd_device::{BacklightBrightness, BacklightControl, LcdDevice};
use topside_panel_lib::view::ui_handler::UiDelayMs;
use topside_panel_lib::view::user_input_event::UserInputEvent;
use topside_panel_lib::view::window_proxy::WindowProxy;

const TARGET_WINDOW_UPDATE_INTERVAL: Duration = Duration::from_millis(20);

pub struct SleepDelay;
impl UiDelayMs for SleepDelay {
  fn delay_ms(&mut self, ms: u32) {
    thread::sleep(Duration::from_millis(ms.into()));
  }
}

#[derive(Default)]
pub struct SimulatorDevice;

impl LcdDevice for SimulatorDevice {
  type Display = SimulatorDisplay<Rgb565>;
  type Window = SimulatorWindowProxy;
  type Backlight = MockBacklight;

  fn setup(self) -> (Self::Display, Self::Window, Self::Backlight) {
    let display = SimulatorDisplay::<Rgb565>::new(Size::new(480, 320));
    let output_settings = OutputSettingsBuilder::new()
        .scale(2)
        .build();
    let window = Window::new("Mock Topside Panel", &output_settings);
    (display, SimulatorWindowProxy { window }, MockBacklight)
  }
}

pub struct SimulatorWindowProxy {
  window: Window,
}

impl WindowProxy<SimulatorDisplay<Rgb565>> for SimulatorWindowProxy {
  fn event_update_interval(&self) -> Duration {
    TARGET_WINDOW_UPDATE_INTERVAL
  }

  fn events(&mut self) -> Vec<UserInputEvent> {
    self.window.events()
        .filter_map(|ref e| {
          match e {
            SimulatorEvent::KeyUp { keycode, .. } => {
              map_keycode(keycode)
                  .map(|key| UserInputEvent::KeyEvent(KeyEvent::KeyUp { key }))
            }
            SimulatorEvent::KeyDown { keycode, .. } => {
              map_keycode(keycode)
                  .map(|key| UserInputEvent::KeyEvent(KeyEvent::KeyDown { key }))
            },
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

fn map_keycode(keycode: &Keycode) -> Option<Key> {
  match keycode {
    Keycode::Up => Some(Key::Up),
    Keycode::Down => Some(Key::Down),
    Keycode::J => Some(Key::Jets1),
    Keycode::L => Some(Key::Light),
    k => {
      info!("Got: {k:?}");
      None
    },
  }
}

pub struct MockBacklight;

impl BacklightControl for MockBacklight {
  fn set_brightness(&mut self, value: BacklightBrightness) {
    info!("set_brightness={value:?}");
  }
}