use std::io::{BufRead, ErrorKind};
use std::ops::Deref;
use std::sync::{Arc, Mutex};

use esp_idf_hal::delay::BLOCK;
use esp_idf_hal::gpio::{AnyOutputPin, Gpio3, InputPin, Output, OutputPin, PinDriver, Pull};
use esp_idf_hal::peripheral::{Peripheral};
use esp_idf_hal::prelude::*;
use esp_idf_hal::uart::{Uart, UartDriver, UartRxDriver, UartTxDriver};
use esp_idf_hal::{gpio, uart};
use esp_idf_hal::uart::config::{DataBits, StopBits};
use esp_idf_sys::{ESP_ERR_TIMEOUT, EspError};
use log::debug;
use nb::block;

use balboa_spa_protocol::transport::Transport;

pub struct EspUartTransport {
  uart_driver: UartDriver<'static>,
  enable_driver: Option<PinDriver<'static, AnyOutputPin, Output>>
}

pub struct EspUartRx {
  rx_driver: UartRxDriver<'static>
}

pub struct EspUartTx {
  tx_driver: UartTxDriver<'static>,
  enable_driver: Option<PinDriver<'static, AnyOutputPin, Output>>,
  writing: bool,
}

impl EspUartTransport {
  /// Create a new transport adapter using the ESP's hardware UART with an optional pin
  /// dedicated to setting the DE and RE pins HIGH or LOW for transmit and receive respectively
  /// (which is only required for some RS485 modules)
  pub fn new(
      uart: impl Peripheral<P = impl Uart> + 'static,
      tx: impl Peripheral<P = impl OutputPin> + 'static,
      rx: impl Peripheral<P = impl InputPin> + 'static,
      enable_pin: Option<impl Peripheral<P = impl OutputPin> + 'static>,
  ) -> Result<Self, EspError> {
    let uart_config = uart::config::Config::new()
        .baudrate(Hertz(115_200))
        .data_bits(DataBits::DataBits8)
        .parity_none()
        .stop_bits(StopBits::STOP1);
    let cts = Option::<gpio::Gpio0>::None;
    let rts = Option::<gpio::Gpio0>::None;
    let enable_driver = match enable_pin {
      Some(pin) => {
        let mut driver = PinDriver::output(pin.into_ref().map_into())?;
        driver.set_low()?;
        Some(driver)
      },
      None => None,
    };
    Ok(EspUartTransport {
      uart_driver: UartDriver::new(uart, tx, rx, cts, rts, &uart_config)?,
      enable_driver,
    })
  }
}

impl Transport<EspUartRx, EspUartTx> for EspUartTransport {
  fn split(self) -> (EspUartRx, EspUartTx) {
    let (tx, rx) = self.uart_driver.into_split();
    (
      EspUartRx { rx_driver: rx },
      EspUartTx { tx_driver: tx, enable_driver: self.enable_driver, writing: false }
    )
  }
}

impl std::io::Read for EspUartRx {
  fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
    self.rx_driver.read(buf, BLOCK)
        .map_err(err_to_std)
  }
}

impl std::io::Write for EspUartTx {
  fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
    if let Some(driver) = &mut self.enable_driver {
      if !self.writing {
        self.writing = true;
        driver.set_high().map_err(err_to_std)?;
      }
    }
    block!(self.tx_driver.write(buf).map_err(err_to_nb_std))
  }

  fn flush(&mut self) -> std::io::Result<()> {
    block!(self.tx_driver.flush().map_err(err_to_nb_std))?;
    if let Some(driver) = &mut self.enable_driver {
      self.writing = false;
      driver.set_low().map_err(err_to_std)?;
    }
    Ok(())
  }
}

fn err_to_nb_std(e: EspError) -> nb::Error<std::io::Error> {
  match e.code() {
    ESP_ERR_TIMEOUT => nb::Error::WouldBlock,
    _ => nb::Error::Other(err_to_std(e)),
  }
}

fn err_to_std(e: EspError) -> std::io::Error {
  std::io::Error::new(ErrorKind::Other, e)
}