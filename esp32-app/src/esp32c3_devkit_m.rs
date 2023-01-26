#[macro_export]
macro_rules! onboard_led {
  ($peripherals:ident) => {
    EspWs2812Driver::new(
        $peripherals.rmt.channel0,
        $peripherals.pins.gpio8)
  }
}

#[macro_export]
macro_rules! spi2_tft_driver {
  ($peripherals:ident) => {
    spi::SpiDriver::new(
        $peripherals.spi2,
        $peripherals.pins.gpio6,
        $peripherals.pins.gpio7,
        Option::<Gpio0>::None,
        Dma::Disabled)
  }
}

pub use onboard_led;
pub use spi2_tft_driver;
pub use crate::esp_ws2812_driver::EspWs2812Driver;
pub use esp_idf_hal;