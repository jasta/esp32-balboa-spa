use std::time::Duration;

#[derive(Debug, Copy, Clone)]
pub struct ProtocolTime {
  duration: Duration,
  hour: u8,
  minute: u8,
}

impl ProtocolTime {
  pub fn from_duration(duration: Duration) -> Result<Self, ProtocolTimeError> {
    Self::try_from(duration)
  }

  pub fn from_hm(hour: u8, minute: u8) -> Self {
    Self::from_duration(Duration::from_secs(minute * 60 + hour * 60 * 60)).unwrap()
  }

  pub fn as_duration(&self) -> Duration {
    self.duration
  }

  pub fn as_raw(&self) -> u16 {
    (self.hour << 8) & 0xff | self.minute & 0xff
  }

  pub fn to_minutes(&self) -> u8 {
    self.minute
  }
}

impl TryFrom<Duration> for ProtocolTime {
  type Error = ProtocolTimeError;

  fn try_from(value: Duration) -> Result<Self, Self::Error> {
    let total_minutes = value.as_secs() / 60;
    let hour = total_minutes / 60;
    let minute = total_minutes % 60;
    if hour >= 24 {
      return Err(ProtocolTimeError::ExceedsSingleDay);
    }
    Ok(Self { duration: value, hour, minute })
  }
}

#[derive(thiserror::Error, Debug)]
pub enum ProtocolTimeError {
  #[error("Time specified is too long")]
  ExceedsSingleDay,
}
