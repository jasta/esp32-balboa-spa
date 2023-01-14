use std::io::ErrorKind;
use std::marker::PhantomData;
use std::thread;
use std::time::Duration;

use anyhow::anyhow;
use embedded_hal::prelude::_embedded_hal_serial_Read;
use esp_idf_hal::delay::{BLOCK, NON_BLOCK};
use esp_idf_hal::prelude::*;
use esp_idf_hal::uart;
use esp_idf_hal::uart::config::{DataBits, Parity, StopBits};
use esp_idf_hal::uart::{SerialError, UartDriver, UartRxDriver, UartTxDriver};
use esp_idf_svc::eventloop::EspEventLoop;
use esp_idf_sys as _;
use esp_idf_sys::EspError;
use nb::block;
use balboa_spa_protocol::main_board::MainBoard;
use balboa_spa_protocol::transport::Transport;
use crate::uart_transport::EspUartTransport;

mod wifi;
mod uart_transport;

fn main() -> anyhow::Result<()> {
  esp_idf_sys::link_patches();

  esp_idf_svc::log::EspLogger::initialize_default();

  let peripherals = Peripherals::take()
      .ok_or_else(|| anyhow!("Unable to take peripherals"))?;
  let event_loop = EspEventLoop::take()?;

  let transport = EspUartTransport::new(
      peripherals.uart0,
      peripherals.pins.gpio21,
      peripherals.pins.gpio20)?;

  let mut logic = MainBoard::new(transport);

  for tick in 0.. {
    println!("Tick #{tick}...");
    thread::sleep(Duration::from_millis(1000));
  }

  Ok(())
}
