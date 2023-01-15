use std::thread;
use std::time::Duration;

use anyhow::anyhow;
use esp_idf_hal::gpio::{OutputPin, PinDriver, RTCPin};
use esp_idf_hal::peripheral::Peripheral;
use esp_idf_hal::prelude::*;
use esp_idf_svc::eventloop::EspEventLoop;
use esp_idf_sys as _;
use balboa_spa_protocol::main_board::MainBoard;
use crate::esp_uart_transport::{EspUartRx, EspUartTransport, EspUartTx};

mod wifi;
mod esp_uart_transport;

fn main() -> anyhow::Result<()> {
  esp_idf_sys::link_patches();

  esp_idf_svc::log::EspLogger::initialize_default();

  let peripherals = Peripherals::take()
      .ok_or_else(|| anyhow!("Unable to take peripherals"))?;
  let event_loop = EspEventLoop::take()?;

  let transport = EspUartTransport::new(
      peripherals.uart0,
      peripherals.pins.gpio21,
      peripherals.pins.gpio20,
      Some(peripherals.pins.gpio3))?;

  let logic = MainBoard::new(transport);

  let (_, runner) = logic.into_runner();
  runner.run_loop()?;

  Ok(())
}
