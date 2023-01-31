use log::info;

/// No idea what this actually is, but it's what the TP800 panel sends as its device type
/// so let's copy it.
const DEFAULT_DEVICE_TYPE: u8 = 0x2;

#[derive(Debug, Clone)]
pub struct ClientIdent {
  pub device_type: u8,
  pub client_hash: u16,
}

impl Default for ClientIdent {
  fn default() -> Self {
    let client_hash = rand::random();
    info!("Initializing with client_hash: {client_hash:02X}");
    Self {
      device_type: DEFAULT_DEVICE_TYPE,
      client_hash,
    }
  }
}
