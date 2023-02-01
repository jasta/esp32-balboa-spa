use std::thread;
use std::time::Duration;
use log::info;
use common_lib::bus_transport::BusTransport;
use common_lib::transport::StdTransport;
use mock_mainboard_lib::channel_manager::CtsEnforcementPolicy;
use mock_mainboard_lib::main_board::MainBoard;
use topside_panel_lib::topside_panel::TopsidePanel;
use wifi_module_lib::wifi_module::WifiModule;
use topside_panel_lib::ui_handler::UiHandler;
use std::io::Write;
use crate::simulator_window::SimulatorDevice;

mod simulator_window;

const GRACEFUL_SHUTDOWN_PERIOD: Duration = Duration::from_secs(3);
const BUS_BUFFER_SIZE: usize = 128;

fn main() -> anyhow::Result<()> {
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
      .set_clear_to_send_policy(CtsEnforcementPolicy::Always, Duration::from_millis(30))
      .set_init_delay(Duration::from_secs(5));

  let bus_transport = BusTransport::new(
    StdTransport::new(client_in, client_out),
    BUS_BUFFER_SIZE);

  let topside = TopsidePanel::new(bus_transport.clone());
  let wifi_module = WifiModule::new(bus_transport);

  let (hottub_handle, hottub_runner) = main_board.into_runner();
  let hottub_thread = thread::Builder::new()
      .name("HotTub Thread".to_owned())
      .spawn(move || hottub_runner.run_loop().unwrap())?;

  let (topside_control, topside_events, topside_runner) = topside.into_runner();
  let topside_thread = thread::Builder::new()
      .name("Topside Thread".to_owned())
      .spawn(move || topside_runner.run_loop().unwrap())?;

  let wifi_thread = thread::Builder::new()
      .name("Wifi Thread".to_owned())
      .spawn(move || wifi_module.run_loop().unwrap())?;

  let ui_thread = thread::Builder::new()
      .name("UI Thread".to_owned())
      .spawn(move || {
        let handler = UiHandler::new(SimulatorDevice, topside_control, topside_events);
        handler.run_loop().unwrap()
      })?;

  ui_thread.join().unwrap();

  info!("Window shut down, requesting graceful shutdown...");
  thread::spawn(|| {
    thread::sleep(GRACEFUL_SHUTDOWN_PERIOD);
    panic!("Graceful shutdown expired timeout...");
  });

  hottub_handle.request_shutdown();
  for thread in [hottub_thread, topside_thread] {
    let _ = thread.join();
  }

  Ok(())
}
