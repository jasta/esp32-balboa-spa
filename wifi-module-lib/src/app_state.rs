use common_lib::channel_filter::ChannelFilter;
use common_lib::cts_state_machine::CtsStateMachine;
use crate::wifi_state_machine::{WifiStateMachine};

#[derive(Debug)]
pub(crate) struct AppState {
  pub cts_state_machine: CtsStateMachine,
  pub wifi_state_machine: WifiStateMachine,
}

impl Default for AppState {
  fn default() -> Self {
    let mut wifi_state_machine = WifiStateMachine::default();
    wifi_state_machine.set_channel_filter(ChannelFilter::BlockEverything);
    Self {
      cts_state_machine: CtsStateMachine::default(),
      wifi_state_machine,
    }
  }
}