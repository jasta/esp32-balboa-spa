use measurements::Temperature;
use std::collections::HashMap;
use std::time::Instant;
use crate::topside_state_machine::TopsideContext;

#[derive(Debug, Clone, PartialEq)]
pub struct ViewModel {
  pub conn_state: ConnectionState,
  pub last_model: Option<HotTubModel>,
}

impl Default for ViewModel {
  fn default() -> Self {
    Self {
      conn_state: ConnectionState::WaitingForPeer,
      last_model: None,
    }
  }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
  WaitingForPeer,
  Negotiating,
  Negotiated,
  Idle,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HotTubModel {
  pub received_at: Instant,
  pub current_temp: Option<Temperature>,
  pub set_temp: Temperature,
  pub is_heating: bool,
  pub devices: HashMap<DeviceCategory, Vec<DeviceModel>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceModel {
  pub category: DeviceCategory,
  pub current_level: DeviceLevel,
  pub available_levels: Vec<DeviceLevel>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DeviceCategory {
  Jet,
  Light,
  Aux,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum DeviceLevel {
  Off,
  PartialOn,
  FullOn,
}
