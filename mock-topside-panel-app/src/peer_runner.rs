use std::io::{Read, Write};
use common_lib::transport::StdTransport;
use mock_mainboard_lib::channel_manager::CtsEnforcementPolicy;
use mock_mainboard_lib::main_board::MainBoard;
use std::time::Duration;
use peer_deadend::new_peer_deadend;
use peer_mock_spa::new_peer_mock_spa;
use crate::args::ConnectMode;
use crate::{peer_deadend, peer_mock_spa};
use crate::peer_mock_spa::{MockSpaControlHandle, MockSpaRunner};

pub struct PeerManager {
  pub control_handle: Box<dyn PeerControlHandle>,
  pub runner: Box<dyn PeerRunner + Send>,
}

impl PeerManager {
  pub fn create<R, W>(mode: ConnectMode, transport: StdTransport<R, W>) -> Self
  where
      R: Read + Send + 'static,
      W: Write + Send + 'static,
  {
    match mode {
      ConnectMode::MockSpa => new_peer_mock_spa(transport),
      ConnectMode::None => new_peer_deadend(transport),
      _ => todo!(),
    }
  }
}

pub trait PeerControlHandle {
  fn request_shutdown(&mut self);
}

pub trait PeerRunner {
  fn run_loop(self: Box<Self>) -> anyhow::Result<()>;
}