use std::io::ErrorKind;

use esp_idf_hal::delay::BLOCK;
use esp_idf_hal::gpio::{AnyOutputPin, InputPin, Output, OutputPin, PinDriver};
use esp_idf_hal::peripheral::Peripheral;
use esp_idf_hal::prelude::*;
use esp_idf_hal::uart::{Uart, UartDriver, UartRxDriver, UartTxDriver};
use esp_idf_hal::{gpio, uart};
use esp_idf_hal::uart::config::{DataBits, StopBits};
use esp_idf_sys::EspError;

use balboa_spa_protocol::transport::Transport;

pub struct EspUartTransport<'d> {
  driver: UartDriver<'d>,
  enable_driver: Option<AnyOutputPin>
}

pub struct EspUartRx<'d> {
  driver: UartRxDriver<'d>
}

pub struct EspUartTx<'d> {
  driver: UartTxDriver<'d>,
  enable_driver: Option<PinDriver<'d, AnyOutputPin, Output>>,
}

impl <'d> EspUartTransport<'d>
{
  /// Create a new transport adapter using the ESP's hardware UART with an optional pin
  /// dedicated to setting the DE and RE pins HIGH or LOW for transmit and receive respectively
  /// (which is only required for some RS485 modules)
  pub fn new(
      uart: impl Peripheral<P = impl Uart> + 'd,
      tx: impl Peripheral<P = impl OutputPin> + 'd,
      rx: impl Peripheral<P = impl InputPin> + 'd,
      enable_pin: Option<impl Peripheral<P = impl OutputPin> + 'd>,
  ) -> Result<Self, EspError> {
    let uart_config = uart::config::Config::new()
        .baudrate(Hertz(115_200))
        .data_bits(DataBits::DataBits8)
        .parity_none()
        .stop_bits(StopBits::STOP1);
    let cts = Option::<gpio::Gpio0>::None;
    let rts = Option::<gpio::Gpio0>::None;
    let enable_driver = if let Some(pin) = enable_pin {
      Some(PinDriver::output(pin.into_ref().downgrade_output())?)
    } else {
      None
    };
    Ok(EspUartTransport {
      driver: UartDriver::new(uart, tx, rx, cts, rts, &uart_config)?,
      enable_driver,
    })
  }
}

impl <'d> Transport<EspUartRx<'d>, EspUartTx<'d>> for EspUartTransport<'d> {
  fn split(self) -> (EspUartRx<'d>, EspUartTx<'d>) {
    let (tx, rx) = self.driver.into_split();
    (
      EspUartRx { driver: rx },
      EspUartTx { driver: tx, enable_driver: self.enable_driver }
    )
  }
}

impl <'d> std::io::Read for EspUartRx<'d> {
  fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
    self.driver.read(buf, BLOCK)
        .map_err(map_esp_err)
    }
  }

impl <'d> std::io::Write for EspUartTx<'d> {
  fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
    if let Some(driver) = &mut self.enable_driver {
      driver.set_high()
          .map_err(map_esp_err)?;
    }
    self.driver.write(buf)
        .map_err(map_esp_err)
  }

  fn flush(&mut self) -> std::io::Result<()> {
    self.driver.flush()
        .map_err(map_esp_err)?;
    if let Some(driver) = &mut self.enable_driver {
      driver.set_low()
          .map_err(map_esp_err)?;
    }
    Ok(())
  }
}

fn map_esp_err(e: EspError) -> std::io::Error {
  std::io::Error::new(ErrorKind::Other, e)
}