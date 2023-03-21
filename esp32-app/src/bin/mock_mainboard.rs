use std::time::Duration;

use anyhow::anyhow;
use esp_idf_hal::prelude::*;
use esp_idf_svc::eventloop::EspEventLoop;
use esp_idf_sys as _;
use esp_idf_sys::esp_app_desc;
use log::{info, warn};
use mock_mainboard_lib::channel_manager::CtsEnforcementPolicy;
use mock_mainboard_lib::main_board::MainBoard;
use topside_panel_lib::app::status_printer::BoardMonitor;
use esp_app::esp_status_printer::EspStatusPrinter;
use esp_app::esp_uart_transport::EspUartTransport;

esp_app_desc!();

fn main() -> anyhow::Result<()> {
  esp_idf_sys::link_patches();

  esp_idf_svc::log::EspLogger::initialize_default();

  let peripherals = Peripherals::take()
      .ok_or_else(|| anyhow!("Unable to take peripherals"))?;
  let event_loop = EspEventLoop::take()?;

  let transport = EspUartTransport::new(
      peripherals.uart1,
      peripherals.pins.gpio0,
      peripherals.pins.gpio3,
      Some(peripherals.pins.gpio1),
      None)?;

  info!("UART transport initialized");

  let logic = MainBoard::new(transport)
      .set_init_delay(Duration::from_secs(10))
      .set_clear_to_send_policy(CtsEnforcementPolicy::Never, Duration::from_millis(20));
  let (shutdown_handle, runner) = logic.into_runner();

  thread::spawn(move || {
    EspStatusPrinter.run_loop()
  });

  info!("Main board setup complete, starting...");
  if let Err(e) = runner.run_loop() {
    panic!("Run loop exited: {e:?}");
  }
  warn!("Exiting seemingly by request, though not sure how?");

  drop(shutdown_handle);

  Ok(())
}
