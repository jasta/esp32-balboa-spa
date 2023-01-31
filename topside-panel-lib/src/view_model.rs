use measurements::Temperature;
use std::collections::HashMap;

pub struct ViewModel {
  pub state: ConnectionState,
}

pub enum ConnectionState {
  Negotiating,
  Negotiated,
  Engaged(HotTubModel),
}

pub struct HotTubModel {
  pub current_temp: Temperature,
  pub set_temp: Temperature,
  pub devices: HashMap<DeviceCategory, Vec<DeviceModel>>,
}

pub struct DeviceModel {
  pub category: DeviceCategory,
  pub current_level: DeviceLevel,
  pub available_levels: Vec<DeviceLevel>,
}

pub enum DeviceCategory {
  Jet,
  Light,
  Aux,
}

pub enum DeviceLevel {
  Off,
  Low,
  High,
}
