use std::io::{Read, Write};
use std::thread;
use std::time::Duration;
use anyhow::anyhow;
use common_lib::transport::Transport;
use debounced_pin::{ActiveLow, Debounce, DebouncedInputPin, DebounceState};
use display_interface_spi::SPIInterfaceNoCS;
use embedded_hal::digital::v2::{InputPin, OutputPin, PinState};
use embedded_hal::spi::MODE_0;
use esp_idf_hal::delay::Ets;
use esp_idf_hal::gpio::{AnyInputPin, AnyIOPin, AnyOutputPin, Gpio0, Input, IOPin, Output, PinDriver, Pull};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::spi;
use esp_idf_hal::spi::config::V02Type;
use esp_idf_hal::spi::Dma;
use esp_idf_hal::spi::SpiDeviceDriver;
use esp_idf_hal::units::FromValueType;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::log::EspLogger;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_sys::esp_app_desc;
use log::{error, info, LevelFilter};
use mipidsi::{Builder, ColorOrder, Orientation};
use topside_panel_lib::app::topside_panel_app::TopsidePanelApp;
use topside_panel_lib::model::key_event::Key;
use topside_panel_lib::view::lcd_device::{BacklightBrightness, BacklightControl};
use wifi_module_lib::advertisement::Advertisement;
use esp_app::backlight_control::HalBacklightControl;
use esp_app::esp_status_printer::EspStatusPrinter;
use esp_app::esp_uart_transport::EspUartTransport;
use esp_app::membrane_switch;
use esp_app::membrane_switch::MembraneSwitchWindowProxy;
use esp_app::ui_device::{EtsUiDelay, FreeRtosDelay, TftAndMembraneSwitchDevice};
use esp_app::wifi::EspWifiManager;

esp_app_desc!();

static LOGGER: EspLogger = EspLogger;

fn main() -> anyhow::Result<()> {
  esp_idf_sys::link_patches();

  // EspLogger::initialize_default();
  LOGGER.initialize();
  LOGGER.set_target_level("spi_master", LevelFilter::Info);
  log::set_logger(&LOGGER).unwrap();

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
      None::<Gpio0>,
      Dma::Disabled,
      None::<Gpio0>,
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
      .init(&mut Ets, Some(PinDriver::output(peripherals.pins.gpio18)?))
      .unwrap();

  info!("Setting up app...");
  let backlight_control = HalBacklightControl::new(PinDriver::output(peripherals.pins.gpio5)?);
  let lcd_device = TftAndMembraneSwitchDevice::new(
      display,
      MembraneSwitchWindowProxy::new(vec![
        (membrane_switch::debounced(peripherals.pins.gpio2.downgrade())?, Key::Up),
        (membrane_switch::debounced(peripherals.pins.gpio3.downgrade())?, Key::Down),
        (membrane_switch::debounced(peripherals.pins.gpio10.downgrade())?, Key::Jets1),
        (membrane_switch::debounced(peripherals.pins.gpio8.downgrade())?, Key::Light),
      ]),
      backlight_control);

  // let nvs = EspDefaultNvsPartition::take()?;
  // let esp_wifi = EspWifiManager::new(
  //     peripherals.modem,
  //     event_loop,
  //     nvs,
  //     Advertisement::fake_balboa().name)?;

  let topside_app = TopsidePanelApp::new(
      transport,
      lcd_device,
      None::<EspWifiManager>,
      FreeRtosDelay,
      Some(EspStatusPrinter));

  info!("Starting app...");
  if let Err(e) = topside_app.run_loop() {
    error!("Fatal error running topside panel: {e}");
  }

  panic!("main exit, rebooting...");
}
