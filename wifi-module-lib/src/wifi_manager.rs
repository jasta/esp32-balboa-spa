use crate::advertisement::Advertisement;

pub trait WifiManager {
  fn advertisement(&self) -> Advertisement;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WifiEvent {
  Connected,
  Connecting,
}
