use common_lib::channel_filter::ChannelFilter;
use common_lib::cts_state_machine::CtsStateMachine;
use crate::advertisement::Advertisement;
use crate::wifi_state_machine::{WifiStateMachine};

#[derive(Debug)]
pub(crate) struct AppState {
  pub cts_state_machine: CtsStateMachine,
  pub wifi_state_machine: WifiStateMachine,
  pub advertisement: Advertisement,
}

impl AppState {
  pub fn new(advertisement: Advertisement) -> Self {
    let mut wifi_state_machine = WifiStateMachine::default();
    wifi_state_machine.set_channel_filter(ChannelFilter::BlockEverything);
    Self {
      cts_state_machine: CtsStateMachine::default(),
      wifi_state_machine,
      advertisement,
    }
  }
}
