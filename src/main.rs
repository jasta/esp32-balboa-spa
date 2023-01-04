use std::thread;
use std::time::Duration;

use anyhow::anyhow;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::eventloop::EspEventLoop;
use esp_idf_sys as _;
use log::info;

mod echo_server;
mod wifi;

fn main() -> anyhow::Result<()> {
  esp_idf_sys::link_patches();

  esp_idf_svc::log::EspLogger::initialize_default();

  let peripherals = Peripherals::take()
      .ok_or_else(|| anyhow!("Unable to take peripherals"))?;
  let event_loop = EspEventLoop::take()?;

  let wifi = wifi::connect_wifi(peripherals.modem, event_loop.clone(), SSID, PASS)?;
  let server = echo_server::start_rs485_echo_server()?;

  println!("Hello, world!");

  for tick in 0.. {
    println!("Tick #{tick}...");
    thread::sleep(Duration::from_millis(1000));
  }

  drop(server);
  drop(wifi);

  Ok(())
}
