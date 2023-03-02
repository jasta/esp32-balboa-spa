use std::fmt::{Debug, Display};
use crate::advertisement::Advertisement;

pub trait WifiManager<'w> {
  /// Unrecoverable error type that indicates a failure out of band of Wi-Fi (e.g. the driver
  /// is in a bad state or out of memory).
  type Error: Debug + Display;

  /// Opaque credentials type that carries the returned credentials from [Self::DppBootstrapped]
  /// to the wifi manager.
  type Credentials;

  /// Type of the associated DPP flow manager.  Held as a separate type since there is usually
  /// a mutable reference overlap with the lifecycle of the WifiManager.
  type DppBootstrapped<'d>: WifiDppBootstrapped<'d, 'w, Credentials = Self::Credentials, Error = Self::Error>
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
  fn dpp_bootstrap(&mut self) -> Result<Self::DppBootstrapped<'_>, Self::Error>;

  /// Store credentials provided by [Self::dpp_bootstrap].  Returns the network name
  /// as a convenience to avoid calling [Self::get_sta_network_name] again.
  fn store_credentials(&mut self, credentials: Self::Credentials) -> Result<String, Self::Error>;

  /// Perform a blocking station-mode connect operation then block the calling thread while we
  /// remain connected.  This is designed to be run in a dedicated thread that just loops to
  /// reconnect.
  fn sta_connect(&mut self) -> Result<(), StaAssociationError>;

  /// Perform a blocking wait until we are disconnected.  Expected that the caller will just
  /// loop forever interleaving between [Self::sta_connect] and [Self::wait_while_connected]
  /// and updating any internal state accordingly to show the user.
  fn wait_while_connected(&mut self) -> Result<(), Self::Error>;
}

pub trait WifiDppBootstrapped<'d, 'w> {
  type Error: Debug + Display;

  type Credentials;

  /// Access the QR code associated with this bootstrapped DPP session.
  fn get_qr_code(&self) -> &str;

  /// Listen for Wi-Fi Easy Connect credentials.  This method blocks until the credentials are
  /// available or a non-recoverable error occurs.
  fn listen_then_wait(self) -> Result<Self::Credentials, Self::Error>;
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
