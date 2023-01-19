use std::cmp::{max, min};
use std::io::{BufRead, ErrorKind};
use std::ops::Deref;
use std::sync::{Arc, Mutex};

use esp_idf_hal::delay::{BLOCK, NON_BLOCK};
use esp_idf_hal::gpio::{AnyOutputPin, Gpio3, InputPin, Output, OutputPin, PinDriver, Pull};
use esp_idf_hal::peripheral::{Peripheral};
use esp_idf_hal::prelude::*;
use esp_idf_hal::uart::{Uart, UartDriver, UartRxDriver, UartTxDriver};
use esp_idf_hal::{gpio, uart};
use esp_idf_hal::uart::config::{DataBits, StopBits};
use esp_idf_sys::{ESP_ERR_TIMEOUT, EspError};
use log::debug;
use nb::block;

use mock_mainboard_lib::transport::Transport;

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
    if buf.is_empty() {
      return Ok(0)
    }

    // uart_read_bytes is implemented kinda funny such that if there is data available in the
    // buffer it won't just return a short read but will instead always try to read up to the
    // provided length (i.e. `buf.len()` in our case).  To combat this we'll read either
    // the amount of data in the buffer _or_ just a single byte then drain the full buffer after.
    let available = self.rx_driver.count().map_err(err_to_std)?;
    if available == 0 {
      let n = block!(rw_to_nb_std(self.rx_driver.read(&mut buf[0..1], BLOCK)))?;
      assert_eq!(n, 1);

      // Now let's try again with the RX buffer.
      self.read_with_rx_buffer(&mut buf[1..]).map_err(err_to_std)
    } else {
      self.read_with_rx_buffer(buf).map_err(err_to_std)
    }
  }
}

impl EspUartRx {
  /// Perform a UART read but _only_ take bytes from the RX buffer (i.e. do not wait
  /// for more data to become available and return immediately if none are).
  fn read_with_rx_buffer(&mut self, buf: &mut [u8]) -> Result<usize, EspError> {
    let available = self.rx_driver.count()?;
    if available == 0 {
      Ok(0)
    } else {
      let max_len = min(available, buf.len());
      match self.rx_driver.read(&mut buf[0..max_len], NON_BLOCK)? {
        0 => panic!("Concurrent UART read detected!"),
        n => Ok(n),
      }
    }
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
    block!(rw_to_nb_std(self.tx_driver.write(buf)))
  }

  fn flush(&mut self) -> std::io::Result<()> {
    block!(flush_to_nb_std(self.tx_driver.flush()))?;
    if let Some(driver) = &mut self.enable_driver {
      self.writing = false;
      driver.set_low().map_err(err_to_std)?;
    }
    Ok(())
  }
}

fn rw_to_nb_std(result: Result<usize, EspError>) -> nb::Result<usize, std::io::Error> {
  match result {
    Ok(0) => Err(nb::Error::WouldBlock),
    Ok(n) => Ok(n),
    Err(e) => Err(nb::Error::Other(err_to_std(e))),
  }
}

fn flush_to_nb_std(result: Result<(), EspError>) -> nb::Result<(), std::io::Error> {
  match result {
    Ok(r) => Ok(r),
    Err(e) if e.code() == ESP_ERR_TIMEOUT => Err(nb::Error::WouldBlock),
    Err(e) => Err(nb::Error::Other(err_to_std(e))),
  }
}

fn err_to_std(e: EspError) -> std::io::Error {
  std::io::Error::new(ErrorKind::Other, e)
}
