use anyhow::anyhow;
pub use measurements::Temperature;
use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::FromPrimitive;
use packed_struct::prelude::*;

const FAHRENHEIT_SCALE: f64 = 1.0;
const CELSIUS_SCALE: f64 = 0.5;

#[derive(Debug, Clone)]
pub struct ProtocolTemperature {
  pub raw_scale: TemperatureScale,
  pub(crate) raw_value: u8,
  pub temperature: Temperature,
}

#[derive(FromPrimitive, ToPrimitive, PrimitiveEnum_u8, Debug, Copy, Clone)]
pub enum TemperatureScale {
  Fahrenheit = 0,
  Celsius = 1,
}

impl TemperatureScale {
  pub fn new_set_temperature(&self, target: &Temperature) -> anyhow::Result<SetTemperature> {
    let raw_target = match self {
      TemperatureScale::Fahrenheit => target.as_fahrenheit() / FAHRENHEIT_SCALE,
      TemperatureScale::Celsius => target.as_celsius() / CELSIUS_SCALE,
    };
    let scaled_target = u8::from_f64(raw_target.round())
        .ok_or_else(|| anyhow!("Cannot scale {raw_target}"))?;
    Ok(SetTemperature { raw_value: scaled_target })
  }

  pub fn new_protocol_temperature_from_raw(&self, raw_value: u8) -> ProtocolTemperature {
    let raw_value_f = f64::from(raw_value);
    let temperature = match self {
      TemperatureScale::Fahrenheit => Temperature::from_fahrenheit(raw_value_f * FAHRENHEIT_SCALE),
      TemperatureScale::Celsius => Temperature::from_celsius(raw_value_f * CELSIUS_SCALE),
    };
    ProtocolTemperature {
      raw_scale: *self,
      raw_value,
      temperature,
    }
  }

  pub fn new_protocol_temperature(&self, target: Temperature) -> anyhow::Result<ProtocolTemperature> {
    let set_temp = self.new_set_temperature(&target)?;
    Ok(ProtocolTemperature {
      raw_scale: *self,
      raw_value: set_temp.raw_value,
      temperature: target,
    })
  }
}

#[derive(Debug, Clone)]
pub struct SetTemperature {
  pub(crate) raw_value: u8,
}
