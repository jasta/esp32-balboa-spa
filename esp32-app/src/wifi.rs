use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use embedded_svc::wifi::{ClientConfiguration, Configuration, Wifi};
use esp_idf_hal::modem::Modem;
use esp_idf_hal::peripheral::Peripheral;
use esp_idf_svc::eventloop::{EspEventLoop, System};
use esp_idf_svc::handle::RawHandle;
use esp_idf_svc::netif::IpEvent;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{EspWifi, WifiEvent, WifiWait};
use esp_idf_svc::wifi_dpp::{EspWifiDpp, QrCode};
use esp_idf_sys::*;
use log::{error, info, warn};
use wifi_module_lib::advertisement::Advertisement;
use wifi_module_lib::wifi_manager::{StaAssociationError, WifiDppBootstrapped, WifiManager};

const STARTED_TIMEOUT: Duration = Duration::from_secs(20);

const CONNECT_TIMEOUT: Duration = Duration::from_secs(120);

pub struct EspWifiManager<'w> {
  wifi: EspWifi<'w>,
  event_loop: EspEventLoop<System>,
  advertisement: Advertisement,
}

impl<'w> EspWifiManager<'w> {
  pub fn new(
      modem: impl Peripheral<P = Modem> + 'static,
      event_loop: EspEventLoop<System>,
      nvs: EspDefaultNvsPartition,
      advertised_name: String,
  ) -> Result<Self, EspError> {
    let wifi = EspWifi::new(modem, event_loop.clone(), Some(nvs))?;
    let mac = wifi.sta_netif().get_mac()?;
    let advertisement = Advertisement::new(
        advertised_name,
        mac);
    Ok(Self {
      wifi,
      event_loop,
      advertisement,
    })
  }
}

impl<'w> WifiManager<'w> for EspWifiManager<'w> {
  type Error = EspError;

  type Credentials = ClientConfiguration;

  type DppBootstrapped<'d> = EspDppBootstrappedAdapter<'d, 'w>
  where 'w: 'd, Self: 'd;

  fn advertisement(&self) -> &Advertisement {
    &self.advertisement
  }

  fn init(&mut self) -> Result<(), Self::Error> {
    self.wifi.start()?;

    let completed = wifi_wait_ext(&WifiWait::new(&self.event_loop)?, Some(STARTED_TIMEOUT), || {
      self.wifi.is_started()
    })?;
    if completed {
      Ok(())
    } else {
      error!("Wi-Fi driver failed to start after {}s", STARTED_TIMEOUT.as_secs());
      Err(EspError::from_infallible::<ESP_ERR_INVALID_STATE>())
    }
  }

  fn get_sta_network_name(&self) -> Result<Option<String>, Self::Error> {
    match self.wifi.get_configuration()? {
      Configuration::Client(c) => Ok(get_network_name(&c)),
      _ => Ok(None),
    }
  }

  fn dpp_bootstrap(&mut self) -> Result<Self::DppBootstrapped<'_>, Self::Error> {
    let bootstrapped = self.wifi.dpp_generate_qrcode(&[6], None, None)?;
    Ok(EspDppBootstrappedAdapter {
      bootstrapped,
    })
  }

  fn store_credentials(&mut self, credentials: Self::Credentials) -> Result<String, Self::Error> {
    let network_name = get_network_name(&credentials)
        .expect("Must have a valid target network!");
    self.wifi.set_configuration(&Configuration::Client(credentials))?;
    Ok(network_name)
  }

  fn sta_connect(&mut self) -> Result<(), StaAssociationError> {
    let result = self.do_sta_connect()
        .map_err(|e| StaAssociationError::SystemError(e.to_string()))?;
    match result {
      None => Ok(()),
      Some(e) => Err(e),
    }
  }

  fn wait_while_connected(&mut self) -> Result<(), Self::Error> {
    WifiWait::new(&self.event_loop)?.wait(|| !self.wifi.is_connected().unwrap());
    Ok(())
  }
}

impl<'w> EspWifiManager<'w> {
  /// Perform STA connect using our own internal state machine for more precise control
  /// over error outputs.
  fn do_sta_connect(&mut self) -> Result<Option<StaAssociationError>, EspError> {
    let (tx, rx) = channel();
    let tx_for_wifi = tx.clone();
    let tx_for_ip = tx;
    let wifi_sub = self.event_loop.subscribe(move |event: &WifiEvent| {
      Self::handle_wifi_event(&tx_for_wifi, event);
    })?;
    let netif_handle = RawHandleSend(self.wifi.sta_netif().handle());
    let ip_sub = self.event_loop.subscribe(move |event: &IpEvent| {
      Self::handle_ip_event(&tx_for_ip, &netif_handle, event);
    })?;

    let start_time = Instant::now();

    let mut associated = false;
    self.wifi.connect()?;

    let result = loop {
      match rx.recv().unwrap() {
        SystemEvent::Wifi(wifi) => match wifi {
          WifiEvent::StaConnected => {
            associated = true
          }
          WifiEvent::StaDisconnected => {
            // TODO: esp_wifi gives us the reason code, but it's lost by esp-idf-svc.  Let's
            // patch upstream to get it back...
            break Some(StaAssociationError::AssociationFailed);
          }
          _ => {},
        }
        SystemEvent::Ip(ip) => match ip {
          IpEvent::DhcpIpAssigned(_) |
          IpEvent::DhcpIp6Assigned(_) => {
            break None;
          }
          _ => {},
        }
      }

      if start_time.elapsed() > CONNECT_TIMEOUT {
        let err = if associated {
          StaAssociationError::DhcpTimedOut
        } else {
          StaAssociationError::AssociationTimedOut
        };
        break Some(err)
      }
    };

    drop(wifi_sub);
    drop(ip_sub);

    Ok(result)
  }

  fn handle_ip_event(tx: &Sender<SystemEvent>, handle: &RawHandleSend, event: &IpEvent) {
    if event.is_for_handle(handle.0) {
      let _ = tx.send(SystemEvent::Ip(*event));
    }
  }

  fn handle_wifi_event(tx: &Sender<SystemEvent>, event: &WifiEvent) {
    let _ = tx.send(SystemEvent::Wifi(*event));
  }
}

pub struct EspDppBootstrappedAdapter<'d, 'w> {
  bootstrapped: EspWifiDpp<'d, 'w, QrCode>,
}

impl<'d, 'w> WifiDppBootstrapped<'d, 'w> for EspDppBootstrappedAdapter<'d, 'w> {
  type Error = EspError;

  type Credentials = ClientConfiguration;
  
  fn get_qr_code(&self) -> &str {
    &self.bootstrapped.get_bootstrapped_data().0
  }

  fn listen_then_wait(self) -> Result<Self::Credentials, Self::Error> {
    info!("Waiting for user to scan code...");
    let mut bootstrapped = self.bootstrapped;
    loop {
      let listener = bootstrapped.start_listen()?;
      match listener.wait_for_credentials() {
        Ok(c) => return Ok(c),
        Err(e) => {
          warn!("DPP error: {e}, retrying...");
          bootstrapped = listener.attempt_retry()
              .expect("Please patch esp-idf with esp_supp_dpp_start_listen fix!");
        }
      }
    }
  }
}

fn get_network_name(config: &ClientConfiguration) -> Option<String> {
  if !config.ssid.is_empty() {
    Some(config.ssid.as_str().to_owned())
  } else {
    config.bssid.map(|_| "<hidden>".to_owned())
  }
}

struct RawHandleSend(*mut esp_netif_t);
unsafe impl Send for RawHandleSend {}

enum SystemEvent {
  Ip(IpEvent),
  Wifi(WifiEvent),
}

/// Adds the ability to extra an EspError from the matcher fn rather than just unwrap which
/// would cause a global panic for us.
fn wifi_wait_ext(
    wait: &WifiWait,
    timeout: Option<Duration>,
    matcher: impl Fn() -> Result<bool, EspError>) -> Result<bool, EspError> {
  let err = Mutex::new(None);
  let matcher_wrapper = || {
    matcher().unwrap_or_else(|e| {
      let mut err_store = err.lock().unwrap();
      *err_store = Some(e);
      true
    })
  };

  let retval = if let Some(timeout) = timeout {
    wait.wait_with_timeout(timeout, matcher_wrapper)
  } else {
    wait.wait(matcher_wrapper);
    true
  };

  match err.into_inner().unwrap() {
    None => Ok(retval),
    Some(e) => Err(e),
  }
}
