use std::fmt::{Debug, Display, Formatter};
use std::io;
use std::io::{Cursor, Write};
use std::time::Duration;
use anyhow::anyhow;
use byteorder::{BigEndian, WriteBytesExt};
use num_derive::FromPrimitive;
use num_derive::ToPrimitive;
use num_traits::{FromPrimitive, ToPrimitive};
use bitvec::prelude::*;
use crate::channel::Channel;
use crate::message::Message;
use crate::parsed_enum::ParsedEnum;
use crate::temperature::{ProtocolTemperature, SetTemperature, TemperatureScale};
use crate::time::ProtocolTime;

#[derive(Debug, Clone)]
#[repr(u8)]
pub enum MessageType {
  NewClientClearToSend() = 0x00,
  ChannelAssignmentRequest {
    device_type: u8,
    client_hash: u16,
  } = 0x01,
  ChannelAssignmentResponse {
    channel: Channel,
    client_hash: u16,
  } = 0x02,
  ChannelAssignmentAck() = 0x03,
  ExistingClientRequest() = 0x04,
  ExistingClientResponse {
    unknown: Vec<u8>,
  } = 0x05,
  ClearToSend() = 0x06,
  NothingToSend() = 0x07,
  ToggleItemRequest {
    item_code: ItemCode,
    dummy1: u8,
  } = 0x11,
  StatusUpdate(StatusUpdateMessage) = 0x13,
  SetTemperatureRequest {
    temperature: SetTemperature,
  } = 0x20,
  SetTimeRequest {
    time: ProtocolTime,
  } = 0x21,
  SettingsRequest(SettingsRequestMessage) = 0x22,
  FilterCycles {
    cycles: Vec<FilterCycle>,
  } = 0x23,
  InformationResponse(InformationResponseMessage) = 0x24,
  PreferencesResponse(PreferencesResponseMessage) = 0x26,
  SetPreferenceRequest(SetPreferenceMessage) = 0x27,
  FaultLogResponse(FaultResponseMessage) = 0x28,
  ChangeSetupRequest {
    setup_number: u8,
  } = 0x2a,
  GfciTestResponse {
    result: ParsedEnum<GfciTestResult, u8>,
  } = 0x2b,
  LockRequest(LockRequestMessage) = 0x2d,
  ConfigurationResponse(ConfigurationResponseMessage) = 0x2e,
  WifiModuleConfigurationResponse {
    mac: [u8; 6],
  } = 0x94,
  ToggleTestSettingRequest(ToggleTestMessage) = 0xe0,
}

#[derive(FromPrimitive, ToPrimitive, Hash, Eq, Debug, Copy, Clone)]
pub enum ItemCode {
  NormalOperation = 0x01,
  ClearNotification = 0x03,
  Pump1 = 0x04,
  Pump2 = 0x05,
  Pump3 = 0x06,
  Pump4 = 0x07,
  Pump5 = 0x08,
  Pump6 = 0x09,
  Blower = 0x0c,
  Mister = 0x0e,
  Light1 = 0x11,
  Light2 = 0x12,
  Aux1 = 0x16,
  Aux2 = 0x17,
  SoakMode = 0x1d,
  HoldMode = 0x3c,
  TemperatureRange = 0x50,
  HeatMode = 0x51,
}

#[derive(Debug, Clone)]
pub struct StatusUpdateMessage {
  pub v1: StatusUpdateResponseV1,
  pub v2: Option<StatusUpdateResponseV2>,
  pub v3: Option<StatusUpdateResponseV3>,
}

impl TryFrom<&StatusUpdateMessage> for Vec<u8> {
  type Error = io::Error;

  fn try_from(value: &StatusUpdateMessage) -> Result<Self, Self::Error> {
    assert!(value.v2.is_none(), "StatusUpdateResponseV2 not supported yet!");
    assert!(value.v3.is_none(), "StatusUpdateResponseV3 not supported yet!");
    Vec::<u8>::try_from(&value.v1)
  }
}

#[derive(Debug, Clone)]
pub struct StatusUpdateResponseV1 {
  pub spa_state: ParsedEnum<SpaState, u8>,
  pub init_mode: ParsedEnum<InitializationMode, u8>,
  pub current_temperature: Option<ProtocolTemperature>,
  pub time: ProtocolTime,
  pub heating_mode: ParsedEnum<HeatingMode, u8>,
  pub reminder_type: ParsedEnum<ReminderType, u8>,
  pub hold_timer: Option<ProtocolTime>,
  pub filter_mode: ParsedEnum<FilterMode, u8>,
  pub panel_locked: bool,
  pub temperate_range: TemperatureRange,
  pub clock_mode: ParsedEnum<ClockMode, u8>,
  pub needs_heat: bool,
  pub heating_state: ParsedEnum<HeatingState, u8>,
  pub mister_on: ParsedEnum<bool, u8>,
  pub set_temperature: ProtocolTemperature,
  pub pump_status: Vec<ParsedEnum<PumpStatus, u8>>,
  pub circulation_pump_on: ParsedEnum<bool, u8>,
  pub blower_status: ParsedEnum<RelayStatus, u8>,
  pub light_status: Vec<ParsedEnum<RelayStatus, u8>>,
  pub reminder_set: ParsedEnum<bool, u8>,
  pub notification_set: ParsedEnum<bool, u8>,
}

#[derive(FromPrimitive, ToPrimitive, Debug, Clone)]
pub enum SpaState {
  Running = 0x00,
  Initializing = 0x01,
  HoldMode = 0x05,
  AbTempsOn = 0x14,
  TestMode = 0x17,
}

#[derive(FromPrimitive, ToPrimitive, Debug, Clone)]
pub enum InitializationMode {
  Idle = 0x00,
  PrimingMode = 0x01,
  PostSettingsReset = 0x02,
  Reminder = 0x03,
  Stage1 = 0x04,
  Stage2 = 0x42,
  Stage3 = 0x05,
}

#[derive(FromPrimitive, ToPrimitive, Debug, Clone)]
pub enum HeatingMode {
  Ready = 0,
  Rest = 1,
  ReadyInRest = 3,
}

#[derive(FromPrimitive, ToPrimitive, Debug, Clone)]
pub enum ReminderType {
  None = 0x00,
  CleanFilter = 0x04,
  CheckPhLevel = 0x0a,
  CheckSanitizer = 0x09,
}

#[derive(FromPrimitive, ToPrimitive, Debug, Clone)]
pub enum FilterMode {
  Off = 0,
  Cycle1 = 1,
  Cycle2 = 2,
  Cycle1And2 = 3,
}

#[derive(FromPrimitive, ToPrimitive, Debug, Clone)]
pub enum TemperatureRange {
  Low = 0,
  High = 1,
}

#[derive(FromPrimitive, ToPrimitive, Debug, Clone)]
pub enum HeatingState {
  Off = 0,
  Heating = 1,
  HeatWaiting = 2,
}

#[derive(FromPrimitive, ToPrimitive, Debug, Clone)]
pub enum PumpStatus {
  Off = 0,
  Low = 1,
  High = 2,
}

#[derive(FromPrimitive, ToPrimitive, Debug, Clone)]
pub enum RelayStatus {
  Off = 0,
  On = 3,
}

impl TryFrom<&StatusUpdateResponseV1> for Vec<u8> {
  type Error = io::Error;

  fn try_from(value: &StatusUpdateResponseV1) -> Result<Self, Self::Error> {
    let mut cursor = Cursor::new(Vec::new());
    cursor.write_u8(value.spa_state.as_raw())?;
    cursor.write_u8(value.init_mode.as_raw())?;
    cursor.write_u8(
      value.current_temperature
        .map(|t| t.raw_value).unwrap_or(0xff))?;
    cursor.write_u16(value.time.as_raw())?;
    cursor.write_u8(value.heating_mode.as_raw())?;
    cursor.write_u8(value.reminder_type.as_raw())?;
    let (sensorA, sensorB) = match value.spa_state.borrow_value() {
      SpaState::AbTempsOn => {
        (
          value.hold_timer.unwrap().to_minutes(),
          value.current_temperature.unwrap().raw_value,
        )
      }
      _ => (0x0, 0x0)
    };
    cursor.write_u8(sensorA)?;
    cursor.write_u8(sensorB)?;

    todo!()
  }
}

#[derive(Debug, Clone)]
pub struct StatusUpdateResponseV2 {
}

#[derive(Debug, Clone)]
pub struct StatusUpdateResponseV3 {
}

#[derive(Debug, Clone)]
#[repr(u8)]
pub enum SettingsRequestMessage {
  Configuration,
  FilterCycles,
  Information,
  Preferences,
  FaultLog {
    entry_num: u8,
  },
  GfciTest,
}

impl From<&SettingsRequestMessage> for Vec<u8> {
  fn from(value: &SettingsRequestMessage) -> Self {
    match value {
      SettingsRequestMessage::Configuration =>
        vec![0x00, 0x0, 0x1],
      SettingsRequestMessage::FilterCycles =>
        vec![0x01, 0x0, 0x0],
      SettingsRequestMessage::Information =>
        vec![0x02, 0x0, 0x0],
      SettingsRequestMessage::Preferences =>
        vec![0x08, 0x0, 0x0],
      SettingsRequestMessage::FaultLog { entry_num } =>
        vec![0x20, *entry_num, 0x0],
      SettingsRequestMessage::GfciTest =>
        vec![0x80, 0x0, 0x0],
    }
  }
}

#[derive(Debug, Clone)]
pub struct FilterCycle {
  enabled: bool,
  start_at: Duration,
  duration: Duration,
}

#[derive(Debug, Clone)]
pub struct InformationResponseMessage {
  software_version: SoftwareVersion,
  system_model_number: String,
  current_configuration_setup: u8,
  configuration_signature: [u8; 4],
  heater_voltage: ParsedEnum<HeaterVoltage, u8>,
  heater_type: ParsedEnum<HeaterType, u8>,
  dip_switch_settings: u16,
}

impl From<&InformationResponseMessage> for Vec<u8> {
  fn from(value: &InformationResponseMessage) -> Self {
    todo!()
  }
}

#[derive(Debug, Clone)]
pub struct PreferencesResponseMessage {
  reminder_set: ParsedEnum<bool, u8>,
  temperature_scale: ParsedEnum<TemperatureScale, u8>,
  clock_mode: ParsedEnum<ClockMode, u8>,
  cleanup_cycle: ParsedEnum<CleanupCycle, u8>,
  dolphin_address: u8,
  m8_artificial_intelligence: ParsedEnum<bool, u8>,
}

impl From<&PreferencesResponseMessage> for Vec<u8> {
  fn from(value: &PreferencesResponseMessage) -> Self {
    todo!()
  }
}

#[derive(Debug, Clone)]
pub struct SoftwareVersion {
  version: [u8; 4],
}

impl Display for SoftwareVersion {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    let suffix = match self.version[4] {
      0 => "".to_owned(),
      n => format!(".{}", n),
    };
    write!(f, "M{}_{} V{}{}", self.version[0], self.version[1], self.version[2], suffix)
  }
}

#[derive(FromPrimitive, ToPrimitive, Debug, Clone)]
pub enum HeaterVoltage {
  V240 = 0x01,
}

#[derive(FromPrimitive, ToPrimitive, Debug, Clone)]
pub enum HeaterType {
  Standard = 0x0a,
}

#[derive(FromPrimitive, ToPrimitive, Debug, Clone)]
pub enum ClockMode {
  Hour12 = 0,
  Hour24 = 1,
}

#[derive(Debug, Clone)]
pub struct CleanupCycle {
  enabled: bool,
  duration: Duration,
}

impl TryFrom<&CleanupCycle> for u8 {
  type Error = anyhow::Error;

  fn try_from(value: &CleanupCycle) -> Result<Self, Self::Error> {
    if value.enabled {
      Ok(0)
    } else {
      let divided = value.duration.as_secs_f64() / (30. * 60.);
      divided.round().to_u8()
          .ok_or_else(|| anyhow!("Cannot convert to u8: {divided}"))
    }
  }
}

#[derive(Debug, Clone)]
#[repr(u8)]
pub enum SetPreferenceMessage {
  Reminders(bool),
  TemperatureScale(TemperatureScale),
  ClockMode(ClockMode),
  CleanupCycle(CleanupCycle),
  DolphinAddress(u8),
  M8ArtificialIntelligence(bool),
}

impl TryFrom<&SetPreferenceMessage> for Vec<u8> {
  type Error = anyhow::Error;

  fn try_from(value: &SetPreferenceMessage) -> Result<Self, Self::Error> {
    let result = match value {
      SetPreferenceMessage::Reminders(v) =>
        vec![0x00, if *v { 1 } else { 0 }],
      SetPreferenceMessage::TemperatureScale(v) =>
        vec![0x01, v.to_u8().unwrap()],
      SetPreferenceMessage::ClockMode(v) =>
        vec![0x02, v.to_u8().unwrap()],
      SetPreferenceMessage::CleanupCycle(v) =>
        vec![0x03, u8::try_from(v)?],
      SetPreferenceMessage::DolphinAddress(v) =>
        vec![0x04, *v],
      SetPreferenceMessage::M8ArtificialIntelligence(v) =>
        vec![0x06, if *v { 1 } else { 0 }]
    };
    Ok(result)
  }
}

#[derive(FromPrimitive, ToPrimitive, thiserror::Error, Debug, Clone)]
pub enum FaultCode {
  #[error("Sensors are out of sync")]
  SensorsOutOfSync = 15,

  #[error("The water flow is low")]
  WaterFlowLow = 16,

  #[error("The water flow has failed")]
  WaterFlowFailed = 17,

  #[error("The settings have been reset")]
  SettingsReset1 = 18,

  #[error("Priming mode")]
  PrimingMode = 19,

  #[error("The clock has failed")]
  ClockFailed = 20,

  #[error("The settings have been reset")]
  SettingsReset2 = 21,

  #[error("Program memory failure")]
  ProgramMemoryFailure = 22,

  #[error("Sensors are out of sync -- call for service")]
  SensorsOutOfSyncCallForService = 26,

  #[error("The heater is dry")]
  HeaterIsDry = 27,

  #[error("The heater may be dry")]
  HeaterMayBeDry = 28,

  #[error("The water is too hot")]
  WaterTooHot = 29,

  #[error("The heater is too hot")]
  HeaterTooHot = 30,

  #[error("Sensor A fault")]
  SensorAFault = 31,

  #[error("Sensor B fault")]
  SensorBFault = 32,

  #[error("A pump may be stuck on")]
  PumpMayBeStuckOn = 34,

  #[error("Hot fault")]
  HotFault = 35,

  #[error("The GFCI test failed")]
  GfciTestFailed = 36,

  #[error("Standby Mode (Hold Mode")]
  StandbyMode = 37,
}

#[derive(FromPrimitive, ToPrimitive, Debug, Clone)]
pub enum GfciTestResult {
  Fail = 0x0,
  Pass = 0x1,
}

#[derive(FromPrimitive, ToPrimitive, Debug, Clone)]
pub enum LockRequestMessage {
  LockSettings = 0x01,
  LockPanel = 0x02,
  UnlockSettings = 0x03,
  UnlockPanel = 0x04,
}

#[derive(Debug, Clone)]
pub struct ConfigurationResponseMessage {
  pumps: Vec<ParsedEnum<PumpConfig, u8>>,
  has_lights: Vec<ParsedEnum<bool, u8>>,
  has_blower: bool,
  has_circular_pump: bool,
  has_aux: Vec<ParsedEnum<bool, u8>>,
  has_mister: ParsedEnum<bool, u8>,
}

impl From<&ConfigurationResponseMessage> for Vec<u8> {
  fn from(value: &ConfigurationResponseMessage) -> Self {
    todo!()
  }
}

#[derive(Debug, Clone)]
pub struct PumpConfig {
  present: bool,
  num_speeds: u8,
}

#[derive(Debug, Clone)]
pub struct FaultResponseMessage {
  total_entries: u8,
  entry_number: u8,
  fault_code: ParsedEnum<FaultCode, u8>,
}

impl From<&FaultResponseMessage> for Vec<u8> {
  fn from(value: &FaultResponseMessage) -> Self {
    todo!()
  }
}

#[derive(FromPrimitive, ToPrimitive, Debug, Clone)]
pub enum ToggleTestMessage {
  SensorABTemperatures = 0x03,
  Timeouts = 0x04,
  TempLimits = 0x05,
}

impl MessageType {
  fn discriminant(&self) -> u8 {
    // This comes from docs on std::mem::discriminant and works only because MessageType is
    // #[repr(u8)]
    unsafe { *<*const _>::from(self).cast::<u8>() }
  }

  pub fn to_message(self, channel: Channel) -> Result<Message, PayloadEncodeError> {
    Ok(Message::new(channel, self.discriminant(), Vec::<u8>::try_from(self)?))
  }
}

impl TryFrom<&Message> for MessageType {
  type Error = PayloadParseError;

  fn try_from(value: &Message) -> Result<Self, Self::Error> {
    todo!()
  }
}

impl TryFrom<MessageType> for Vec<u8> {
  type Error = PayloadEncodeError;

  fn try_from(value: MessageType) -> Result<Self, Self::Error> {
    let result = match value {
      MessageType::NewClientClearToSend() => vec![],
      MessageType::ChannelAssignmentRequest { device_type, client_hash } => {
        let mut cursor = Cursor::new(Vec::with_capacity(3));
        cursor.write_u8(device_type)?;
        cursor.write_u16::<BigEndian>(client_hash)?;
        cursor.into_inner()
      }
      MessageType::ChannelAssignmentResponse { channel, client_hash } => {
        let mut cursor = Cursor::new(Vec::with_capacity(3));
        cursor.write_u8(u8::from(&channel))?;
        cursor.write_u16::<BigEndian>(client_hash)?;
        cursor.into_inner()
      }
      MessageType::ChannelAssignmentAck() => vec![],
      MessageType::ExistingClientRequest() => vec![],
      MessageType::ExistingClientResponse { unknown } => unknown,
      MessageType::ClearToSend() => vec![],
      MessageType::NothingToSend() => vec![],
      MessageType::ToggleItemRequest { item_code, dummy1 } =>
        vec![item_code.to_u8().unwrap(), dummy1],
      MessageType::StatusUpdate(message) =>
        Vec::<u8>::try_from(&message)?,
      MessageType::SetTemperatureRequest { temperature } =>
        vec![temperature.raw_value],
      MessageType::SetTimeRequest { time } => {
        let mut cursor = Cursor::new(Vec::with_capacity(2));
        cursor.write_u16(time.as_raw())?;
        cursor.into_inner()
      }
      MessageType::SettingsRequest(message) =>
        Vec::<u8>::from(message),
      MessageType::FilterCycles { cycles } => {
        return Err(PayloadEncodeError::NotSupported)
      }
      MessageType::InformationResponse(message) =>
        Vec::<u8>::from(&message),
      MessageType::PreferencesResponse(message) =>
        Vec::<u8>::from(&message),
      MessageType::SetPreferenceRequest(message) =>
        Vec::<u8>::try_from(&message)?,
      MessageType::FaultLogResponse(message) =>
        Vec::<u8>::from(&message),
      MessageType::ChangeSetupRequest { setup_number } =>
        vec![setup_number],
      MessageType::GfciTestResponse { result } =>
        vec![result.as_raw()],
      MessageType::LockRequest(message) =>
        vec![message.to_u8().unwrap()],
      MessageType::ConfigurationResponse(message) =>
        Vec::<u8>::from(&message),
      MessageType::WifiModuleConfigurationResponse { mac } =>
        mac.to_vec(),
      MessageType::ToggleTestSettingRequest(message) =>
        vec![message.to_u8().unwrap()],
      _ => return Err(PayloadEncodeError::NotSupported),
    };
    Ok(result)
  }
}

#[derive(thiserror::Error, Debug)]
pub enum PayloadParseError {
  #[error("Wrong message type")]
  InvalidMessageType,

  #[error("Unexpected EOF")]
  UnexpectedEof(#[from] io::Error),
}

#[derive(thiserror::Error, Debug, Clone)]
pub enum PayloadEncodeError {
  #[error("Generic I/O error")]
  GenericIoError(#[from] io::Error),

  #[error("Generic internal error")]
  GenericError(#[from] anyhow::Error),

  #[error("Message type encoding not yet supported")]
  NotSupported,
}