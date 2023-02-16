use anyhow::anyhow;
use wifi_module_lib::advertisement::Advertisement;
use wifi_module_lib::wifi_manager::{StaAssociationError, WifiManager};

pub struct MockWifiManager {
  advertisement: Advertisement,
}

impl MockWifiManager {
  pub fn new() -> Self {
    Self {
      advertisement: Advertisement::fake_balboa(),
    }
  }
}

impl WifiManager for MockWifiManager {
  type Error = anyhow::Error;

  fn advertisement(&self) -> &Advertisement {
    &self.advertisement
  }

  fn init(&mut self) -> Result<(), Self::Error> {
    Ok(())
  }

  fn get_sta_network_name(&self) -> Result<Option<String>, Self::Error> {
    Ok(Some("mynetwork".to_owned()))
  }

  fn dpp_generate_qr(&mut self) -> Result<String, Self::Error> {
    Err(anyhow!("Not implemented"))
  }

  fn dpp_listen_then_wait(&mut self) -> Result<String, Self::Error> {
    Err(anyhow!("Not implemented"))
  }

  fn sta_connect(&mut self) -> Result<(), StaAssociationError> {
    Ok(())
  }

  fn wait_while_connected(&mut self) -> Result<(), Self::Error> {
    loop {
      // Never exit...
    }
  }
}
