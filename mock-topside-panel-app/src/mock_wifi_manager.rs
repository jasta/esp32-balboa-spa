use wifi_module_lib::advertisement::Advertisement;
use wifi_module_lib::wifi_manager::WifiManager;

pub struct MockWifiManager;

impl WifiManager for MockWifiManager {
  fn advertisement(&self) -> Advertisement {
    Advertisement::fake_balboa()
  }
}
