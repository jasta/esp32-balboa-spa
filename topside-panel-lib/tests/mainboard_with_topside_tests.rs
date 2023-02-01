use std::thread;
use std::time::{Duration, Instant};
use anyhow::anyhow;
use log::LevelFilter;
use common_lib::transport::StdTransport;
use topside_panel_lib::topside_panel::{Event, TopsidePanel, ViewModelEventHandle};
use mock_mainboard_lib::channel_manager::CtsEnforcementPolicy;
use mock_mainboard_lib::main_board::MainBoard;
use topside_panel_lib::view_model::{ConnectionState, ViewModel};

#[test]
fn test_get_model_updates() -> anyhow::Result<()> {
  let _ = env_logger::builder().filter_level(LevelFilter::Debug).is_test(true).try_init();

  let expires_at = ExpiresAtTimer::expires_after(Duration::from_secs(10));

  let ((client_in, server_out), (server_in, client_out)) = (pipe::pipe(), pipe::pipe());
  let main_board = MainBoard::new(StdTransport::new(server_in, server_out))
      .set_clear_to_send_policy(CtsEnforcementPolicy::Always, Duration::MAX);

  let bus_transport = StdTransport::new(client_in, client_out);
  let topside = TopsidePanel::new(bus_transport);

  let (topside_control, topside_event, topside_runner) = topside.into_runner();
  let (main_control, main_runner) = main_board.into_runner();

  let topside_thread = thread::spawn(move || topside_runner.run_loop());
  let init_model = next_model(&topside_event, expires_at.remaining())?;
  assert_eq!(init_model.conn_state, ConnectionState::WaitingForPeer);
  assert_eq!(init_model.last_model, None);

  let main_thread = thread::spawn(move || main_runner.run_loop());

  let states = [
    ConnectionState::Negotiating,
    ConnectionState::Negotiated,
    ConnectionState::Idle,
  ];
  for state in states {
    let init_model = next_model(&topside_event, expires_at.remaining())?;
    assert_eq!(init_model.conn_state, state);
    assert_eq!(init_model.last_model, None);
  }

  main_control.complete_init();
  let heating_model = next_model(&topside_event, expires_at.remaining())?;
  assert_eq!(heating_model.conn_state, ConnectionState::Idle);
  assert_ne!(heating_model.last_model, None);
  assert!(heating_model.last_model.unwrap().is_heating);

  Ok(())
}

fn next_model(event_handle: &ViewModelEventHandle, timeout: Duration) -> anyhow::Result<ViewModel> {
  match event_handle.events_rx.recv_timeout(timeout)? {
    Event::ModelUpdated(model) => Ok(model),
  }
}

struct ExpiresAtTimer {
  started_at: Instant,
  expires_after: Duration,
}

impl ExpiresAtTimer {
  pub fn expires_after(duration: Duration) -> Self {
    Self {
      started_at: Instant::now(),
      expires_after: duration,
    }
  }

  pub fn remaining(&self) -> Duration {
    self.expires_after.saturating_sub(self.started_at.elapsed())
  }
}