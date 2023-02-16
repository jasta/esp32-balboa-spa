use std::fmt::Debug;
use crate::wifi_manager::StaAssociationError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewModel {
  pub mode: Mode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
  /// Device is starting up and determining state
  Initializing,

  /// Critical configuration error, must be power cycled or factory reset.  An internal debug
  /// string is provided for context/debuggability but is not meant to be human/screen friendly.
  UnrecoverableError(String),

  /// Needs to be provisioned onto the Wi-Fi network, provides context for helping guide
  /// the user through this flow
  NeedsProvisioning(UnprovisionedModel),

  /// Persistent difficulties contacting the provisioned Wi-Fi network, likely needs user
  /// intervention to fix (like fixing their Wi-Fi AP or moving it closer)
  TroubleAssociating(TroubleAssociatingModel),

  /// Nominal cases, including attempting to associate.  Doesn't mean steady state, but it does
  /// mean we can show a healthy looking UI.
  Nominal(NominalModel),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnprovisionedModel {
  pub params: ProvisioningParams,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TroubleAssociatingModel {
  pub error: StaAssociationError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvisioningParams {
  /// Convert to an image and have a compatible phone use the Wi-Fi Easy Connect (DPP) feature
  /// to scan the barcode which delivers network credentials to us.
  pub dpp_qr_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NominalModel {
  /// Name of the target network we're connecting/connected to.
  pub network_name: String,

  /// Actual state of the Wi-Fi connection.
  pub connection_state: ConnectionState,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ConnectionState {
  /// Not currently associated or retrying actively, but will try again shortly.  If too
  /// many subsequent failures are reached, the overall mode will transition to
  /// [TroubleAssociatingModel].  The UI should remain optimistic that association will occur
  /// until then.
  NotAssociated,

  /// Trying to associate actively.
  Associating,

  /// Associated and presumably nominally connected.
  Associated,

  /// Connected and with an IP address assigned (though not round trip confirmed to be routable
  /// anywhere).
  Connected,
}

impl Default for ConnectionState {
  fn default() -> Self {
    ConnectionState::NotAssociated
  }
}