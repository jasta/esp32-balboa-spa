use std::thread;
use std::time::Duration;
use log::info;
use common_lib::transport::StdTransport;
use mock_mainboard_lib::channel_manager::CtsEnforcementPolicy;
use mock_mainboard_lib::main_board::MainBoard;
use std::io::Write;
use clap::Parser;
use mock_wifi_manager::MockWifiManager;
use topside_panel_lib::app::topside_panel_app::TopsidePanelApp;
use crate::args::{Args, ConnectMode, WifiMode};
use crate::simulator_window::SimulatorDevice;

mod simulator_window;
mod args;
mod mock_wifi_manager;

const GRACEFUL_SHUTDOWN_PERIOD: Duration = Duration::from_secs(3);

fn main() -> anyhow::Result<()> {
  let args = Args::parse();

  match args.connect_to {
    ConnectMode::MockSpa => {},
    _ => todo!(),
  }

  env_logger::builder()
      .format(|buf, record| {
        let ts = buf.timestamp_micros();
        writeln!(
          buf,
          "{}: {}: {:?}: {}: {}",
          ts,
          record.metadata().target(),
          std::thread::current().id(),
          buf.default_level_style(record.level())
              .value(record.level()),
          record.args()
        )
      })
      .init();

  let ((client_in, server_out), (server_in, client_out)) = (pipe::pipe(), pipe::pipe());
  let main_board = MainBoard::new(StdTransport::new(server_in, server_out))
      .set_clear_to_send_policy(CtsEnforcementPolicy::Always, Duration::MAX)
      .set_init_delay(Duration::from_secs(5));

  let mock_wifi = MockWifiManager::new();
  let wifi_mode_control = mock_wifi.new_control_handle();
  match args.wifi_mode {
    WifiMode::Provision => wifi_mode_control.drive_first_run(),
    WifiMode::ProvisionForever => wifi_mode_control.drive_dpp_forever(),
    WifiMode::Normal => wifi_mode_control.drive_subsequent_run(),
    WifiMode::Fail => wifi_mode_control.drive_cant_connect(),
  }

  let topside_app = TopsidePanelApp::new(
      StdTransport::new(client_in, client_out),
      SimulatorDevice,
      Some(mock_wifi));

  let (hottub_handle, hottub_runner) = main_board.into_runner();
  let hottub_thread = thread::Builder::new()
      .name("HotTub Thread".to_owned())
      .spawn(move || hottub_runner.run_loop().unwrap())?;

  topside_app.run_loop()?;

  info!("Window shut down, requesting graceful shutdown...");
  thread::spawn(|| {
    thread::sleep(GRACEFUL_SHUTDOWN_PERIOD);
    panic!("Graceful shutdown expired timeout...");
  });

  hottub_handle.request_shutdown();
  hottub_thread.join().unwrap();

  Ok(())
}
