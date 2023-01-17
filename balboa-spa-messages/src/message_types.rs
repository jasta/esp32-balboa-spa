//! [De/]Serializers for each individual message type.
//!
//! TODO1: There's a good amount of code
//! duplication here in order to achieve convenient mapping between types and the protocol
//! discriminant.  It looks like Rust is really lacking a way to do this elegantly, even with
//! external crates like enum_kinds which don't support complex enum discriminants yet.  See:
//!
//! https://github.com/Soft/enum-kinds/pull/7#issuecomment-1381043346
//!
//! TODO2: Use binread/binwrite to greatly reduce boilerplate here, might even solve the enum-kinds
//! problem!

use std::fmt::{Debug, Display, Formatter};
use std::io;
use std::io::{Cursor, Read, Write};
use std::string::FromUtf8Error;
use std::time::Duration;

use anyhow::anyhow;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use num_derive::FromPrimitive;
use num_derive::ToPrimitive;
use num_traits::{FromPrimitive, ToPrimitive};
use packed_struct::prelude::*;

use crate::channel::Channel;
use crate::array_utils;
use crate::message::Message;
use crate::parsed_enum::ParsedEnum;
use crate::temperature::{ProtocolTemperature, SetTemperature, TemperatureScale};
use crate::time::ProtocolTime;

const MINUTES_30: Duration = Duration::from_secs(30 * 60);

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
    item_code: ParsedEnum<ItemCode, u8>,
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

#[derive(FromPrimitive, ToPrimitive, Debug, Copy, PartialEq, Clone)]
#[repr(u8)]
pub enum MessageTypeKind {
  NewClientClearToSend = 0x00,
  ChannelAssignmentRequest = 0x01,
  ChannelAssignmentResponse = 0x02,
  ChannelAssignmentAck = 0x03,
  ExistingClientRequest = 0x04,
  ExistingClientResponse = 0x05,
  ClearToSend = 0x06,
  NothingToSend = 0x07,
  ToggleItemRequest = 0x11,
  StatusUpdate = 0x13,
  SetTemperatureRequest = 0x20,
  SetTimeRequest = 0x21,
  SettingsRequest = 0x22,
  FilterCycles = 0x23,
  InformationResponse = 0x24,
  PreferencesResponse = 0x26,
  SetPreferenceRequest = 0x27,
  FaultLogResponse = 0x28,
  ChangeSetupRequest = 0x2a,
  GfciTestResponse = 0x2b,
  LockRequest = 0x2d,
  ConfigurationResponse = 0x2e,
  WifiModuleConfigurationResponse = 0x94,
  ToggleTestSettingRequest = 0xe0,
}

impl From<&MessageType> for MessageTypeKind {
  fn from(value: &MessageType) -> Self {
    Self::from_u8(value.discriminant()).unwrap()
  }
}

#[derive(FromPrimitive, ToPrimitive, Debug, Copy, Clone)]
pub enum Boolean {
  False = 0,
  True = 1,
}

impl From<Boolean> for bool {
  fn from(value: Boolean) -> Self {
    match value {
      Boolean::False => false,
      Boolean::True => false,
    }
  }
}

impl From<&Boolean> for bool {
  fn from(value: &Boolean) -> Self {
    (*value).into()
  }
}

impl From<bool> for Boolean {
  fn from(value: bool) -> Self {
    match value {
      false => Boolean::False,
      true => Boolean::True,
    }
  }
}

#[derive(FromPrimitive, ToPrimitive, Hash, PartialEq, Eq, Debug, Copy, Clone)]
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
  type Error = anyhow::Error;

  fn try_from(value: &StatusUpdateMessage) -> Result<Self, Self::Error> {
    assert!(value.v2.is_none(), "StatusUpdateResponseV2 not supported yet!");
    assert!(value.v3.is_none(), "StatusUpdateResponseV3 not supported yet!");
    Vec::<u8>::try_from(&value.v1)
  }
}

impl TryFrom<&[u8]> for StatusUpdateMessage {
  type Error = anyhow::Error;

  fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
    let v1 = StatusUpdateResponseV1::try_from(value)?;
    Ok(Self {
      v1,
      v2: None,
      v3: None,
    })
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
  pub mister_on: ParsedEnum<Boolean, u8>,
  pub set_temperature: ProtocolTemperature,
  pub pump_status: Vec<ParsedEnum<PumpStatus, u8>>,
  pub circulation_pump_on: ParsedEnum<Boolean, u8>,
  pub blower_status: ParsedEnum<RelayStatus, u8>,
  pub light_status: Vec<ParsedEnum<RelayStatus, u8>>,
  pub reminder_set: ParsedEnum<Boolean, u8>,
  pub notification_set: ParsedEnum<Boolean, u8>,
}

#[derive(PackedStruct)]
#[packed_struct(bit_numbering="msb0")]
pub struct StatusFlags9_14 {
  #[packed_field(bits="2")]
  panel_locked: bool,

  #[packed_field(bits="3..=4", ty="enum")]
  filter_mode: FilterMode,

  #[packed_field(bits="6", ty="enum")]
  clock_mode: ClockMode,

  #[packed_field(bits="7", ty="enum")]
  temperature_scale: TemperatureScale,

  #[packed_field(bits="10..=11", ty="enum")]
  heating_state: HeatingState,

  #[packed_field(bits="12")]
  needs_heat: bool,

  #[packed_field(bits="13", ty="enum")]
  temperature_range: TemperatureRange,

  #[packed_field(bits="16..=17", ty="enum")]
  pump4_status: PumpStatus,

  #[packed_field(bits="18..=19", ty="enum")]
  pump3_status: PumpStatus,

  #[packed_field(bits="20..=21", ty="enum")]
  pump2_status: PumpStatus,

  #[packed_field(bits="22..=23", ty="enum")]
  pump1_status: PumpStatus,

  #[packed_field(bits="24..=25", ty="enum")]
  pump6_status: PumpStatus,

  #[packed_field(bits="26..=27", ty="enum")]
  pump5_status: PumpStatus,

  #[packed_field(bits="37..=38", ty="enum")]
  blower_status: RelayStatus,

  #[packed_field(bits="39")]
  circulation_pump_on: bool,

  #[packed_field(bits="44..=45", ty="enum")]
  light2_status: RelayStatus,

  #[packed_field(bits="46..=47", ty="enum")]
  light1_status: RelayStatus,
}

#[derive(PackedStruct)]
#[packed_struct(bit_numbering="msb0")]
pub struct StatusFlags18_19 {
  #[packed_field(bits="15")]
  reminder: bool,

  #[packed_field(bits="2")]
  notification: bool,
}

#[derive(PackedStruct)]
#[packed_struct(bit_numbering="msb0")]
pub struct StatusFlags21 {
  #[packed_field(bits="6")]
  sensor_ab: bool,

  #[packed_field(bits="5")]
  timeouts_are_8hr: bool,

  #[packed_field(bits="4")]
  settings_locked: bool,
}

#[derive(FromPrimitive, ToPrimitive, Debug, PartialEq, Clone)]
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

#[derive(FromPrimitive, ToPrimitive, PrimitiveEnum_u8, Debug, Copy, Clone)]
pub enum FilterMode {
  Off = 0,
  Cycle1 = 1,
  Cycle2 = 2,
  Cycle1And2 = 3,
}

#[derive(FromPrimitive, ToPrimitive, PrimitiveEnum_u8, Debug, Copy, Clone)]
pub enum TemperatureRange {
  Low = 0,
  High = 1,
}

#[derive(FromPrimitive, ToPrimitive, PrimitiveEnum_u8, Debug, Copy, Clone)]
pub enum HeatingState {
  Off = 0,
  Heating = 1,
  HeatWaiting = 2,
}

#[derive(FromPrimitive, ToPrimitive, PrimitiveEnum_u8, Debug, Copy, Clone)]
pub enum PumpStatus {
  Off = 0,
  Low = 1,
  High = 2,
}

#[derive(FromPrimitive, ToPrimitive, PrimitiveEnum_u8, Debug, Copy, Clone)]
pub enum RelayStatus {
  Off = 0,
  On = 3,
}

impl TryFrom<&StatusUpdateResponseV1> for Vec<u8> {
  type Error = anyhow::Error;

  fn try_from(value: &StatusUpdateResponseV1) -> Result<Self, Self::Error> {
    let mut cursor = Cursor::new(Vec::new());
    cursor.write_u8(value.spa_state.as_raw())?;
    cursor.write_u8(value.init_mode.as_raw())?;
    cursor.write_u8(
      value.current_temperature.as_ref()
        .map(|t| t.raw_value).unwrap_or(0xff))?;
    cursor.write_u16::<BigEndian>(value.time.as_raw())?;
    cursor.write_u8(value.heating_mode.as_raw())?;
    cursor.write_u8(value.reminder_type.as_raw())?;
    let is_ab_temps_on = value.spa_state.as_ref()
        .map(|s| s == &SpaState::AbTempsOn)
        .unwrap_or(false);

    let (sensor_a, sensor_b) = match is_ab_temps_on {
      true => {
        (
          value.hold_timer.unwrap().to_minutes(),
          value.current_temperature.as_ref().unwrap().raw_value,
        )
      }
      false => (0x0, 0x0)
    };
    cursor.write_u8(sensor_a)?;
    cursor.write_u8(sensor_b)?;

    let mut pump_status = [PumpStatus::Off; 6];
    for (i, val) in pump_status.iter_mut().enumerate() {
      if let Some(pump) = value.pump_status.get(i) {
        *val = *pump.as_ref().unwrap();
      }
    }

    let mut light_status = [RelayStatus::Off; 2];
    for (i, val) in light_status.iter_mut().enumerate() {
      if let Some(light) = value.light_status.get(i) {
        *val = *light.as_ref().unwrap();
      }
    }

    let flags9_14 = StatusFlags9_14 {
      temperature_scale: value.set_temperature.raw_scale.clone(),
      clock_mode: value.clock_mode.as_ref().unwrap().to_owned(),
      filter_mode: value.filter_mode.as_ref().unwrap().to_owned(),
      panel_locked: value.panel_locked,
      temperature_range: value.temperate_range.clone(),
      needs_heat: value.needs_heat,
      heating_state: value.heating_state.as_ref().unwrap().to_owned(),
      pump1_status: pump_status[0],
      pump2_status: pump_status[1],
      pump3_status: pump_status[2],
      pump4_status: pump_status[3],
      pump5_status: pump_status[4],
      pump6_status: pump_status[5],
      circulation_pump_on: value.circulation_pump_on.as_ref().unwrap().into(),
      blower_status: value.blower_status.as_ref().unwrap().to_owned(),
      light1_status: light_status[0],
      light2_status: light_status[1],
    };
    let packed9_14 = flags9_14.pack()?;
    cursor.write_all(&packed9_14)?;

    cursor.write_u8(value.mister_on.as_raw())?;
    cursor.write_u8(0)?; // ???
    cursor.write_u8(0)?; // ???

    let flags18_19 = StatusFlags18_19 {
      reminder: value.reminder_set.as_ref().unwrap().into(),
      notification: value.notification_set.as_ref().unwrap().into(),
    };
    let packed18_19 = flags18_19.pack()?;
    cursor.write_all(&packed18_19)?;

    cursor.write_u8(value.set_temperature.raw_value)?;

    let flags21 = StatusFlags21 {
      sensor_ab: is_ab_temps_on,
      timeouts_are_8hr: false,
      settings_locked: false,
    };
    let packed21 = flags21.pack()?;
    cursor.write_all(&packed21)?;

    Ok(cursor.into_inner())
  }
}

impl TryFrom<&[u8]> for StatusUpdateResponseV1 {
  type Error = anyhow::Error;

  fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
    let mut cursor = Cursor::new(value);
    let spa_state = ParsedEnum::from_raw(cursor.read_u8()?);
    let init_mode = ParsedEnum::from_raw(cursor.read_u8()?);
    let raw_current_temperature = cursor.read_u8()?;
    let time_hour = cursor.read_u8()?;
    let time_minute = cursor.read_u8()?;
    let time = ProtocolTime::from_hm(time_hour, time_minute);
    let heating_mode = ParsedEnum::from_raw(cursor.read_u8()?);
    let reminder_type = ParsedEnum::from_raw(cursor.read_u8()?);
    let _sensor_a_temp = cursor.read_u8()?;
    let _sensor_b_temp = cursor.read_u8()?;
    let mut flags9_14 = [0u8; 6];
    cursor.read_exact(&mut flags9_14)?;
    let unpacked9_14 = StatusFlags9_14::unpack(&flags9_14)?;
    let mister_on = ParsedEnum::from_raw(cursor.read_u8()?);
    let _ = cursor.read_u8()?;
    let _ = cursor.read_u8()?;
    let mut flags18_19 = [0u8; 2];
    cursor.read_exact(&mut flags18_19)?;
    let unpacked18_19 = StatusFlags18_19::unpack(&flags18_19)?;
    let raw_set_temperature = cursor.read_u8()?;
    let mut flags21 = [0u8; 1];
    cursor.read_exact(&mut flags21)?;
    let _unpacked21 = StatusFlags21::unpack(&flags21)?;

    let current_temperature = match raw_current_temperature {
      0xff => None,
      raw_temp => Some(unpacked9_14.temperature_scale.new_protocol_temperature_from_raw(raw_temp)),
    };
    let set_temperature =
        unpacked9_14.temperature_scale.new_protocol_temperature_from_raw(raw_set_temperature);

    let pump_status = [
      unpacked9_14.pump1_status,
      unpacked9_14.pump2_status,
      unpacked9_14.pump3_status,
      unpacked9_14.pump4_status,
      unpacked9_14.pump5_status,
      unpacked9_14.pump6_status,
    ]
        .into_iter()
        .map(|p| ParsedEnum::new(p))
        .collect();

    let light_status = [
      unpacked9_14.light1_status,
      unpacked9_14.light2_status,
    ]
        .into_iter()
        .map(|l| ParsedEnum::new(l))
        .collect();

    Ok(Self {
      spa_state,
      init_mode,
      current_temperature,
      time,
      heating_mode,
      reminder_type,
      hold_timer: None,
      filter_mode: ParsedEnum::new(unpacked9_14.filter_mode),
      panel_locked: unpacked9_14.panel_locked,
      temperate_range: unpacked9_14.temperature_range,
      clock_mode: ParsedEnum::new(unpacked9_14.clock_mode),
      needs_heat: unpacked9_14.needs_heat,
      heating_state: ParsedEnum::new(unpacked9_14.heating_state),
      mister_on,
      set_temperature,
      pump_status,
      circulation_pump_on: ParsedEnum::new(unpacked9_14.circulation_pump_on.into()),
      blower_status: ParsedEnum::new(unpacked9_14.blower_status),
      light_status,
      reminder_set: ParsedEnum::new(unpacked18_19.reminder.into()),
      notification_set: ParsedEnum::new(unpacked18_19.notification.into()),
    })
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

impl TryFrom<&[u8]> for SettingsRequestMessage {
  type Error = PayloadParseError;

  fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
    let mut cursor = Cursor::new(value);
    let result = match cursor.read_u8()? {
      0x00 => Self::Configuration,
      0x01 => Self::FilterCycles,
      0x02 => Self::Information,
      0x08 => Self::Preferences,
      0x20 => Self::FaultLog { entry_num: cursor.read_u8()? },
      0x80 => Self::GfciTest,
      _=> return Err(PayloadParseError::InvalidMessageType),
    };
    Ok(result)
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
  pub software_version: SoftwareVersion,
  pub system_model_number: String,
  pub current_configuration_setup: u8,
  pub configuration_signature: [u8; 4],
  pub heater_voltage: ParsedEnum<HeaterVoltage, u8>,
  pub heater_type: ParsedEnum<HeaterType, u8>,
  pub dip_switch_settings: u16,
}

impl TryFrom<&InformationResponseMessage> for Vec<u8> {
  type Error = PayloadEncodeError;

  fn try_from(value: &InformationResponseMessage) -> Result<Self, Self::Error> {
    let mut cursor = Cursor::new(Vec::new());
    cursor.write_all(&value.software_version.version)?;
    let system_model_number = array_utils::encode_array::<8>(
        "system_model_number",
        value.system_model_number.as_bytes())?;
    cursor.write_all(&system_model_number)?;
    cursor.write_u8(value.current_configuration_setup)?;
    cursor.write_all(&value.configuration_signature)?;
    cursor.write_u8(value.heater_voltage.as_raw())?;
    cursor.write_u8(value.heater_type.as_raw())?;
    cursor.write_u16::<BigEndian>(value.dip_switch_settings)?;
    Ok(cursor.into_inner())
  }
}

impl TryFrom<&[u8]> for InformationResponseMessage {
  type Error = PayloadParseError;

  fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
    let mut cursor = Cursor::new(value);
    let mut version = [0u8; 4];
    cursor.read_exact(&mut version)?;
    let mut system_model_number = [0u8; 8];
    cursor.read_exact(&mut system_model_number)?;
    let current_configuration_setup = cursor.read_u8()?;
    let mut configuration_signature = [0u8; 4];
    cursor.read_exact(&mut configuration_signature)?;
    let heater_voltage = ParsedEnum::from_raw(cursor.read_u8()?);
    let heater_type = ParsedEnum::from_raw(cursor.read_u8()?);
    let dip_switch_settings = cursor.read_u16::<BigEndian>()?;
    Ok(Self {
      software_version: SoftwareVersion { version },
      system_model_number: String::from_utf8(system_model_number.to_vec())?,
      current_configuration_setup,
      configuration_signature,
      heater_voltage,
      heater_type,
      dip_switch_settings,
    })
  }
}

#[derive(Debug, Clone)]
pub struct PreferencesResponseMessage {
  reminder_set: ParsedEnum<Boolean, u8>,
  temperature_scale: ParsedEnum<TemperatureScale, u8>,
  clock_mode: ParsedEnum<ClockMode, u8>,
  cleanup_cycle: ParsedEnum<CleanupCycle, u8>,
  dolphin_address: u8,
  m8_artificial_intelligence: ParsedEnum<Boolean, u8>,
}

impl TryFrom<&PreferencesResponseMessage> for Vec<u8> {
  type Error = PayloadEncodeError;

  fn try_from(value: &PreferencesResponseMessage) -> Result<Self, Self::Error> {
    let mut cursor = Cursor::new(Vec::new());
    cursor.write_u8(0)?;
    cursor.write_u8(value.reminder_set.as_raw())?;
    cursor.write_u8(0)?;
    cursor.write_u8(value.temperature_scale.as_raw())?;
    cursor.write_u8(value.clock_mode.as_raw())?;
    cursor.write_u8(value.cleanup_cycle.as_raw())?;
    cursor.write_u8(value.dolphin_address)?;
    cursor.write_u8(0)?;
    cursor.write_u8(value.m8_artificial_intelligence.as_raw())?;
    cursor.write_all(&[0u8; 9])?;
    Ok(cursor.into_inner())
  }
}

impl TryFrom<&[u8]> for PreferencesResponseMessage {
  type Error = PayloadParseError;

  fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
    let mut cursor = Cursor::new(value);
    cursor.read_u8()?;
    let reminder_set = ParsedEnum::from_raw(cursor.read_u8()?);
    cursor.read_u8()?;
    let temperature_scale = ParsedEnum::from_raw(cursor.read_u8()?);
    let clock_mode = ParsedEnum::from_raw(cursor.read_u8()?);
    let cleanup_cycle = ParsedEnum::from_raw(cursor.read_u8()?);
    let dolphin_address = cursor.read_u8()?;
    cursor.read_u8()?;
    let m8_artificial_intelligence = ParsedEnum::from_raw(cursor.read_u8()?);
    Ok(Self {
      reminder_set,
      temperature_scale,
      clock_mode,
      cleanup_cycle,
      dolphin_address,
      m8_artificial_intelligence,
    })
  }
}

#[derive(Debug, Clone)]
pub struct SoftwareVersion {
  pub version: [u8; 4],
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

#[derive(FromPrimitive, ToPrimitive, PrimitiveEnum_u8, Debug, Copy, Clone)]
pub enum ClockMode {
  Hour12 = 0,
  Hour24 = 1,
}

#[derive(Debug, Clone)]
pub struct CleanupCycle {
  duration: Option<Duration>,
}

impl TryFrom<&CleanupCycle> for u8 {
  type Error = PayloadEncodeError;

  fn try_from(value: &CleanupCycle) -> Result<Self, Self::Error> {
    let encoded = match value.duration {
      None => 0,
      Some(duration) => {
        let divided = duration.as_secs_f64() / MINUTES_30.as_secs_f64();
        divided.round().to_u8()
            .ok_or_else(|| anyhow!("Cannot convert to u8: {divided}"))?
      }
    };
    Ok(encoded)
  }
}

impl TryFrom<u8> for CleanupCycle {
  type Error = PayloadParseError;

  fn try_from(value: u8) -> Result<Self, Self::Error> {
    let parsed = match value {
      0 => Self { duration: None },
      cycle => {
        let duration = Duration::from_secs(u64::from(cycle) * MINUTES_30.as_secs());
        Self { duration: Some(duration) }
      }
    };
    Ok(parsed)
  }
}

impl FromPrimitive for CleanupCycle {
  fn from_i64(n: i64) -> Option<Self> {
    u8::try_from(n)
        .ok()
        .and_then(|v| Self::try_from(v).ok())
  }

  fn from_u64(n: u64) -> Option<Self> {
    u8::try_from(n)
        .ok()
        .and_then(|v| Self::try_from(v).ok())
  }
}

impl ToPrimitive for CleanupCycle {
  fn to_i64(&self) -> Option<i64> {
    u8::try_from(self)
        .ok()
        .map(i64::from)
  }

  fn to_u64(&self) -> Option<u64> {
    u8::try_from(self)
        .ok()
        .map(u64::from)
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
  pub pumps: Vec<ParsedEnum<PumpConfig, u8>>,
  pub has_lights: Vec<ParsedEnum<Boolean, u8>>,
  pub has_blower: bool,
  pub has_circulation_pump: bool,
  pub has_aux: Vec<ParsedEnum<Boolean, u8>>,
  pub has_mister: ParsedEnum<Boolean, u8>,
}

#[derive(PackedStruct)]
#[packed_struct(bit_numbering="msb0")]
pub struct ConfigurationResponsePack {
  #[packed_field(bits="0..=1", ty="enum")]
  pump4: PumpConfig,

  #[packed_field(bits="2..=3", ty="enum")]
  pump3: PumpConfig,

  #[packed_field(bits="4..=5", ty="enum")]
  pump2: PumpConfig,

  #[packed_field(bits="6..=7", ty="enum")]
  pump1: PumpConfig,

  #[packed_field(bits="8..=9", ty="enum")]
  pump6: PumpConfig,

  #[packed_field(bits="10..=11", ty="enum")]
  pump5: PumpConfig,

  #[packed_field(bits="16..=17", ty="enum")]
  light2: RelayConfig,

  #[packed_field(bits="22..=23", ty="enum")]
  light1: RelayConfig,

  #[packed_field(bits="24", ty="enum")]
  circulation_pump: RelayConfig,

  #[packed_field(bits="30..=31", ty="enum")]
  blower: RelayConfig,

  #[packed_field(bits="34..=35", ty="enum")]
  mister: RelayConfig,

  #[packed_field(bits="36", ty="enum")]
  aux2: RelayConfig,

  #[packed_field(bits="37", ty="enum")]
  aux1: RelayConfig,

  #[packed_field(bits="40..=47")]
  unknown: u8,
}

impl TryFrom<&ConfigurationResponseMessage> for Vec<u8> {
  type Error = PayloadEncodeError;

  fn try_from(value: &ConfigurationResponseMessage) -> Result<Self, Self::Error> {
    let mut pumps = [PumpConfig::None; 6];
    for (i, val) in pumps.iter_mut().enumerate() {
      if let Some(pump) = value.pumps.get(i) {
        *val = PumpConfig::from_primitive(pump.as_raw()).unwrap_or(PumpConfig::None);
      }
    }

    let mut lights = [RelayConfig::None; 2];
    for (i, val) in lights.iter_mut().enumerate() {
      if let Some(light) = value.has_lights.get(i) {
        *val = RelayConfig::from_primitive(light.as_raw()).unwrap_or(RelayConfig::None);
      }
    }

    let mut aux = [RelayConfig::None; 2];
    for (i, val) in aux.iter_mut().enumerate() {
      if let Some(aux) = value.has_aux.get(i) {
        *val = RelayConfig::from_primitive(aux.as_raw()).unwrap_or(RelayConfig::None);
      }
    }

    let blower = RelayConfig::from(Boolean::from(value.has_blower));
    let circulation_pump = RelayConfig::from(Boolean::from(value.has_circulation_pump));

    // Maybe something to do with circulation pump??
    let unknown = match value.has_circulation_pump {
      true => 0x68,
      false => 0x00,
    };

    let packed_struct = ConfigurationResponsePack {
      pump1: pumps[0],
      pump2: pumps[1],
      pump3: pumps[2],
      pump4: pumps[3],
      pump5: pumps[4],
      pump6: pumps[5],
      light1: lights[0],
      light2: lights[1],
      blower,
      circulation_pump,
      aux1: aux[0],
      aux2: aux[1],
      mister: RelayConfig::from_primitive(value.has_mister.as_raw()).unwrap(),
      unknown,
    };

    Ok(packed_struct.pack()
        .map_err(anyhow::Error::msg)?
        .to_vec())
  }
}

impl TryFrom<&[u8]> for ConfigurationResponseMessage {
  type Error = PayloadParseError;

  fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
    let unpacked = ConfigurationResponsePack::unpack_from_slice(value)
        .map_err(anyhow::Error::msg)?;

    let pumps = [
      unpacked.pump1,
      unpacked.pump2,
      unpacked.pump3,
      unpacked.pump4,
      unpacked.pump5,
      unpacked.pump6,
    ]
        .into_iter()
        .map(|p| ParsedEnum::new(p))
        .collect();

    let has_lights = [
      unpacked.light1,
      unpacked.light2,
    ]
        .into_iter()
        .map(|l| ParsedEnum::new(Boolean::from(l)))
        .collect();

    let has_aux = [
      unpacked.aux1,
      unpacked.aux2,
    ]
        .into_iter()
        .map(|a| ParsedEnum::new(Boolean::from(a)))
        .collect();

    Ok(Self {
      pumps,
      has_lights,
      has_blower: Boolean::from(unpacked.blower).into(),
      has_circulation_pump: Boolean::from(unpacked.circulation_pump).into(),
      has_aux,
      has_mister: ParsedEnum::new(Boolean::from(unpacked.mister)),
    })
  }
}

#[derive(FromPrimitive, ToPrimitive, PrimitiveEnum_u8, Debug, Copy, Clone)]
pub enum PumpConfig {
  None = 0x0,
  Speed1 = 0x1,
  Speed2 = 0x2,
}

#[derive(FromPrimitive, ToPrimitive, PrimitiveEnum_u8, Debug, Copy, Clone)]
pub enum RelayConfig {
  None = 0,
  Present = 1,
}

impl From<Boolean> for RelayConfig {
  fn from(value: Boolean) -> Self {
    match value {
      Boolean::True => RelayConfig::Present,
      Boolean::False => RelayConfig::None,
    }
  }
}

impl From<RelayConfig> for Boolean {
  fn from(value: RelayConfig) -> Self {
    match value {
      RelayConfig::Present => Boolean::True,
      RelayConfig::None => Boolean::False,
    }
  }
}

#[derive(Debug, Clone)]
pub struct FaultResponseMessage {
  pub total_entries: u8,
  pub entry_number: u8,
  pub fault_code: ParsedEnum<FaultCode, u8>,
  pub days_ago: u8,
  pub time: ProtocolTime,
  pub set_temperature: u8, // <-- what's the scale!?!
}

impl TryFrom<&FaultResponseMessage> for Vec<u8> {
  type Error = PayloadEncodeError;

  fn try_from(value: &FaultResponseMessage) -> Result<Self, Self::Error> {
    let mut cursor = Cursor::new(Vec::new());
    cursor.write_u8(value.total_entries)?;
    cursor.write_u8(value.entry_number)?;
    cursor.write_u8(value.fault_code.as_raw())?;
    cursor.write_u8(value.days_ago)?;
    cursor.write_u16::<BigEndian>(value.time.as_raw())?;
    cursor.write_u8(0)?;
    cursor.write_u8(value.set_temperature)?;
    cursor.write_u8(0)?;
    cursor.write_u8(0)?;
    Ok(cursor.into_inner())
  }
}

impl TryFrom<&[u8]> for FaultResponseMessage {
  type Error = PayloadParseError;

  fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
    let mut cursor = Cursor::new(value);
    let total_entries = cursor.read_u8()?;
    let entry_number = cursor.read_u8()?;
    let fault_code = ParsedEnum::from_raw(cursor.read_u8()?);
    let days_ago = cursor.read_u8()?;
    let hour = cursor.read_u8()?;
    let minute = cursor.read_u8()?;
    let time = ProtocolTime::from_hm(hour, minute);
    let _ = cursor.read_u8()?;
    let set_temperature = cursor.read_u8()?;
    let _ = cursor.read_u8()?;
    let _ = cursor.read_u8()?;
    Ok(Self {
      total_entries,
      entry_number,
      fault_code,
      days_ago,
      time,
      set_temperature,
    })
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
    let kind = MessageTypeKind::from_u8(value.message_type)
        .ok_or(PayloadParseError::InvalidMessageType)?;
    let parsed = match kind {
      MessageTypeKind::NewClientClearToSend => MessageType::NewClientClearToSend(),
      MessageTypeKind::ChannelAssignmentRequest => {
        let mut cursor = Cursor::new(&value.payload);
        let device_type = cursor.read_u8()?;
        let client_hash = cursor.read_u16::<BigEndian>()?;
        MessageType::ChannelAssignmentRequest { device_type, client_hash }
      }
      MessageTypeKind::ChannelAssignmentResponse => {
        let mut cursor = Cursor::new(&value.payload);
        let channel = Channel::from(cursor.read_u8()?);
        let client_hash = cursor.read_u16::<BigEndian>()?;
        MessageType::ChannelAssignmentResponse { channel, client_hash }
      }
      MessageTypeKind::ChannelAssignmentAck => MessageType::ChannelAssignmentAck(),
      MessageTypeKind::ExistingClientRequest => MessageType::ExistingClientRequest(),
      MessageTypeKind::ExistingClientResponse => {
        MessageType::ExistingClientResponse { unknown: value.payload.clone() }
      }
      MessageTypeKind::ClearToSend => MessageType::ClearToSend(),
      MessageTypeKind::NothingToSend => MessageType::NothingToSend(),
      MessageTypeKind::ToggleItemRequest => {
        let mut cursor = Cursor::new(&value.payload);
        let item_code = ParsedEnum::from_raw(cursor.read_u8()?);
        let dummy1 = cursor.read_u8()?;
        MessageType::ToggleItemRequest { item_code, dummy1 }
      }
      MessageTypeKind::StatusUpdate => {
        MessageType::StatusUpdate(StatusUpdateMessage::try_from(value.payload.as_slice())?)
      }
      MessageTypeKind::SetTemperatureRequest => {
        let mut cursor = Cursor::new(&value.payload);
        let temperature = SetTemperature { raw_value: cursor.read_u8()? };
        MessageType::SetTemperatureRequest { temperature }
      },
      MessageTypeKind::SetTimeRequest => {
        let mut cursor = Cursor::new(&value.payload);
        let hour = cursor.read_u8()?;
        let minute = cursor.read_u8()?;
        let time = ProtocolTime::from_hm(hour, minute);
        MessageType::SetTimeRequest { time }
      }
      MessageTypeKind::SettingsRequest => {
        MessageType::SettingsRequest(SettingsRequestMessage::try_from(value.payload.as_slice())?)
      },
      MessageTypeKind::FilterCycles => todo!(),
      MessageTypeKind::InformationResponse => {
        MessageType::InformationResponse(InformationResponseMessage::try_from(value.payload.as_slice())?)
      }
      MessageTypeKind::PreferencesResponse => todo!(),
      MessageTypeKind::SetPreferenceRequest => todo!(),
      MessageTypeKind::FaultLogResponse =>
        MessageType::FaultLogResponse(FaultResponseMessage::try_from(value.payload.as_slice())?),
      MessageTypeKind::ChangeSetupRequest => todo!(),
      MessageTypeKind::GfciTestResponse => todo!(),
      MessageTypeKind::LockRequest => todo!(),
      MessageTypeKind::ConfigurationResponse =>
        MessageType::ConfigurationResponse(ConfigurationResponseMessage::try_from(value.payload.as_slice())?),
      MessageTypeKind::WifiModuleConfigurationResponse => todo!(),
      MessageTypeKind::ToggleTestSettingRequest => todo!(),
    };
    Ok(parsed)
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
        vec![item_code.as_raw(), dummy1],
      MessageType::StatusUpdate(message) =>
        Vec::<u8>::try_from(&message)?,
      MessageType::SetTemperatureRequest { temperature } =>
        vec![temperature.raw_value],
      MessageType::SetTimeRequest { time } => {
        let mut cursor = Cursor::new(Vec::with_capacity(2));
        cursor.write_u16::<BigEndian>(time.as_raw())?;
        cursor.into_inner()
      }
      MessageType::SettingsRequest(message) =>
        Vec::<u8>::from(&message),
      MessageType::FilterCycles { .. } => {
        return Err(PayloadEncodeError::NotSupported)
      }
      MessageType::InformationResponse(message) =>
        Vec::<u8>::try_from(&message)?,
      MessageType::PreferencesResponse(message) =>
        Vec::<u8>::try_from(&message)?,
      MessageType::SetPreferenceRequest(message) =>
        Vec::<u8>::try_from(&message)?,
      MessageType::FaultLogResponse(message) =>
        Vec::<u8>::try_from(&message)?,
      MessageType::ChangeSetupRequest { setup_number } =>
        vec![setup_number],
      MessageType::GfciTestResponse { result } =>
        vec![result.as_raw()],
      MessageType::LockRequest(message) =>
        vec![message.to_u8().unwrap()],
      MessageType::ConfigurationResponse(message) =>
        Vec::<u8>::try_from(&message)?,
      MessageType::WifiModuleConfigurationResponse { mac } =>
        mac.to_vec(),
      MessageType::ToggleTestSettingRequest(message) =>
        vec![message.to_u8().unwrap()],
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

  #[error("Generic error for any malformed or misunderstood data")]
  InvalidData(#[from] anyhow::Error),

  #[error("Utf8-decoding error")]
  Utf8Error(#[from] FromUtf8Error),
}

#[derive(thiserror::Error, Debug)]
pub enum PayloadEncodeError {
  #[error("Generic I/O error")]
  GenericIoError(#[from] io::Error),

  #[error("Generic internal error")]
  GenericError(#[from] anyhow::Error),

  #[error("Message type encoding not yet supported")]
  NotSupported,
}