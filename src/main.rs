use anyhow::anyhow;
use esp_idf_sys as _;

use esp_idf_hal::modem::{Modem};
use esp_idf_hal::peripheral::Peripheral;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::eventloop::{EspEventLoop, System};

use esp_idf_svc::wifi::EspWifi;
use esp_idf_sys::EspError;

#[toml_cfg::toml_config]
pub struct Config {
  #[default("")]
  wifi_ssid: &'static str,

  #[default("")]
  wifi_psk: &'static str,
}

fn main() -> anyhow::Result<()> {
  esp_idf_sys::link_patches();

  esp_idf_svc::log::EspLogger::initialize_default();

  let peripherals = Peripherals::take()
      .ok_or_else(|| anyhow!("Unable to take peripherals"))?;
  let event_loop = EspEventLoop::take()?;

  let _ = connect_wifi(peripherals.modem, event_loop.clone(), CONFIG.wifi_ssid, CONFIG.wifi_psk);

  println!("Hello, world!");

  Ok(())
}

pub fn connect_wifi(
    modem: impl Peripheral<P = Modem>,
    event_loop: EspEventLoop<System>,
    ssid: &str,
    psk: &str
) -> Result<(), EspError> {
  let mut wifi = EspWifi::new(modem, event_loop, None)?;

  wifi.scan();

  Ok(())
}
