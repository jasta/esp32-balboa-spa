use std::borrow::Borrow;
use esp_idf_hal::peripheral::Peripheral;
use esp_idf_hal::modem::Modem;
use esp_idf_svc::eventloop::{EspEventLoop, System};
use esp_idf_svc::wifi::{EspWifi, WifiEvent, WifiWait};
use embedded_svc::wifi::{AccessPointConfiguration, ClientConfiguration, Configuration, Wifi};
use esp_idf_svc::netif::{EspNetif, EspNetifWait, IpEvent};
use std::time::{Duration, Instant};
use std::net::Ipv4Addr;
use std::ops::Deref;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::sync::mpsc::{channel, sync_channel};
use anyhow::anyhow;
use esp_idf_svc::handle::RawHandle;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_sys::{esp, EspError};
use log::{debug, error, info, warn};
use wifi_module_lib::advertisement::Advertisement;
use wifi_module_lib::wifi_manager::{ConnectionError, StaAssociationError, WifiManager};

const STARTED_TIMEOUT: Duration = Duration::from_secs(20);

const CONNECT_TIMEOUT: Duration = Duration::from_secs(120);

struct EspWifiManager<'a> {
  wifi: EspWifi<'a>,
  event_loop: EspEventLoop<System>,
  advertisement: Advertisement,
}

impl<'a> EspWifiManager<'a> {
  pub fn new(
      modem: impl Peripheral<P = Modem> + 'static,
      event_loop: &EspEventLoop<System>,
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
      event_loop: event_loop.clone(),
      advertisement,
    })
  }
}

impl WifiManager for EspWifiManager {
  type Error = EspError;

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
      Err(EspError::from_infallible::<esp_idf_sys::ESP_ERR_INVALID_STATE>())
    }
  }

  fn get_sta_network_name(&self) -> Result<Option<String>, Self::Error> {
    match self.wifi.get_configuration()? {
      Configuration::Client(c) => {
        if !c.ssid.is_empty() {
          Ok(Some(c.ssid.into()))
        } else {
          Ok(c.bssid.map(|_| "<hidden>".to_owned()))
        }
      }
      _ => Ok(None),
    }
  }

  fn dpp_generate_qr(&self) -> Result<String, Self::Error> {
    todo!()
  }

  fn dpp_listen_then_wait(&mut self) -> Result<String, Self::Error> {
    todo!()
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
    WifiWait::new(&self.event_loop)?.wait(|| !wifi.is_connected().unwrap());
    Ok(())
  }
}

impl<'a> EspWifiManager<'a> {
  /// Perform STA connect using our own internal state machine for more precise control
  /// over error outputs.
  fn do_sta_connect(&mut self) -> Result<Option<StaAssociationError>, EspError> {
    let (tx, rx) = channel();
    let wifi_sub = self.event_loop.subscribe(move |event: &WifiEvent| {
      let _ = tx.send(SystemEvent::Wifi(*event));
    })?;
    let netif_handle = self.wifi.sta_netif().handle();
    let ip_sub = self.event_loop.subscribe(move |event: &IpEvent| {
      if event.is_for_handle(netif_handle) {
        let _ = tx.send(SystemEvent::Ip(*event));
      }
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
}

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
      let err_store = is_started_err.lock().unwrap();
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
