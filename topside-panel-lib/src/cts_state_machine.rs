use std::time::{Duration, Instant};
use balboa_spa_messages::channel::Channel;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Sender;
use balboa_spa_messages::message_types::{MessageType, MessageTypeKind};
use crate::message_state_machine::SmResult::{NotHandled, HandledNoReply, SendReply};
use crate::client_ident::ClientIdent;
use crate::message_state_machine::{MessageState, MessageStateMachine, SmResult, StateArgs};
use crate::topside_panel::Event;
use crate::view_model::{ConnectionState, ViewModel};

const DEFAULT_NEW_CLIENT_RETRY_WAIT: Duration = Duration::from_secs(2);

pub type CtsStateMachine = MessageStateMachine<StateWaitingForNewClientCTS>;

#[derive(Default, Debug)]
pub struct CtsContext {
  client_ident: ClientIdent,
  current_message_for_us: bool,
}

impl CtsStateMachine {
  pub fn take_current_message_for_us(&mut self) -> bool {
    std::mem::take(&mut self.context.current_message_for_us)
  }
}

#[derive(Default, Debug)]
pub struct StateWaitingForNewClientCTS;

impl MessageState for StateWaitingForNewClientCTS {
  type Kind = CtsStateKind;
  type Context = CtsContext;

  fn kind(&self) -> Self::Kind {
    CtsStateKind::WaitingForNewClientCTS
  }

  fn handle_message(&self, args: &mut StateArgs<Self::Kind, Self::Context>) -> SmResult {
    match (args.channel, args.mt) {
      (&Channel::MulticastChannelAssignment, &MessageType::NewClientClearToSend()) => {
        args.sm.move_to_state(StateWaitingForChannelAssignment {
          ident: args.context.client_ident.clone(),
          requested_at: Instant::now(),
        });
        SendReply(MessageType::ChannelAssignmentRequest {
          device_type: args.context.client_ident.device_type,
          client_hash: args.context.client_ident.client_hash,
        }.to_message(Channel::MulticastChannelAssignment))
      }
      _ => NotHandled,
    }
  }
}

#[derive(Debug)]
struct StateWaitingForChannelAssignment {
  ident: ClientIdent,
  requested_at: Instant,
}

impl MessageState for StateWaitingForChannelAssignment {
  type Kind = CtsStateKind;
  type Context = CtsContext;

  fn kind(&self) -> Self::Kind {
    CtsStateKind::WaitingForChannelAssignment
  }

  fn handle_message(&self, args: &mut StateArgs<Self::Kind, Self::Context>) -> SmResult {
    match (args.channel, args.mt) {
      (&Channel::MulticastChannelAssignment, &MessageType::NewClientClearToSend()) => {
        if self.requested_at.elapsed() >= DEFAULT_NEW_CLIENT_RETRY_WAIT {
          args.sm.move_to_state(StateWaitingForNewClientCTS);
        }
        HandledNoReply
      }
      (&Channel::MulticastChannelAssignment, &MessageType::ChannelAssignmentResponse { channel, client_hash }) => {
        if self.ident.client_hash == client_hash {
          args.sm.move_to_state(StateWaitingForChannelAssigned(channel));
          SendReply(MessageType::ChannelAssignmentAck().to_message(channel))
        } else {
          NotHandled
        }
      }
      _ => NotHandled,
    }
  }
}

#[derive(Debug)]
struct StateWaitingForChannelAssigned(Channel);

impl MessageState for StateWaitingForChannelAssigned {
  type Kind = CtsStateKind;
  type Context = CtsContext;

  fn kind(&self) -> Self::Kind {
    CtsStateKind::ChannelAssigned
  }

  fn handle_message(&self, args: &mut StateArgs<Self::Kind, Self::Context>) -> SmResult {
    if args.channel == &self.0 {
      args.context.current_message_for_us = true;
      HandledNoReply
    } else {
      NotHandled
    }
  }
}

#[derive(Debug, PartialEq)]
pub enum CtsStateKind {
  WaitingForNewClientCTS,
  WaitingForChannelAssignment,
  ChannelAssigned,
}