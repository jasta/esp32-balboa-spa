#[derive(thiserror::Error, Debug)]
pub(crate) enum HandlingError {
  #[error("Topside fatal error, must halt: {0}")]
  FatalError(String),

  #[error("Peer sent us a malformed, unexpected, or misunderstood payload: {0}")]
  UnexpectedPayload(String),
}
