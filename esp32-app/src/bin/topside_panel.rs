use std::fmt::Display;
use std::marker::PhantomData;
use std::time::Duration;

use anyhow::anyhow;
use debounced_pin::{ActiveLow, Debounce, DebouncedInputPin, DebounceState};
use display_interface_spi::SPIInterfaceNoCS;
use embedded_graphics::prelude::DrawTarget;
use embedded_hal::digital::v2::{InputPin, OutputPin, PinState};
use embedded_hal::spi::MODE_0;
use esp_idf_hal::delay::Ets;
use esp_idf_hal::gpio::{AnyInputPin, AnyOutputPin, Gpio0, Input, Output, PinDriver};
use esp_idf_hal::peripheral::Peripheral;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::spi;
use esp_idf_hal::spi::config::V02Type;
use esp_idf_hal::spi::Dma;
use esp_idf_hal::spi::SpiDeviceDriver;
use esp_idf_hal::units::FromValueType;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_sys::EspError;
use log::{error, info, warn};
use mipidsi::{Builder, ColorOrder, Orientation};
use topside_panel_lib::app::topside_panel_app::TopsidePanelApp;
use topside_panel_lib::model::button::Button;
use topside_panel_lib::view::lcd_device::{BacklightBrightness, BacklightControl, LcdDevice};
use topside_panel_lib::view::user_input_event::UserInputEvent;
use topside_panel_lib::view::window_proxy::WindowProxy;
use wifi_module_lib::advertisement::Advertisement;

use esp_app::esp_uart_transport::EspUartTransport;
use esp_app::wifi::EspWifiManager;

fn main() -> anyhow::Result<()> {
  esp_idf_sys::link_patches();

  esp_idf_svc::log::EspLogger::initialize_default();

  let peripherals = Peripherals::take()
      .ok_or_else(|| anyhow!("Unable to take peripherals"))?;

  let event_loop = EspSystemEventLoop::take()?;

  info!("Initializing RS485 UART transport...");
  let transport = EspUartTransport::new(
      peripherals.uart1,
      peripherals.pins.gpio0,
      peripherals.pins.gpio1,
      Some(peripherals.pins.gpio9),
      None)?;

  info!("Initializing TFT display...");
  let tft_device = SpiDeviceDriver::new_single(
      peripherals.spi2,
      peripherals.pins.gpio6,
      peripherals.pins.gpio7,
      Option::<Gpio0>::None,
      Dma::Disabled,
      Option::<Gpio0>::None,
      &spi::config::Config::new()
          .baudrate(40.MHz().into())
          .data_mode(V02Type(MODE_0).into())
          .write_only(true)
  )?;
  let display_interface = SPIInterfaceNoCS::new(
      tft_device,
      PinDriver::output(peripherals.pins.gpio4)?);
  let mut display = Builder::ili9341_rgb565(display_interface)
      .with_orientation(Orientation::Landscape(false))
      .with_color_order(ColorOrder::Bgr)
      .init(&mut Ets, Option::<PinDriver<AnyOutputPin, Output>>::None)
      .unwrap();

  info!("Setting up app...");
  let lcd_device = TftAndMembraneSwitchDevice::new(
      display,
      MembraneSwitchWindowProxy::new(vec![
        (debounced(peripherals.pins.gpio2)?, Button::Up),
        (debounced(peripherals.pins.gpio3)?, Button::Down),
      ]),
      HalBacklightControl { pin: PinDriver::output(peripherals.pins.gpio5)? });

  let nvs = EspDefaultNvsPartition::take()?;
  let esp_wifi = EspWifiManager::new(
      peripherals.modem,
      event_loop,
      nvs,
      Advertisement::fake_balboa().name)?;

  let topside_app = TopsidePanelApp::new(
      transport,
      lcd_device,
      Some(esp_wifi));

  info!("Starting app...");
  let result = topside_app.run_loop();
  if let Err(e) = result {
    error!("Fatal error running topside panel: {e}");
  }

  panic!("main exit, rebooting...");
}

fn debounced(
    pin: impl Peripheral<P = impl esp_idf_hal::gpio::InputPin> + 'static,
) -> Result<DebouncedInputPin<PinDriver<'static, AnyInputPin, Input>, ActiveLow>, EspError> {
  let raw_input = PinDriver::input(pin.into_ref().map_into())?;
  Ok(DebouncedInputPin::new(raw_input, ActiveLow))
}

struct TftAndMembraneSwitchDevice<DISP, BUTTON: InputPin, BACKLIGHT> {
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

struct MembraneSwitchWindowProxy<I: InputPin, DISP> {
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

struct HalBacklightControl<O> {
  pin: O,
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
    if let Err(e) = self.pin.set_state(state) {
      warn!("Could not set backlight state: {e}");
    }
  }
}