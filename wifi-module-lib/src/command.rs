use balboa_spa_messages::message::Message;

#[derive(Debug)]
pub(crate) enum Command {
  ReceivedMainboardMessage(Message),
  ReadError(anyhow::Error),
  RelayIpMessage(Message),
  Shutdown,
}
