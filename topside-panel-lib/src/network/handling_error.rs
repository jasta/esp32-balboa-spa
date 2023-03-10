use balboa_spa_messages::message_types::PayloadEncodeError;
use common_lib::message_state_machine::MessageHandlingError;

#[derive(thiserror::Error, Debug)]
pub(crate) enum HandlingError {
  #[error("Topside fatal error, must halt: {0}")]
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

impl From<PayloadEncodeError> for HandlingError {
  fn from(value: PayloadEncodeError) -> Self {
    match value {
      PayloadEncodeError::GenericError(e) => HandlingError::FatalError(format!("{e:?})")),
      PayloadEncodeError::GenericIoError(e) => HandlingError::FatalError(format!("{e:?}")),
      PayloadEncodeError::NotSupported => HandlingError::FatalError("Not supported".to_owned()),
    }
  }
}
