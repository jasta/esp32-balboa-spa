use esp_idf_hal::uart::{UartDriver, UartRxDriver, UartTxDriver};
use esp_idf_hal::delay::BLOCK;
use std::io::ErrorKind;
use esp_idf_hal::gpio::{InputPin, OutputPin};
use esp_idf_hal::peripheral::Peripheral;
use esp_idf_hal::prelude::*;
use esp_idf_hal::uart;
use esp_idf_hal::uart::config::{DataBits, Parity, StopBits};
use esp_idf_sys::EspError;
use balboa_spa_protocol::transport::Transport;

pub struct EspUartTransport<'d> {
  driver: UartDriver<'d>,
}

pub struct EspUartRx<'d> {
  driver: UartRxDriver<'d>
}

pub struct EspUartTx<'d> {
  driver: UartTxDriver<'d>
}

impl <'d> EspUartTransport {
  pub fn new(
      uart: impl Peripheral<P = UART> + 'd,
      tx: impl Peripheral<P = impl OutputPin> + 'd,
      rx: impl Peripheral<P = impl InputPin> + 'd,
  ) -> Result<Self, EspError> {
    let uart_config = uart::config::Config::new()
        .baudrate(Hertz(115_200))
        .data_bits(DataBits::DataBits8)
        .parity(Parity::ParityNone)
        .stop_bits(StopBits::STOP1);
    Ok(EspUartTransport {
      driver: UartDriver::new(uart, tx, rx, None, None, &uart_config)?
    })
  }
}

impl Transport<EspUartRx, EspUartTx> for EspUartTransport {
  fn split(self) -> (EspUartRx, EspUartTx) {
    (self.rx, self.tx)
  }
}

impl std::io::Read for EspUartRx {
  fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
    self.driver.read(buf, BLOCK)
        .map_err(|e| std::io::Error::new(ErrorKind::Other, e))
    }
  }

impl std::io::Write for EspUartTx {
  fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
    self.driver.write(buf)
        .map_err(|e| std::io::Error::new(ErrorKind::Other, e))
  }

  fn flush(&mut self) -> std::io::Result<()> {
    self.driver.flush()
        .map_err(|e| std::io::Error::new(ErrorKind::Other, e))
  }
}
