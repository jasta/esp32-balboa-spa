use std::time::{Duration, Instant};
use balboa_spa_messages::channel::Channel;
use std::fmt::Debug;
use log::{debug, info};
use balboa_spa_messages::message::Message;
use balboa_spa_messages::message_types::{MessageType, PayloadEncodeError};
use std::io::Write;
use balboa_spa_messages::framed_writer::FramedWriter;
use SmHandleResult::{HandledNoReply, SendReply};
use crate::client_ident::ClientIdent;
use crate::cts_state_machine::SmHandleResult::{ClearToSend, NotHandled};

const DEFAULT_NEW_CLIENT_RETRY_WAIT: Duration = Duration::from_secs(2);

pub struct CTSStateMachine {
  state: Box<dyn CommunicationState + Send + 'static>,
  state_mover: StateMover,
  client_ident: ClientIdent,
}

impl Default for CTSStateMachine {
  fn default() -> Self {
    Self {
      state: Box::new(StateWaitingForNewClientCTS),
      state_mover: Default::default(),
      client_ident: Default::default(),
    }
  }
}

impl CTSStateMachine {
  pub fn new() -> Self {
    Default::default()
  }

  pub fn handle_message<W: Write>(
      &mut self,
      writer: &mut FramedWriter<W>,
      channel: &Channel,
      mt: &MessageType
  ) -> Result<SendStatus, CtsHandlingError> {
    let state_mover = &mut self.state_mover;
    state_mover.state = None;
    let mut args = CommunicationStateArgs {
      sm: state_mover,
      channel,
      mt,
      client_ident: &mut self.client_ident,
    };
    let result = Self::dispatch_handle_message(
        &self.state,
        writer,
        &mut args);
    if let Some(new_state) = std::mem::take(&mut state_mover.state) {
      self.maybe_move_to_state(new_state);
    }
    result
  }

  fn dispatch_handle_message(
      to_state: &Box<dyn CommunicationState + Send + 'static>,
      writer: &mut FramedWriter<impl Write>,
      args: &mut CommunicationStateArgs
  ) -> Result<SendStatus, CtsHandlingError> {
    match to_state.handle_message(args) {
      ClearToSend => Ok(SendStatus::Clear),
      HandledNoReply => Ok(SendStatus::NotClear),
      SendReply(message_result) => {
        match message_result {
          Ok(message) => {
            writer.write(&message)
                .map_err(|e| CtsHandlingError::FatalError(e.to_string()))?;
            Ok(SendStatus::NotClear)
          }
          Err(e) => Err(CtsHandlingError::FatalError(e.to_string())),
        }
      }
      NotHandled => {
        // TODO: Probably want some kind of conditionalized logging here, but not 100% sure
        // what should be excluded yet.  The protocol is _very_ chatty.
        Ok(SendStatus::NotClear)
      },
    }
  }

  fn maybe_move_to_state(&mut self, new_state: Box<dyn CommunicationState + Send + 'static>) {
    if self.state.kind() != new_state.kind() {
      let old_state = &self.state;
      debug!("Moving from {old_state:?} to {new_state:?}");
      self.state = new_state;
    }
  }
}

#[derive(thiserror::Error, Debug)]
pub enum CtsHandlingError {
  #[error("Unrecoverable error that likely requires software updates: {0}")]
  FatalError(String),
}

pub enum SendStatus {
  Clear,
  NotClear,
}

struct CommunicationStateArgs<'a> {
  sm: &'a mut StateMover,
  channel: &'a Channel,
  mt: &'a MessageType,
  client_ident: &'a ClientIdent,
}

#[derive(Default, Debug)]
struct StateMover {
  state: Option<Box<dyn CommunicationState + Send + 'static>>,
}

impl StateMover {
  fn move_to_state(&mut self, new_state: impl CommunicationState + Send + 'static) {
    // Not a real move yet, just records the move to be acted upon after the message is handled.
    self.state = Some(Box::new(new_state));
  }
}

trait CommunicationState: Debug {
  fn kind(&self) -> CommunicationStateKind;
  fn handle_message(&self, args: &mut CommunicationStateArgs) -> SmHandleResult;
}

#[derive(Debug)]
struct StateWaitingForNewClientCTS;

impl CommunicationState for StateWaitingForNewClientCTS {
  fn kind(&self) -> CommunicationStateKind {
    CommunicationStateKind::WaitingForNewClientCTS
  }

  fn handle_message(&self, args: &mut CommunicationStateArgs) -> SmHandleResult {
    match (args.channel, args.mt) {
      (&Channel::MulticastChannelAssignment, &MessageType::NewClientClearToSend()) => {
        args.sm.move_to_state(StateWaitingForChannelAssignment {
          ident: args.client_ident.clone(),
          requested_at: Instant::now(),
        });
        SendReply(MessageType::ChannelAssignmentRequest {
          device_type: args.client_ident.device_type,
          client_hash: args.client_ident.client_hash,
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

impl CommunicationState for StateWaitingForChannelAssignment {
  fn kind(&self) -> CommunicationStateKind {
    CommunicationStateKind::WaitingForChannelAssignment
  }

  fn handle_message(&self, args: &mut CommunicationStateArgs) -> SmHandleResult {
    match (args.channel, args.mt) {
      (&Channel::MulticastChannelAssignment, &MessageType::NewClientClearToSend()) => {
        if self.requested_at.elapsed() >= DEFAULT_NEW_CLIENT_RETRY_WAIT {
          args.sm.move_to_state(StateWaitingForNewClientCTS);
        }
        HandledNoReply
      }
      (&Channel::MulticastChannelAssignment, &MessageType::ChannelAssignmentResponse { channel, client_hash }) => {
        if self.ident.client_hash == client_hash {
          args.sm.move_to_state(StateWaitingForCTS(channel));
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
struct StateWaitingForCTS(Channel);

impl CommunicationState for StateWaitingForCTS {
  fn kind(&self) -> CommunicationStateKind {
    CommunicationStateKind::WaitingForCTS
  }

  fn handle_message(&self, args: &mut CommunicationStateArgs) -> SmHandleResult {
    match (args.channel, args.mt) {
      (c, MessageType::ClearToSend()) => {
        if c == &self.0 { ClearToSend } else { NotHandled }
      },
      _ => NotHandled,
    }
  }
}

enum SmHandleResult {
  ClearToSend,
  SendReply(Result<Message, PayloadEncodeError>),
  HandledNoReply,
  NotHandled,
}

#[derive(Debug, Clone, PartialEq)]
enum CommunicationStateKind {
  WaitingForNewClientCTS,
  WaitingForChannelAssignment,
  WaitingForCTS,
}
