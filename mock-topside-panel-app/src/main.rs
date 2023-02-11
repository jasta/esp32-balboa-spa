use std::thread;
use std::time::Duration;
use log::info;
use common_lib::bus_transport::BusTransport;
use common_lib::transport::StdTransport;
use mock_mainboard_lib::channel_manager::CtsEnforcementPolicy;
use mock_mainboard_lib::main_board::MainBoard;
use topside_panel_lib::network::topside_panel_client::TopsidePanelClient;
use wifi_module_lib::wifi_module_client::WifiModuleClient;
use std::io::Write;
use clap::Parser;
use mock_wifi_manager::MockWifiManager;
use topside_panel_lib::app::topside_panel_app::TopsidePanelApp;
use topside_panel_lib::view::ui_handler::UiHandler;
use wifi_module_lib::advertisement::Advertisement;
use crate::args::{Args, ConnectMode};
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

  let topside_app = TopsidePanelApp::new(
      StdTransport::new(client_in, client_out),
      SimulatorDevice,
      MockWifiManager);

  let (hottub_handle, hottub_runner) = main_board.into_runner();
  let hottub_thread = thread::Builder::new()
      .name("HotTub Thread".to_owned())
      .spawn(move || hottub_runner.run_loop().unwrap())?;

  topside_app.run_loop();

  hottub_handle.request_shutdown();
  hottub_thread.join().unwrap();

  Ok(())
}
