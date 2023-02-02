use balboa_spa_messages::message_types::TemperatureRange;
use balboa_spa_messages::temperature::{ProtocolTemperature, TemperatureScale};
use measurements::Temperature;
use num_traits::cast::ToPrimitive;

const LOW_RANGE_C: [f64; 2] = [10.0, 36.0];
const HIGH_RANGE_C: [f64; 2] = [26.5, 40.0];

#[derive(Debug, Clone, PartialEq)]
pub struct TemperatureRangeModel {
  pub display: (TemperatureDisplay, TemperatureDisplay),
  range: TemperatureRange,
  scale: TemperatureScale,
}

impl TemperatureRangeModel {
  pub fn new(range: TemperatureRange, scale: TemperatureScale) -> Self {
    let values = match range {
      TemperatureRange::Low => LOW_RANGE_C,
      TemperatureRange::High => HIGH_RANGE_C,
    };

    let display_values = values.map(|v| {
      TemperatureModel::new(Temperature::from_celsius(v), scale).display
    });

    Self {
      display: (display_values[0], display_values[1]),
      range,
      scale,
    }
  }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TemperatureModel {
  pub display: TemperatureDisplay,
  temperature: Temperature,
  scale: TemperatureScale,
}

impl TemperatureModel {
  pub fn new(temperature: Temperature, scale: TemperatureScale) -> Self {
    Self {
      temperature,
      scale,
      display: TemperatureDisplay::new(temperature, scale),
    }
  }
}

impl From<ProtocolTemperature> for TemperatureModel {
  fn from(value: ProtocolTemperature) -> Self {
    TemperatureModel::new(value.temperature, value.raw_scale)
  }
}

/// Breaks down a temperature value into a nice UI-friendly display that lets us paint the
/// whole integer with a large/clear paint brush and the fractional remainder nicely rounded to 0.5
/// and painted smaller.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct TemperatureDisplay {
  pub big_part: u16,
  pub little_part: Option<u8>,

  /// Integer value that is sufficient to use a scale range on a meter or gauge widget.  For
  /// example, if the value is 26.5C, a suitable value for lvgl widgets would be 265.
  pub int_value: i32,
}

impl TemperatureDisplay {
  fn new(value: Temperature, scale: TemperatureScale) -> Self {
    let (big_part, little_part) = match scale {
      TemperatureScale::Fahrenheit => {
        let value = value.as_fahrenheit();
        (value.round().to_u16().unwrap(), None)
      },
      TemperatureScale::Celsius => {
        let value = value.as_celsius();
        let rounded = (value * 2.0).round() / 2.0;
        (
          rounded.trunc().to_u16().unwrap(),
          Some(rounded.fract().to_u8().unwrap())
        )
      }
    };
    let int_value = big_part * 10 + u16::from(little_part.unwrap_or(0));
    Self {
      big_part,
      little_part,
      int_value: i32::from(int_value),
    }
  }
}
