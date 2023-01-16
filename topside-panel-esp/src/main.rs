use std::intrinsics::unreachable;
use anyhow::anyhow;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::{gpio, uart};
use esp_idf_hal::delay::BLOCK;
use esp_idf_hal::gpio::PinDriver;
use esp_idf_hal::uart::config::{DataBits, StopBits};
use esp_idf_hal::uart::UartDriver;
use esp_idf_hal::units::Hertz;
use log::info;

fn main() -> anyhow::Result<()> {
  esp_idf_sys::link_patches();

  esp_idf_svc::log::EspLogger::initialize_default();

  let peripherals = Peripherals::take()
      .ok_or_else(|| anyhow!("Unable to take peripherals"))?;

  let uart = peripherals.uart2;
  let enable_pin = peripherals.pins.gpio13;
  let tx = peripherals.pins.gpio14;
  let rx = peripherals.pins.gpio27;

  let uart_config = uart::config::Config::new()
      .baudrate(Hertz(115_200))
      .data_bits(DataBits::DataBits8)
      .parity_none()
      .stop_bits(StopBits::STOP1);
  let cts = Option::<gpio::Gpio0>::None;
  let rts = Option::<gpio::Gpio0>::None;

  let mut enable_driver = PinDriver::output(enable_pin)?;
  enable_driver.set_low()?;

  let uart_driver = UartDriver::new(uart, tx, rx, cts, rts, &uart_config)?;

  let (tx, rx) = uart_driver.split();

  let mut buf = [0u8; 1];
  loop {
    if rx.read(&mut buf, BLOCK)? == 1 {
      let c = buf[0];
      info!("Got {c:02X}");
    }
  }
}
