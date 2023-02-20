use topside_panel_lib::view::window_proxy::WindowProxy;
use embedded_hal::digital::v2::InputPin;
use std::fmt::Display;
use std::time::Duration;
use topside_panel_lib::view::user_input_event::UserInputEvent;
use debounced_pin::{ActiveLow, Debounce, DebouncedInputPin, DebounceState};
use log::warn;
use topside_panel_lib::model::button::Button;
use std::marker::PhantomData;
use esp_idf_hal::gpio::{AnyIOPin, Input, PinDriver, Pull};
use esp_idf_sys::EspError;

pub struct MembraneSwitchWindowProxy<I: InputPin, DISP> {
  event_update_interval: Duration,
  button_map: Vec<(DebouncedInputPin<I, ActiveLow>, Button)>,
  _phantom: PhantomData<DISP>,
}

impl<I: InputPin, DISP> MembraneSwitchWindowProxy<I, DISP> {
  pub fn new(button_map: Vec<(DebouncedInputPin<I, ActiveLow>, Button)>) -> Self {
    let debounced_map: Vec<_> = button_map.into_iter()
        .map(|mapping| {
          (mapping.0, mapping.1)
        })
        .collect();
    Self {
      event_update_interval: Duration::from_millis(1),
      button_map: debounced_map,
      _phantom: PhantomData,
    }
  }
}

impl<I, DISP> WindowProxy<DISP> for MembraneSwitchWindowProxy<I, DISP>
where
    I: InputPin,
    I::Error: Display,
{
  fn event_update_interval(&self) -> Duration {
    self.event_update_interval
  }

  fn events(&mut self) -> Vec<UserInputEvent> {
    self.button_map.iter_mut()
        .filter_map(|(physical, virt)| {
          match physical.update() {
            Ok(DebounceState::Active) => Some(UserInputEvent::ButtonPressed(virt.to_owned())),
            Err(e) => {
              warn!("Could not detect button {:?}: {e}", virt);
              None
            }
            _ => None,
          }
        })
        .collect()
  }

  fn update(&mut self, _display: &DISP) {
    // Not relevant for physical displays...
  }
}

pub fn debounced(
    pin: AnyIOPin,
) -> Result<DebouncedInputPin<PinDriver<'static, AnyIOPin, Input>, ActiveLow>, EspError> {
  let mut raw_input = PinDriver::input(pin)?;
  raw_input.set_pull(Pull::Up)?;
  Ok(DebouncedInputPin::new(raw_input.into_input()?, ActiveLow))
}
