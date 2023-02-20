use std::thread;
use std::time::Duration;
use std::io::Write;
use log::info;
use common_lib::transport::StdTransport;
use clap::Parser;
use mock_wifi_manager::MockWifiManager;
use topside_panel_lib::app::topside_panel_app::TopsidePanelApp;
use crate::args::{Args, WifiMode};
use crate::peer_runner::PeerManager;
use crate::simulator_window::{SimulatorDevice, SleepDelay};

mod simulator_window;
mod args;
mod mock_wifi_manager;
mod peer_runner;
mod peer_mock_spa;
mod peer_deadend;

const GRACEFUL_SHUTDOWN_PERIOD: Duration = Duration::from_secs(3);

fn main() -> anyhow::Result<()> {
  let args = Args::parse();

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
  let peer_manager = PeerManager::create(
      args.connect_to,
      StdTransport::new(server_in, server_out));

  let mock_wifi = MockWifiManager::new();
  let wifi_mode_control = mock_wifi.new_control_handle();
  match args.wifi_mode {
    WifiMode::Provision => wifi_mode_control.drive_first_run(),
    WifiMode::ProvisionForever => wifi_mode_control.drive_dpp_forever(),
    WifiMode::Normal => wifi_mode_control.drive_subsequent_run(),
    WifiMode::Fail => wifi_mode_control.drive_cant_connect(),
    WifiMode::DriverFail => wifi_mode_control.drive_init_failed(),
  }

  let topside_app = TopsidePanelApp::new(
      StdTransport::new(client_in, client_out),
      SimulatorDevice,
      Some(mock_wifi),
      SleepDelay);

  let mut peer_handle = peer_manager.control_handle;
  let peer_runner = peer_manager.runner;
  let peer_thread = thread::Builder::new()
      .name("HotTub Thread".to_owned())
      .spawn(move || peer_runner.run_loop().unwrap())?;

  topside_app.run_loop()?;

  info!("Window shut down, requesting graceful shutdown...");
  thread::spawn(|| {
    thread::sleep(GRACEFUL_SHUTDOWN_PERIOD);
    panic!("Graceful shutdown expired timeout...");
  });

  peer_handle.request_shutdown();
  peer_thread.join().unwrap();

  Ok(())
}
