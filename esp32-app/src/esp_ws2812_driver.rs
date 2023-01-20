use esp_idf_hal::gpio::OutputPin;
use esp_idf_hal::peripheral::Peripheral;
use esp_idf_hal::rmt::RmtChannel;
use smart_leds::SmartLedsWrite;
use ws2812_esp32_rmt_driver::{RGB8, Ws2812Esp32Rmt, Ws2812Esp32RmtDriverError};

pub struct EspWs2812Driver {
  inner: Ws2812Esp32Rmt,
}

impl EspWs2812Driver {
  pub fn new<RMT_CHANNEL: RmtChannel>(
    _rmt: impl Peripheral<P = RMT_CHANNEL> + 'static,
    pin: impl Peripheral<P = impl OutputPin> + 'static
  ) -> Result<Self, Ws2812Esp32RmtDriverError> {
    let rmt_u8 = u8::try_from(RMT_CHANNEL::channel()).unwrap();
    let pin_u32 = pin.into_ref().pin().try_into().unwrap();
    Ok(Self {
      inner: Ws2812Esp32Rmt::new(rmt_u8, pin_u32)?,
    })
  }

  pub fn into_inner(self) -> Ws2812Esp32Rmt {
    self.inner
  }
}