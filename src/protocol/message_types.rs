use std::io;
use std::io::Cursor;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use measurements::Temperature;
use num_derive::FromPrimitive;
use num_derive::ToPrimitive;
use time::Time;
use crate::protocol::message::{Channel, Message, MessageType, MessageTypeHolder};

#[derive(Debug, Clone)]
#[repr(u8)]
pub enum MessageType2 {
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
  StatusUpdate {
    v1: StatusUpdateResponseV1,
    v2: Option<StatusUpdateResponseV2>,
    v3: Option<StatusUpdateResponseV3>,
  } = 0x13,
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
  SetWifiSettingsRequest = 0x92,
  WifiModuleConfigurationResponse = 0x94,
  ToggleTestSettingRequest = 0xe0,
  UnknownError1 = 0xe1,
  UnknownError2 = 0xf0,
}

#[derive(FromPrimitive, ToPrimitive, Debug, Copy, Clone)]
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
pub struct StatusUpdateResponseV1 {
  spa_state: ParsedEnum<SpaState, u8>,
  init_mode: ParsedEnum<InitializationMode, u8>,
  current_temperature: Option<Temperature>,
  time: Time,
  heating_mode: ParsedEnum<HeatingMode, u8>,
  reminder_type: ParsedEnum<ReminderType, u8>,
  hold_timer: Option<Time>,
  filter_mode: ParsedEnum<FilterMode, u8>,
  panel_locked: bool,
  temperate_range: TemperatureRange,
  needs_heat: bool,
  heating_state: ParsedEnum<HeatingState, u8>,
  mister_on: ParsedEnum<bool, u8>,
  set_temperature: Temperature,
  pump_status: Vec<ParsedEnum<PumpStatus, u8>>
  circulation_pump_on: ParsedEnum<bool, u8>,
  blower_status: ParsedEnum<RelayStatus, u8>,
  light_status: Vec<ParsedEnum<RelayStatus, u8>>,
  reminder_set: bool,
  notification_set: ParsedEnum<bool, u8>,
}

#[derive(Debug, Clone)]
pub struct ParsedEnum<TYPE, PRIMITIVE> {
  parsed: Option<TYPE>,
  raw: PRIMITIVE,
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

#[derive(Debug, Clone)]
pub struct StatusUpdateResponseV2 {

}

#[derive(Debug, Clone)]
pub struct StatusUpdateResponseV3 {

}

#[derive(Debug, Clone)]
pub struct ChannelAssignmentRequest {
  pub device_type: u8,
  pub client_hash: u16,
}

impl TryFrom<&Message> for ChannelAssignmentRequest {
  type Error = PayloadParseError;

  fn try_from(value: &Message) -> Result<Self, Self::Error> {
    if value.message_type != MessageTypeHolder::Known(MessageType::ChannelAssignmentRequest) {
      return Err(PayloadParseError::InvalidMessageType);
    }
    let mut cursor = Cursor::new(&value.payload);
    let device_type = cursor.read_u8()?;
    let client_hash = cursor.read_u16::<BigEndian>()?;
    Ok(Self { device_type, client_hash })
  }
}

impl From<ChannelAssignmentRequest> for Vec<u8> {
  fn from(value: ChannelAssignmentRequest) -> Self {
    let mut cursor = Cursor::new(Vec::new());
    cursor.write_u8(value.device_type).unwrap();
    cursor.write_u16::<BigEndian>(value.client_hash).unwrap();
    cursor.into_inner()
  }
}

#[derive(thiserror::Error, Debug, Clone)]
pub enum PayloadParseError {
  #[error("Wrong message type")]
  InvalidMessageType,

  #[error("Unexpected EOF")]
  UnexpectedEof(#[from] io::Error)
}