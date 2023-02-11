use std::io::{Read, Write};
use std::mem::{discriminant, transmute};
use std::thread;
use log::info;
use common_lib::bus_transport::{BusSwitch, BusTransport};
use common_lib::transport::Transport;
use wifi_module_lib::advertisement::Advertisement;
use wifi_module_lib::wifi_manager::WifiManager;
use wifi_module_lib::wifi_module_client::{WifiHardware, WifiModuleClient};
use crate::network::topside_panel_client::TopsidePanelClient;
use crate::view::lcd_device::LcdDevice;
use crate::view::ui_handler::UiHandler;

pub struct TopsidePanelApp<T, LCD, WIFI> {
  transport: T,
  lcd_device: LCD,
  wifi_manager: Option<WIFI>,
}

impl<R, W, T, LCD, WIFI> TopsidePanelApp<T, LCD, WIFI>
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
    T: Transport<R, W>,
    LCD: LcdDevice,
    WIFI: WifiManager,
{
  pub fn new(
      transport: T,
      lcd_device: LCD,
      wifi_manager: Option<WIFI>
  ) -> Self {
    Self {
      transport,
      lcd_device,
      wifi_manager,
    }
  }

  pub fn run_loop(self) {
    let (bus_switch, topside_client, wifi_client) = match self.wifi_hardware {
      None => {
        (None, TopsidePanelClient::new(self.transport), None)
      },
      Some(wifi_manager) => {
        let mut switch = BusTransport::new_switch(self.transport);
        let topside = TopsidePanelClient::new(switch.new_connection());
        let wifi = WifiModuleClient::new(
          switch.new_connection(),
          wifi_manager);
        (Some(switch), topside, Some(wifi))
      }
    };

    bus_switch.start();

    let (topside_control, topside_events, topside_runner) = topside.into_runner();
    let topside_thread = thread::Builder::new()
        .name("Topside Thread".to_owned())
        .spawn(move || topside_runner.run_loop().unwrap())?;

    if let Some(wifi_client) = wifi_client {
      let wifi_runner = wifi_client.into_runner()?;
      let wifi_thread = thread::Builder::new()
          .name("Wifi Thread".to_owned())
          .spawn(move || wifi_runner.run_loop().unwrap())?;
    }

    let ui_thread = thread::Builder::new()
        .name("UI Thread".to_owned())
        .spawn(move || {
          let handler = UiHandler::new(self.lcd_device, topside_control, topside_events);
          handler.run_loop().unwrap()
        })?;

    ui_thread.join().unwrap();

    info!("Window shut down, requesting graceful shutdown...");
    thread::spawn(|| {
      thread::sleep(GRACEFUL_SHUTDOWN_PERIOD);
      panic!("Graceful shutdown expired timeout...");
    });

    topside_thread.join().unwrap();
  }
}
