use std::sync::mpsc::{channel, Receiver, Sender};
use std::{mem, thread};
use std::time::Duration;
use anyhow::anyhow;
use enum_kinds::EnumKind;
use MockWifiCommand::{AnswerInit, AnswerStaConnect};
use wifi_module_lib::advertisement::Advertisement;
use wifi_module_lib::wifi_manager::{StaAssociationError, WifiManager};
use crate::mock_wifi_manager::MockWifiCommand::{AnswerDppListenThenWait, AnswerStaNetworkName, AnswerWaitWhileConnected, AnswerDppGenerateQr, Sleep};

const DEFAULT_CONNECT_DELAY: Duration = Duration::from_secs(2);

pub struct MockWifiManager {
  command_tx: Sender<MockWifiCommand>,
  command_rx: Receiver<MockWifiCommand>,
  advertisement: Advertisement,
}

impl MockWifiManager {
  pub fn new() -> Self {
    let (command_tx, command_rx) = channel();
    Self {
      command_tx,
      command_rx,
      advertisement: Advertisement::fake_balboa(),
    }
  }

  pub fn new_control_handle(&self) -> ControlHandle {
    ControlHandle {
      command_tx: self.command_tx.clone()
    }
  }

  fn next_command(&self) -> MockWifiCommand {
    loop {
      match self.command_rx.recv().unwrap() {
        Sleep(d) => thread::sleep(d),
        other => return other,
      }
    }
  }

  fn expect_command(&self, expected: MockWifiCommandKind) -> Result<MockWifiCommand, String> {
    let cmd = self.next_command();
    let actual = MockWifiCommandKind::from(&cmd);
    if actual != expected {
      Err(format!("Got {actual:?}, expected {expected:?}"))
    } else {
      Ok(cmd)
    }
  }
}

impl WifiManager for MockWifiManager {
  type Error = String;

  fn advertisement(&self) -> &Advertisement {
    &self.advertisement
  }

  fn init(&mut self) -> Result<(), Self::Error> {
    match self.expect_command(MockWifiCommandKind::AnswerInit)? {
      AnswerInit(r) => r,
      _ => panic!(),
    }
  }

  fn get_sta_network_name(&self) -> Result<Option<String>, Self::Error> {
    match self.expect_command(MockWifiCommandKind::AnswerStaNetworkName)? {
      AnswerStaNetworkName(r) => r,
      _ => panic!(),
    }
  }

  fn dpp_generate_qr(&mut self) -> Result<String, Self::Error> {
    match self.expect_command(MockWifiCommandKind::AnswerDppGenerateQr)? {
      AnswerDppGenerateQr(r) => r,
      _ => panic!(),
    }
  }

  fn dpp_listen_then_wait(&mut self) -> Result<String, Self::Error> {
    match self.expect_command(MockWifiCommandKind::AnswerDppListenThenWait)? {
      AnswerDppListenThenWait(r) => r,
      _ => panic!(),
    }
  }

  fn sta_connect(&mut self) -> Result<(), StaAssociationError> {
    match self.expect_command(MockWifiCommandKind::AnswerStaConnect) {
      Ok(AnswerStaConnect(r)) => r,
      Err(e) => Err(StaAssociationError::SystemError(e)),
      _ => panic!(),
    }
  }

  fn wait_while_connected(&mut self) -> Result<(), Self::Error> {
    match self.expect_command(MockWifiCommandKind::AnswerWaitWhileConnected)? {
      AnswerWaitWhileConnected(r) => r,
      _ => panic!(),
    }
  }
}

pub struct ControlHandle {
  command_tx: Sender<MockWifiCommand>,
}

impl ControlHandle {
  pub fn drive_custom(self) -> Sender<MockWifiCommand> {
    self.command_tx
  }

  pub fn drive_dpp_forever(self) {
    self.send_cmds([
      AnswerInit(Ok(())),
      AnswerStaNetworkName(Ok(None)),
      Sleep(Duration::from_secs(1)),
      AnswerDppGenerateQr(Ok("Hello, world".to_owned())),
      // Get stuck...
    ].as_slice());
  }

  pub fn drive_first_run(self) {
    self.send_cmds([
      AnswerInit(Ok(())),
      AnswerStaNetworkName(Ok(None)),
      Sleep(Duration::from_secs(1)),
      AnswerDppGenerateQr(Ok("Hello, world".to_owned())),
      Sleep(Duration::from_secs(5)),
      AnswerDppListenThenWait(Ok("mynetwork".to_owned())),
      Sleep(DEFAULT_CONNECT_DELAY),
      AnswerStaConnect(Ok(())),
      // Never AnswerWaitWhileConnected, just stay connected...
    ].as_slice());
  }

  pub fn drive_subsequent_run(self) {
    self.send_cmds([
      AnswerInit(Ok(())),
      AnswerStaNetworkName(Ok(Some("mynetwork".to_owned()))),
      Sleep(DEFAULT_CONNECT_DELAY),
      AnswerStaConnect(Ok(())),
      // Never AnswerWaitWhileConnected, just stay connected...
    ].as_slice());
  }

  pub fn drive_cant_connect(self) {
    self.send_cmds([
      AnswerInit(Ok(())),
      AnswerStaNetworkName(Ok(Some("mynetwork".to_owned()))),
      Sleep(Duration::from_secs(60)),
      AnswerStaConnect(Err(StaAssociationError::AssociationTimedOut)),
      Sleep(Duration::from_secs(60)),
      AnswerStaConnect(Err(StaAssociationError::AssociationTimedOut)),
      Sleep(Duration::from_secs(60)),
      AnswerStaConnect(Err(StaAssociationError::AssociationTimedOut)),
      Sleep(Duration::from_secs(60)),
      AnswerStaConnect(Err(StaAssociationError::AssociationTimedOut)),
      // Hang forever trying to connect... not correct, but should do the trick :)
    ].as_slice());
  }

  fn send_cmds(&self, cmds: &[MockWifiCommand]) {
    for cmd in cmds {
      let _ = self.command_tx.send(cmd.clone());
    }
  }
}

#[derive(EnumKind, Debug, Clone)]
#[enum_kind(MockWifiCommandKind)]
pub enum MockWifiCommand {
  Sleep(Duration),
  AnswerInit(Result<(), String>),
  AnswerStaNetworkName(Result<Option<String>, String>),
  AnswerDppGenerateQr(Result<String, String>),
  AnswerDppListenThenWait(Result<String, String>),
  AnswerStaConnect(Result<(), StaAssociationError>),
  AnswerWaitWhileConnected(Result<(), String>),
}
