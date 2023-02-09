use balboa_spa_messages::message_types::PayloadParseError;
use common_lib::message_state_machine::MessageHandlingError;

#[derive(thiserror::Error, Debug)]
pub(crate) enum HandlingError {
  #[error("Wi-Fi module fatal error, must halt: {0}")]
  FatalError(String),

  #[error("Peer sent us a malformed, unexpected, or misunderstood payload: {0}")]
  UnexpectedPayload(String),

  #[error("Graceful shutdown requested")]
  ShutdownRequested,
}

impl From<MessageHandlingError> for HandlingError {
  fn from(value: MessageHandlingError) -> Self {
    match value {
      MessageHandlingError::FatalError(m) => HandlingError::FatalError(m),
    }
  }
}

impl From<PayloadParseError> for HandlingError {
  fn from(value: PayloadParseError) -> Self {
    HandlingError::UnexpectedPayload(value.to_string())
  }
}