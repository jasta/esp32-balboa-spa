use chrono::{Timelike, Utc};
use balboa_spa_messages::message_types::{Boolean, ClockMode, ConfigurationResponseMessage, FaultResponseMessage, FilterMode, HeatingMode, HeatingState, InitializationMode, PumpConfig, PumpStatus, RelayStatus, ReminderType, SpaState, StatusUpdateMessage, StatusUpdateResponseV1, TemperatureRange};
use balboa_spa_messages::parsed_enum::ParsedEnum;
use balboa_spa_messages::temperature::{ProtocolTemperature, SetTemperature, Temperature, TemperatureScale};
use balboa_spa_messages::time::ProtocolTime;

#[derive(Debug)]
pub struct MockSpa {
  pub run_state: MockSpaState,
  pub hardware: MockHardware,
  pub settings: UserSettings,
}

impl Default for MockSpa {
  fn default() -> Self {
    Self {
      run_state: MockSpaState::Initializing,
      hardware: MockHardware {
        pumps: vec![PumpDevice::default()],
        blower: RelayDevice::default(),
        lights: vec![RelayDevice::default()],
      },
      settings: UserSettings {
        temp_range: TemperatureRange::High,
        clock_mode: ClockMode::Hour12,
        temperature_scale: TemperatureScale::Celsius,
        set_temperature: Temperature::from_celsius(39.5),
      }
    }
  }
}

#[derive(Debug)]
pub enum MockSpaState {
  Initializing,
  Heating,
  Hold,
}

#[derive(Debug)]
pub struct MockHardware {
  pub pumps: Vec<PumpDevice>,
  pub blower: RelayDevice,
  pub lights: Vec<RelayDevice>,
}

#[derive(Debug)]
pub struct PumpDevice {
  pub status: PumpStatus,
  pub capability: PumpConfig,
}

impl Default for PumpDevice {
  fn default() -> Self {
    Self {
      status: PumpStatus::Off,
      capability: PumpConfig::Speed2,
    }
  }
}

#[derive(Debug)]
pub struct RelayDevice {
  pub status: RelayStatus,
}

impl Default for RelayDevice {
  fn default() -> Self {
    Self { status: RelayStatus::Off }
  }
}

#[derive(Debug)]
pub struct UserSettings {
  temp_range: TemperatureRange,
  clock_mode: ClockMode,
  temperature_scale: TemperatureScale,
  set_temperature: Temperature,
}

impl MockSpa {
  pub fn new() -> Self {
    Default::default()
  }

  pub fn as_status(&self) -> StatusUpdateMessage {
    let run_status = self.run_state.as_status();
    let hw_status = self.hardware.as_status();
    let user_status = self.settings.as_status();

    let current_temperature = match run_status.current_temperature {
      CurrentTemperatureState::Unknown => None,
      CurrentTemperatureState::Low => {
        Some(user_status.temperature_scale
            .new_protocol_temperature(Temperature::from_celsius(20.0)).unwrap())
      },
      CurrentTemperatureState::AtTarget => Some(user_status.set_temperature.clone()),
    };

    let pump_status = hw_status.pumps.into_iter()
        .map(|p| {
          match run_status.pumps_forced_low {
            Some(true) => ParsedEnum::new(PumpStatus::Low),
            _ => p
          }
        })
        .collect();

    let status = StatusUpdateResponseV1 {
      spa_state: ParsedEnum::new(run_status.spa_mode),
      init_mode: ParsedEnum::new(run_status.init_mode),
      current_temperature,
      time: user_status.time,
      heating_mode: ParsedEnum::new(run_status.heating_mode),
      reminder_type: ParsedEnum::new(ReminderType::None),
      hold_timer: None,
      filter_mode: ParsedEnum::new(FilterMode::Off),
      panel_locked: false,
      temperate_range: user_status.temperature_range,
      clock_mode: ParsedEnum::new(user_status.clock_mode),
      needs_heat: run_status.needs_heat,
      heating_state: ParsedEnum::new(run_status.heating_state),
      mister_on: ParsedEnum::new(Boolean::False),
      set_temperature: user_status.set_temperature,
      pump_status,
      circulation_pump_on: ParsedEnum::new(Boolean::from(run_status.circulation_pump_on)),
      blower_status: hw_status.blower,
      light_status: hw_status.lights,
      reminder_set: ParsedEnum::new(Boolean::False),
      notification_set: ParsedEnum::new(Boolean::False),
    };
    StatusUpdateMessage {
      v1: status,
      v2: None,
      v3: None,
    }
  }

  pub fn as_configuration(&self) -> ConfigurationResponseMessage {
    self.hardware.as_configuration()
  }

  pub fn as_fault_log(&self, _entry_num: u8) -> FaultResponseMessage {
    FaultResponseMessage {
      total_entries: 0,
      entry_number: 0,
      fault_code: ParsedEnum::from_raw(0),
      days_ago: 0,
      time: ProtocolTime::from_hm(0, 0),
      set_temperature: 0,
    }
  }
}

impl MockSpaState {
  pub fn as_status(&self) -> RuntimeStatus {
    match self {
      MockSpaState::Initializing => {
        RuntimeStatus {
          spa_mode: SpaState::Initializing,
          init_mode: InitializationMode::PrimingMode,
          current_temperature: CurrentTemperatureState::Unknown,
          heating_mode: HeatingMode::Rest,
          needs_heat: true,
          heating_state: HeatingState::Off,
          circulation_pump_on: false,
          pumps_forced_low: Some(false),
        }
      }
      MockSpaState::Heating => {
        RuntimeStatus {
          spa_mode: SpaState::Running,
          init_mode: InitializationMode::Idle,
          current_temperature: CurrentTemperatureState::Low,
          heating_mode: HeatingMode::Ready,
          needs_heat: true,
          heating_state: HeatingState::Heating,
          circulation_pump_on: true,
          pumps_forced_low: Some(true),
        }
      }
      MockSpaState::Hold => {
        RuntimeStatus {
          spa_mode: SpaState::HoldMode,
          init_mode: InitializationMode::Idle,
          current_temperature: CurrentTemperatureState::AtTarget,
          heating_mode: HeatingMode::ReadyInRest,
          needs_heat: false,
          heating_state: HeatingState::HeatWaiting,
          circulation_pump_on: false,
          pumps_forced_low: None,
        }
      }
    }
  }
}


#[derive(Debug)]
pub struct RuntimeStatus {
  spa_mode: SpaState,
  init_mode: InitializationMode,
  current_temperature: CurrentTemperatureState,
  heating_mode: HeatingMode,
  needs_heat: bool,
  heating_state: HeatingState,
  circulation_pump_on: bool,
  pumps_forced_low: Option<bool>,
}

#[derive(Debug, Clone)]
pub enum CurrentTemperatureState {
  Unknown,
  Low,
  AtTarget,
}

impl UserSettings {
  pub fn adjust_temperature(&mut self, value: SetTemperature) {
    let new_temp = self.temperature_scale.new_protocol_temperature_from_set(value);
    self.set_temperature = new_temp.temperature;
  }

  pub fn as_status(&self) -> UserSettingsStatus {
    let now = Utc::now();
    let time = ProtocolTime::from_hm(
      u8::try_from(now.hour()).unwrap(),
      u8::try_from(now.minute()).unwrap());
    let set_temperature = self.temperature_scale.new_protocol_temperature(
        self.set_temperature).unwrap();
    UserSettingsStatus {
      time,
      temperature_scale: self.temperature_scale,
      temperature_range: self.temp_range,
      clock_mode: self.clock_mode,
      set_temperature,
    }
  }
}

#[derive(Debug)]
pub struct UserSettingsStatus {
  time: ProtocolTime,
  temperature_scale: TemperatureScale,
  temperature_range: TemperatureRange,
  clock_mode: ClockMode,
  set_temperature: ProtocolTemperature,
}

impl MockHardware {
  pub fn as_status(&self) -> HardwareStatus {
    let pumps = self.pumps.iter()
        .map(|d| ParsedEnum::new(d.status))
        .collect();
    let lights = self.lights.iter()
        .map(|d| ParsedEnum::new(d.status))
        .collect();
    HardwareStatus {
      pumps,
      blower: ParsedEnum::new(self.blower.status),
      lights,
    }
  }

  fn as_configuration(&self) -> ConfigurationResponseMessage {
    let pumps = self.pumps.iter()
        .map(|p| ParsedEnum::new(p.capability))
        .collect();
    let has_lights = self.lights.iter()
        .map(|_| ParsedEnum::new(Boolean::True))
        .collect();
    ConfigurationResponseMessage {
      pumps,
      has_lights,
      has_blower: true,
      has_circulation_pump: true,
      has_aux: vec![],
      has_mister: ParsedEnum::new(Boolean::False),
    }
  }
}

#[derive(Debug)]
pub struct HardwareStatus {
  pumps: Vec<ParsedEnum<PumpStatus, u8>>,
  blower: ParsedEnum<RelayStatus, u8>,
  lights: Vec<ParsedEnum<RelayStatus, u8>>,
}