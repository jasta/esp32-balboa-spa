use anyhow::anyhow;
use measurements::Temperature;
use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::FromPrimitive;

#[derive(Debug, Clone)]
pub struct ProtocolTemperature {
  pub raw_scale: TemperatureScale,
  raw_value: u8,
  pub temperature: Temperature,
}

const FAHRENHEIT_SCALE: f64 = 1.0;
const CELSIUS_SCALE: f64 = 0.5;

#[derive(FromPrimitive, ToPrimitive, Debug, Clone)]
pub enum TemperatureScale {
  Fahrenheit = 0,
  Celsius = 1,
}

impl TemperatureScale {
  pub fn new_set_temperature(&self, target: Temperature) -> anyhow::Result<SetTemperature> {
    let raw_target = match self {
      TemperatureScale::Fahrenheit => target.as_fahrenheit() / FAHRENHEIT_SCALE,
      TemperatureScale::Celsius => target.as_celsius() / CELSIUS_SCALE,
    };
    let scaled_target = u8::from_f64(raw_target.round())
        .ok_or_else(|| anyhow!("Cannot scale {raw_target}"))?;
    Ok(SetTemperature { raw_value: scaled_target })
  }
}

#[derive(Debug, Clone)]
pub struct SetTemperature {
  raw_value: u8,
}