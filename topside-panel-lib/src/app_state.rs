use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::time::Instant;
use balboa_spa_messages::message_types::{Boolean, ConfigurationResponseMessage, HeatingState, PumpConfig, PumpStatus, RelayStatus, StatusUpdateMessage, StatusUpdateResponseV1};
use crate::topside_state_machine::{TopsideStateKind, TopsideStateMachine};
use crate::cts_state_machine::{CtsStateKind, CtsStateMachine};
use crate::view_model::{ConnectionState, DeviceCategory, DeviceLevel, DeviceModel, HotTubModel, ViewModel};

#[derive(Default, Debug)]
pub(crate) struct AppState {
  pub cts_state_machine: CtsStateMachine,
  pub topside_state_machine: TopsideStateMachine,
}

impl AppState {
  pub fn fast_snapshot(&self) -> FastSnapshot {
    FastSnapshot {
      cts_state: self.cts_state_machine.state_kind(),
      topside_state: self.topside_state_machine.state_kind(),
    }
  }

  pub fn generate_view_model(&self) -> ViewModel {
    let conn_state = self.generate_conn_state();
    let last_model = self.generate_hot_tub_model();
    ViewModel {
      conn_state,
      last_model,
    }
  }

  fn generate_conn_state(&self) -> ConnectionState {
    match self.cts_state_machine.state_kind() {
      CtsStateKind::WaitingForNewClientCTS => ConnectionState::WaitingForPeer,
      CtsStateKind::WaitingForChannelAssignment => ConnectionState::Negotiating,
      CtsStateKind::ChannelAssigned => {
        match self.topside_state_machine.state_kind() {
          TopsideStateKind::ReadingStatus => ConnectionState::Idle,
          _ => ConnectionState::Negotiating,
        }
      },
    }
  }

  fn generate_hot_tub_model(&self) -> Option<HotTubModel> {
    let info = &self.topside_state_machine.context.info;
    let config = &self.topside_state_machine.context.config;
    let prefs = &self.topside_state_machine.context.prefs;
    let status = &self.topside_state_machine.context.status;

    if let Some(info) = info {
      if let Some(config) = config {
        if let Some(prefs) = prefs {
          if let Some(status) = status {
            let status_v1 = &status.message.v1;
            let current_temp = status_v1.current_temperature
                .clone()
                .map(|t| t.temperature);
            let set_temp = status_v1.set_temperature.clone().temperature;
            let heating_state = status_v1.heating_state.as_ref()
                .unwrap_or(&HeatingState::Off);
            let is_heating = match heating_state {
              HeatingState::Off => false,
              HeatingState::Heating => true,
              HeatingState::HeatWaiting => false,
            };
            let devices = DeviceMapper::convert(config, status_v1);
            let model = HotTubModel {
              received_at: status.received_at,
              current_temp,
              set_temp,
              is_heating,
              devices,
            };
            return Some(model);
          }
        }
      }
    }
    None
  }
}

#[derive(Debug, PartialEq)]
pub struct FastSnapshot {
  cts_state: CtsStateKind,
  topside_state: TopsideStateKind,
}

struct DeviceMapper;
impl DeviceMapper {
  pub fn convert(config: &ConfigurationResponseMessage, status: &StatusUpdateResponseV1) -> HashMap<DeviceCategory, Vec<DeviceModel>> {
    let mut out = HashMap::new();

    let jets_zipped = config.pumps.iter()
        .zip(&status.pump_status);
    let jets: Vec<_> = jets_zipped
        .filter_map(|(c, s)| {
          Self::convert_pump(c.as_ref(), s.as_ref())
        })
        .collect();
    out.insert(DeviceCategory::Jet, jets);

    let lights_zipped = config.has_lights.iter()
        .zip(&status.light_status);
    let lights: Vec<_> = lights_zipped
        .filter_map(|(c, s)| {
          Self::convert_relay(DeviceCategory::Light, c.as_ref(), s.as_ref())
        })
        .collect();
    out.insert(DeviceCategory::Light, lights);

    out
  }

  fn convert_relay(
      category: DeviceCategory,
      config: Option<&Boolean>,
      status: Option<&RelayStatus>
  ) -> Option<DeviceModel> {
    if !bool::from(config.unwrap_or(&Boolean::False)) {
      return None;
    }

    match status {
      Some(s) => {
        let current_level = match s {
          RelayStatus::Off => DeviceLevel::Off,
          RelayStatus::On => DeviceLevel::FullOn,
        };
        Some(DeviceModel {
          category,
          available_levels: vec![current_level],
          current_level,
        })
      }
      None => None,
    }
  }

  fn convert_pump(config: Option<&PumpConfig>, status: Option<&PumpStatus>) -> Option<DeviceModel> {
    let level = config.unwrap_or(&PumpConfig::None);
    let available_levels = match level {
      PumpConfig::None => return None,
      PumpConfig::Speed1 => vec![DeviceLevel::Off, DeviceLevel::FullOn],
      PumpConfig::Speed2 => vec![DeviceLevel::Off, DeviceLevel::PartialOn, DeviceLevel::FullOn],
    };
    match status {
      Some(s) => {
        let current_level = match s {
          PumpStatus::Off => DeviceLevel::Off,
          PumpStatus::Low => DeviceLevel::PartialOn,
          PumpStatus::High => DeviceLevel::FullOn,
        };
        Some(DeviceModel {
          category: DeviceCategory::Jet,
          available_levels,
          current_level,
        })
      }
      None => None,
    }
  }
}