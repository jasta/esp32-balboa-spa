use balboa_spa_messages::message::Message;
use balboa_spa_messages::message_types::MessageTypeKind;
use log::{Level, log};
use num_traits::FromPrimitive;

#[derive(Debug, Default)]
pub struct MessageLogger {
}

impl MessageLogger {
  pub fn log(&self, direction: MessageDirection, message: &Message) {
    let (suffix, level) = match MessageTypeKind::from_u8(message.message_type) {
      None => ("(unknown!)", Level::Warn),
      Some(kind) => {
        match kind {
          MessageTypeKind::NewClientClearToSend |
          MessageTypeKind::ClearToSend => {
            ("", Level::Debug)
          }
          _ => ("", Level::Info)
        }
      }
    };

    let direction_label = match direction {
      MessageDirection::Inbound => "<=",
      MessageDirection::Outbound => "=>",
    };
    log!(level, "{direction_label} Message{suffix}: {message:?}");
  }
}

#[derive(Debug, Clone, Copy)]
pub enum MessageDirection {
  Inbound,
  Outbound,
}
