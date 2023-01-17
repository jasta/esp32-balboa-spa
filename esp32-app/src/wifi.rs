use esp_idf_hal::peripheral::Peripheral;
use esp_idf_hal::modem::Modem;
use esp_idf_svc::eventloop::{EspEventLoop, System};
use esp_idf_svc::wifi::EspWifi;
use embedded_svc::wifi::{AccessPointConfiguration, ClientConfiguration, Configuration, Wifi};
use esp_idf_svc::netif::{EspNetif, EspNetifWait};
use std::time::Duration;
use std::net::Ipv4Addr;
use anyhow::anyhow;
use log::info;

pub fn connect_wifi(
    modem: impl Peripheral<P = Modem> + 'static,
    event_loop: EspEventLoop<System>,
    ssid: &str,
    psk: &str
) -> anyhow::Result<EspWifi<'static>> {
  let mut wifi = EspWifi::new(modem, event_loop.clone(), None)?;

  let config = Configuration::Mixed(
    ClientConfiguration {
      ssid: ssid.into(),
      password: psk.into(),
      ..Default::default()
    },
    AccessPointConfiguration {
      ssid: "esp32-test".into(),
      channel: 1,
      ..Default::default()
    }
  );
  wifi.set_configuration(&config)?;
  wifi.start()?;
  wifi.connect()?;

  let wait_result = EspNetifWait::new::<EspNetif>(wifi.sta_netif(), &event_loop)?
      .wait_with_timeout(Duration::from_secs(20), || {
        wifi.is_connected().unwrap() && wifi.sta_netif().get_ip_info().unwrap().ip != Ipv4Addr::new(0, 0, 0, 0)
      });
  if !wait_result {
    return Err(anyhow!("WiFi connection or DHCP lease timed out"));
  }

  let ip_info = wifi.sta_netif().get_ip_info()?;
  info!("WiFi DHCP info: {:?}", ip_info);

  Ok(wifi)
}
