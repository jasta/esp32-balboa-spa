use common_lib::cts_state_machine::CtsStateMachine;
use crate::wifi_state_machine::WifiStateMachine;

#[derive(Default, Debug)]
pub(crate) struct AppState {
  pub cts_state_machine: CtsStateMachine,
  pub wifi_state_machine: WifiStateMachine,
}
