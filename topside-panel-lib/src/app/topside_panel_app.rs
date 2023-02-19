use std::io::{Read, Write};
use std::marker::PhantomData;
use std::thread;
use std::time::Duration;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::PixelColor;
use log::{info};
use lvgl::Color;
use common_lib::bus_transport::{BusTransport};
use common_lib::transport::Transport;
use wifi_module_lib::wifi_manager::WifiManager;
use wifi_module_lib::wifi_module_client::WifiModuleClient;
use crate::network::topside_panel_client::TopsidePanelClient;
use crate::view::lcd_device::LcdDevice;
use crate::view::ui_handler::UiHandler;

pub struct TopsidePanelApp<R, W, T, LCD, WIFI> {
  transport: T,
  _phantom_rw: PhantomData<(R, W)>,
  lcd_device: LCD,
  wifi_manager: Option<WIFI>,
}

impl<R, W, T, LCD, WIFI> TopsidePanelApp<R, W, T, LCD, WIFI>
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
    T: Transport<R, W>,
    LCD: LcdDevice + Send + 'static,
    LCD::Display: DrawTarget,
    <<LCD as LcdDevice>::Display as DrawTarget>::Color: PixelColor + From<Color>,
    WIFI: WifiManager<'static> + Send + 'static,
{
  pub fn new(
      transport: T,
      lcd_device: LCD,
      wifi_manager: Option<WIFI>
  ) -> Self {
    Self {
      transport,
      _phantom_rw: PhantomData,
      lcd_device,
      wifi_manager,
    }
  }

  pub fn run_loop(self) -> anyhow::Result<()> {
    let (
      bus_switch,
      topside_transport,
      wifi_client
    ) = match self.wifi_manager {
      None => {
        (None, HomogenousTransport::new(self.transport), None)
      },
      Some(wifi_manager) => {
        let mut switch = BusTransport::new_switch(self.transport);
        let topside_transport = HomogenousTransport::new(switch.new_connection());
        let wifi = WifiModuleClient::new(
          switch.new_connection(),
          wifi_manager);
        (Some(switch), topside_transport, Some(wifi))
      }
    };

    let topside_client = TopsidePanelClient::new(topside_transport);

    if let Some(bus_switch) = bus_switch {
      bus_switch.start();
    }

    let (topside_control, topside_events, topside_runner) =
        topside_client.into_runner();
    let topside_thread = thread::Builder::new()
        .name("TopsideRunner".to_owned())
        .spawn(move || topside_runner.run_loop().unwrap())?;

    if let Some(wifi_client) = wifi_client {
      let (wifi_events, wifi_runner) = wifi_client.into_runner()?;
      let wifi_thread = thread::Builder::new()
          .name("WifiRunner".to_owned())
          .spawn(move || wifi_runner.run_loop().unwrap())?;

      let control_for_relay = topside_control.clone();
      let event_relay = thread::Builder::new()
          .name("EventRelay".to_owned())
          .spawn(move || {
            while let Ok(wifi_event) = wifi_events.try_recv_latest() {
              if let Some(wifi_event) = wifi_event {
                control_for_relay.send_wifi_model(wifi_event);
              }
            }
          })?;
    }

    let ui_thread = thread::Builder::new()
        .name("UI Thread".to_owned())
        .spawn(move || {
          let handler = UiHandler::new(
              self.lcd_device,
              topside_control,
              topside_events);
          handler.run_loop().unwrap()
        })?;

    ui_thread.join().unwrap();

    Ok(())
  }
}

type HomogenousRead = Box<dyn Read + Send + 'static>;
type HomogenousWrite = Box<dyn Write + Send + 'static>;

struct HomogenousTransport {
  reader: HomogenousRead,
  writer: HomogenousWrite,
}

impl HomogenousTransport {
  pub fn new<R, W, T>(transport: T) -> Self
  where
      R: Read + Send + 'static,
      W: Write + Send + 'static,
      T: Transport<R, W>
  {
    let (reader, writer) = transport.split();
    Self {
      reader: Box::new(reader),
      writer: Box::new(writer),
    }
  }
}

impl Transport<HomogenousRead, HomogenousWrite> for HomogenousTransport {
  fn split(self) -> (HomogenousRead, HomogenousWrite) {
    (self.reader, self.writer)
  }
}