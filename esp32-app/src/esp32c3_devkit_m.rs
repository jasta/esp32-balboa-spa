#[macro_export]
macro_rules! onboard_led {
  ($peripherals:ident) => {
    EspWs2812Driver::new(
        $peripherals.rmt.channel0,
        $peripherals.pins.gpio8)
  }
}

pub use onboard_led;
pub use crate::esp_ws2812_driver::EspWs2812Driver;