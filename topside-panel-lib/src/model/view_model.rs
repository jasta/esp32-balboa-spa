use measurements::Temperature;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::time::Instant;
use balboa_spa_messages::message_types::TemperatureRange;
use balboa_spa_messages::temperature::{ProtocolTemperature, TemperatureScale};
use wifi_module_lib::wifi_module_client::WifiModuleClient;
use crate::model::temperature_model::{TemperatureModel, TemperatureRangeModel};

#[derive(Debug, Clone, PartialEq)]
pub struct ViewModel {
  pub conn_state: ConnectionState,
  pub last_model: Option<HotTubModel>,
  pub wifi_model: Option<wifi_module_lib::view_model::ViewModel>,
}

impl Default for ViewModel {
  fn default() -> Self {
    Self {
      conn_state: ConnectionState::WaitingForPeer,
      last_model: None,
      wifi_model: None,
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
  pub current_temp: Option<TemperatureModel>,
  pub set_temp: TemperatureModel,
  pub is_heating: bool,
  pub temp_range: TemperatureRangeModel,
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
