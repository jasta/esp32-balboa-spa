use std::time::{Duration, Instant};
use balboa_spa_messages::channel::Channel;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicUsize;
use log::{debug, info};
use balboa_spa_messages::message_types::MessageType;
use crate::channel_allocator_broker::{AllocatorToken, ChannelAllocatorBroker, GLOBAL_BROKER};
use crate::client_ident::ClientIdent;
use crate::message_state_machine::{MessageState, MessageStateMachine, SmResult, StateArgs};
use crate::message_state_machine::SmResult::{HandledNoReply, NotHandled, SendReply};

const DEFAULT_NEW_CLIENT_RETRY_WAIT: Duration = Duration::from_secs(2);

pub type CtsStateMachine = MessageStateMachine<StateWaitingForNewClientCTS>;

#[derive(Debug)]
pub struct CtsContext {
  client_ident: ClientIdent,
  got_channel: Option<Channel>,
  allocator_broker: Arc<ChannelAllocatorBroker>,
  allocator_token: Option<AllocatorToken>,
}

impl Default for CtsContext {
  fn default() -> Self {
    Self {
      allocator_broker: GLOBAL_BROKER.clone(),
      client_ident: Default::default(),
      got_channel: None,
      allocator_token: None,
    }
  }
}

impl CtsStateMachine {
  pub fn take_got_channel(&mut self) -> Option<Channel> {
    std::mem::take(&mut self.context.got_channel)
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
        match args.context.allocator_broker.try_allocate() {
          Some(token) => {
            args.context.allocator_token = Some(token);
            args.sm.move_to_state(StateWaitingForChannelAssignment {
              ident: args.context.client_ident.clone(),
              requested_at: Instant::now(),
            });
            SendReply(MessageType::ChannelAssignmentRequest {
              device_type: args.context.client_ident.device_type,
              client_hash: args.context.client_ident.client_hash,
            }.to_message(Channel::MulticastChannelAssignment))
          }
          None => {
            debug!("Yielding to other channel allocator...");
            NotHandled
          },
        }
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
          args.context.allocator_token = None;
          args.sm.move_to_state(StateWaitingForNewClientCTS);
        }
        HandledNoReply
      }
      (&Channel::MulticastChannelAssignment, &MessageType::ChannelAssignmentResponse { channel, client_hash }) => {
        if self.ident.client_hash == client_hash {
          args.context.got_channel = Some(channel);
          args.context.allocator_token = None;
          args.sm.move_to_state(StateChannelAssigned(channel));
          SendReply(MessageType::ChannelAssignmentAck().to_message(channel))
        } else {
          debug!("Ignoring channel assignment for {client_hash:04X} (I'm {:04X})", self.ident.client_hash);
          NotHandled
        }
      }
      _ => NotHandled,
    }
  }
}

#[derive(Debug)]
struct StateChannelAssigned(Channel);

impl MessageState for StateChannelAssigned {
  type Kind = CtsStateKind;
  type Context = CtsContext;

  fn kind(&self) -> Self::Kind {
    CtsStateKind::ChannelAssigned
  }

  fn handle_message(&self, args: &mut StateArgs<Self::Kind, Self::Context>) -> SmResult {
    NotHandled
  }
}

#[derive(Debug, PartialEq)]
pub enum CtsStateKind {
  WaitingForNewClientCTS,
  WaitingForChannelAssignment,
  ChannelAssigned,
}