use balboa_spa_messages::message::Message;
use crate::wifi_manager::WifiEvent;

#[derive(Debug)]
pub(crate) enum Command {
  ReceivedMainboardMessage(Message),
  OnWifiEvent(WifiEvent),
  ReadError(anyhow::Error),
  RelayIpMessage(Message),
  Shutdown,
}
