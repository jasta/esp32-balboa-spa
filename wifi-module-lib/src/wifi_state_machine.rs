use std::collections::VecDeque;
use balboa_spa_messages::message::Message;
use balboa_spa_messages::message_types::MessageType;
use common_lib::message_state_machine::{MessageState, MessageStateMachine, SmResult, StateArgs};
use common_lib::message_state_machine::SmResult::{HandledNoReply, NotHandled, SendReply};

pub type WifiStateMachine = MessageStateMachine<StateRelaying>;

#[derive(Default, Debug)]
pub struct WifiContext {
  pub for_relay_messages: VecDeque<Message>,
  pub outbound_messages: VecDeque<MessageType>,
}

#[derive(Default, Debug)]
pub struct StateRelaying;

impl MessageState for StateRelaying {
  type Kind = WifiStateKind;
  type Context = WifiContext;

  fn kind(&self) -> Self::Kind {
    WifiStateKind::Relaying
  }

  fn handle_message(&self, args: &mut StateArgs<Self::Kind, Self::Context>) -> SmResult {
    match args.mt {
      MessageType::ClearToSend() => {
        let reply = args.context.outbound_messages.pop_front()
            .unwrap_or(MessageType::NothingToSend());
        SendReply(reply.to_message(*args.channel))
      }
      mt => {
        let message = mt.clone().to_message(*args.channel)
            .expect("Failed to re-encode message");
        args.context.for_relay_messages.push_back(message);

        // No reply yet.  We'll forward this to our peer over Wi-Fi and if they have something
        // to say we'll put it into outbound_messages queue and send on the next CTS window.
        HandledNoReply
      }
    }
  }
}

#[derive(Debug, PartialEq)]
pub enum WifiStateKind {
  Relaying,
}
