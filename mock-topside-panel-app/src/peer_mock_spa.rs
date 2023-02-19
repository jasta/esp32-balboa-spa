use mock_mainboard_lib::main_board::{ControlHandle, MainBoard, Runner};
use std::io::{Read, Write};
use std::time::Duration;
use common_lib::transport::StdTransport;
use mock_mainboard_lib::channel_manager::CtsEnforcementPolicy;
use crate::peer_runner::{PeerControlHandle, PeerManager, PeerRunner};

pub fn new_peer_mock_spa<R, W>(transport: StdTransport<R, W>) -> PeerManager
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
{
  let main_board = MainBoard::new(transport)
      .set_clear_to_send_policy(CtsEnforcementPolicy::Always, Duration::MAX)
      .set_init_delay(Duration::from_secs(5));
  let (hottub_handle, hottub_runner) = main_board.into_runner();
  PeerManager {
    control_handle: Box::new(MockSpaControlHandle(hottub_handle)),
    runner: Box::new(MockSpaRunner(hottub_runner)),
  }
}

pub struct MockSpaControlHandle(ControlHandle);

impl PeerControlHandle for MockSpaControlHandle {
  fn request_shutdown(&mut self) {
    self.0.request_shutdown();
  }
}

pub struct MockSpaRunner<R, W>(Runner<R, W>);

impl<R: Read + Send + 'static, W: Write + Send + 'static> PeerRunner for MockSpaRunner<R, W> {
  fn run_loop(self: Box<Self>) -> anyhow::Result<()> {
    self.0.run_loop()
  }
}
