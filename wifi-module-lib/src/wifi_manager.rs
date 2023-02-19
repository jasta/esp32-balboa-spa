use std::fmt::{Debug, Display};
use crate::advertisement::Advertisement;

pub trait WifiManager<'w> {
  /// Unrecoverable error type that indicates a failure out of band of Wi-Fi (e.g. the driver
  /// is in a bad state or out of memory).
  type Error: Debug + Display;

  /// Type of the associated DPP flow manager.  Held as a separate type since there is usually
  /// a mutable reference overlap with the lifecycle of the WifiManager.
  type DppBootstrapper<'d>: WifiDppBootstrapper<'d, 'w, Error = Self::Error>
  where 'w: 'd, Self: 'd;

  /// Provides the advertisement we'll use when peers are trying to discover us.
  fn advertisement(&self) -> &Advertisement;

  /// Initialize driver and load stored credentials.
  fn init(&mut self) -> Result<(), Self::Error>;

  /// Check if stored credentials are present and valid, then return the SSID for display
  /// purposes.  If connecting to a BSSID, Some("<hidden>") will be used.
  fn get_sta_network_name(&self) -> Result<Option<String>, Self::Error>;

  /// Generate a QR code that Wi-Fi Easy Connect clients can use to send us credentials.  This
  /// method assumes [Self::get_sta_network_name] was None.
  fn create_bootstrapper(&mut self) -> Result<Self::DppBootstrapper<'_>, Self::Error>;

  /// Perform a blocking station-mode connect operation then block the calling thread while we
  /// remain connected.  This is designed to be run in a dedicated thread that just loops to
  /// reconnect.
  fn sta_connect(&mut self) -> Result<(), StaAssociationError>;

  /// Perform a blocking wait until we are disconnected.  Expected that the caller will just
  /// loop forever interleaving between [Self::sta_connect] and [Self::wait_while_connected]
  /// and updating any internal state accordingly to show the user.
  fn wait_while_connected(&mut self) -> Result<(), Self::Error>;
}

pub trait WifiDppBootstrapper<'d, 'w> {
  type Error: Debug + Display;

  type DppBootstrapped<'b>: WifiDppBootstrapped<'b, Error = Self::Error>
  where Self: 'b;

  fn bootstrap(&mut self) -> Result<Self::DppBootstrapped<'_>, Self::Error>;
}

pub trait WifiDppBootstrapped<'b> {
  type Error: Debug + Display;

  /// Access the QR code associated with this bootstrapped DPP session.
  fn get_qr_code(&self) -> &str;

  /// Listen for Wi-Fi Easy Connect credentials.  Store them upon success which enables us to move
  /// to [Self::sta_connect] next.  This method blocks until the credentials are available or
  /// a non-recoverable error occurs.  Returns the STA target network name as a convenience.
  fn listen_then_wait(self) -> Result<String, Self::Error>;
}

#[derive(thiserror::Error, Debug, PartialEq, Eq, Clone)]
pub enum DppListenError {
  #[error("Unknown underlying system error: {0}")]
  SystemError(String),
}

#[derive(thiserror::Error, Debug, PartialEq, Eq, Clone)]
pub enum StaAssociationError {
  #[error("Association timed out")]
  AssociationTimedOut,

  #[error("Association failed")]
  AssociationFailed,

  #[error("DHCP failed to acquire IP")]
  DhcpTimedOut,

  #[error("Unknown underlying system error: {0}")]
  SystemError(String)
}
