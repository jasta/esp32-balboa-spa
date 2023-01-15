use std::ops::Deref;
use std::thread;
use std::time::Duration;

use anyhow::anyhow;
use esp_idf_hal::gpio::{Output, OutputPin, Pin, PinDriver, RTCPin};
use esp_idf_hal::prelude::*;
use esp_idf_svc::eventloop::EspEventLoop;
use esp_idf_sys as _;
use log::info;
use balboa_spa_protocol::main_board::MainBoard;
use crate::esp_uart_transport::{EspUartTransport};

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

  info!("UART transport initialized");

  let logic = MainBoard::new(transport);
  let (shutdown_handle, runner) = logic.into_runner();

  info!("Main board setup complete, starting...");
  runner.run_loop()?;
  info!("Exiting seemingly by request, though not sure how?");

  drop(shutdown_handle);

  Ok(())
}
