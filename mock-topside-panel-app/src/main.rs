use std::thread;
use std::time::Duration;
use embedded_graphics::geometry::Size;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics_simulator::{OutputSettingsBuilder, SimulatorDisplay, Window};
use log::info;
use common_lib::bus_transport::BusTransport;
use common_lib::transport::StdTransport;
use mock_mainboard_lib::channel_manager::CtsEnforcementPolicy;
use mock_mainboard_lib::main_board::MainBoard;
use topside_panel_lib::topside_panel::TopsidePanel;
use wifi_module_lib::wifi_module::WifiModule;

const GRACEFUL_SHUTDOWN_PERIOD: Duration = Duration::from_secs(3);
const BUS_BUFFER_SIZE: usize = 128;

fn main() -> anyhow::Result<()> {
  env_logger::init();

  let display = SimulatorDisplay::<Rgb565>::new(Size::new(480, 320));
  let output_settings = OutputSettingsBuilder::new().build();

  let mut window = Window::new("Mock Topside Panel", &output_settings);

  let ((client_in, server_out), (server_in, client_out)) = (pipe::pipe(), pipe::pipe());
  let main_board = MainBoard::new(StdTransport::new(server_in, server_out))
      .set_clear_to_send_policy(CtsEnforcementPolicy::Always, Duration::MAX);

  let bus_transport = BusTransport::new(
    StdTransport::new(client_in, client_out),
    BUS_BUFFER_SIZE);

  let topside = TopsidePanel::new(bus_transport.clone());
  let wifi_module = WifiModule::new(bus_transport);
  
  let (hottub_handle, hottub_runner) = main_board.into_runner();
  let mut hottub_thread = thread::Builder::new()
      .name("HotTub Thread".to_owned())
      .spawn(move || hottub_runner.run_loop().unwrap())?;

  let mut topside_thread = thread::Builder::new()
      .name("Topside Thread".to_owned())
      .spawn(move || topside.run_loop().unwrap())?;

  let mut wifi_thread = thread::Builder::new()
      .name("Wifi Thread".to_owned())
      .spawn(move || wifi_module.run_loop().unwrap())?;

  window.show_static(&display);

  info!("Window shut down, requesting graceful shutdown...");
  thread::spawn(|| {
    thread::sleep(GRACEFUL_SHUTDOWN_PERIOD);
    panic!("Graceful shutdown expired timeout...");
  });

  hottub_handle.request_shutdown();
  for thread in [hottub_thread, topside_thread, wifi_thread] {
    let _ = thread.join();
  }

  Ok(())
}