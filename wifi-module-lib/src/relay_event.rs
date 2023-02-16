use balboa_spa_messages::message::Message;

#[derive(Debug, Clone)]
pub(crate) enum RelayEvent {
  MessageForIpClient(Message),
}