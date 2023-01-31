use measurements::Temperature;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ViewModel {
  pub state: ConnectionState,
  pub last_model: Option<HotTubModel>,
}

impl Default for ViewModel {
  fn default() -> Self {
    Self {
      state: ConnectionState::WaitingForPeer,
      last_model: None,
    }
  }
}

#[derive(Debug, Clone)]
pub enum ConnectionState {
  WaitingForPeer,
  Negotiating,
  Negotiated,
  Engaged,
}

#[derive(Debug, Clone)]
pub struct HotTubModel {
  pub current_temp: Temperature,
  pub set_temp: Temperature,
  pub devices: HashMap<DeviceCategory, Vec<DeviceModel>>,
}

#[derive(Debug, Clone)]
pub struct DeviceModel {
  pub category: DeviceCategory,
  pub current_level: DeviceLevel,
  pub available_levels: Vec<DeviceLevel>,
}

#[derive(Debug, Clone)]
pub enum DeviceCategory {
  Jet,
  Light,
  Aux,
}

#[derive(Debug, Clone)]
pub enum DeviceLevel {
  Off,
  Low,
  High,
}
