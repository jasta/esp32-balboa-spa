use balboa_spa_messages::channel::Channel;
use balboa_spa_messages::message_types::{MessageType, PayloadEncodeError};
use std::marker::PhantomData;
use std::io::Write;
use balboa_spa_messages::framed_writer::FramedWriter;
use log::debug;
use balboa_spa_messages::message::Message;
use std::fmt::{Debug, Formatter};

#[derive(Debug)]
pub struct MessageStateMachine<IS, K, C> {
  state: Box<dyn MessageState<Context=C, Kind=K> + Send + 'static>,
  state_mover: StateMover<K, C>,
  pub context: C,
  _phantom: PhantomData<IS>,
}

impl <IS, K, C> Default for MessageStateMachine<IS, K, C>
where
    IS: MessageState<Context=C, Kind=K> + Default + Send + 'static,
    C: Default
{
  fn default() -> Self {
    Self {
      state: Box::new(IS::default()),
      state_mover: Default::default(),
      context: Default::default(),
      _phantom: PhantomData,
    }
  }
}

impl <IS, K, C: Default> MessageStateMachine<IS, K, C>
where
    IS: MessageState<Context=C, Kind=K> + Default + Send + 'static,
    C: Default
{
  pub fn new() -> Self {
    Default::default()
  }
}

impl <IS, K, C> MessageStateMachine<IS, K, C> {
  pub fn state_kind(&self) -> K {
    &self.state.kind()
  }
}

impl <IS, K: PartialEq, C> MessageStateMachine<IS, K, C> {
  pub fn handle_message<W: Write>(
      &mut self,
      writer: &mut FramedWriter<W>,
      channel: &Channel,
      mt: &MessageType,
  ) -> Result<(), MessageHandlingError> {
    let state_mover = &mut self.state_mover;
    state_mover.state = None;
    let mut args = StateArgs {
      sm: state_mover,
      channel,
      mt,
      context: &mut self.context,
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
      to_state: &Box<dyn MessageState<Context=C, Kind=K> + Send + 'static>,
      writer: &mut FramedWriter<impl Write>,
      args: &mut StateArgs<K, C>,
  ) -> Result<(), MessageHandlingError> {
    match to_state.handle_message(args) {
      SmResult::HandledNoReply => Ok(()),
      SmResult::SendReply(message_result) => {
        match message_result {
          Ok(message) => {
            writer.write(&message)
                .map_err(|e| MessageHandlingError::FatalError(e.to_string()))?;
            Ok(())
          }
          Err(e) => Err(MessageHandlingError::FatalError(e.to_string())),
        }
      }
      SmResult::NotHandled => {
        // TODO: Probably want some kind of conditionalized logging here, but not 100% sure
        // what should be excluded yet.  The protocol is _very_ chatty.
        Ok(())
      },
    }
  }

  fn maybe_move_to_state(&mut self, new_state: Box<dyn MessageState<Context=C, Kind=K> + Send + 'static>) {
    if self.state.kind() != new_state.kind() {
      let old_state = &self.state;
      debug!("Moving from {old_state:?} to {new_state:?}");
      self.state = new_state;
    }
  }
}

#[derive(thiserror::Error, Debug)]
pub enum MessageHandlingError {
  #[error("Unrecoverable error that likely requires software updates: {0}")]
  FatalError(String),
}

pub struct StateArgs<'a, K, C> {
  pub sm: &'a mut StateMover<K, C>,
  pub channel: &'a Channel,
  pub mt: &'a MessageType,
  pub context: &'a mut C,
}

#[derive(Debug)]
pub struct StateMover<K, C> {
  state: Option<Box<dyn MessageState<Context=C, Kind=K> + Send + 'static>>,
}

impl <K, C> Default for StateMover<K, C> {
  fn default() -> Self {
    Self { state: None }
  }
}

impl <K, C> StateMover<K, C> {
  pub fn move_to_state(&mut self, new_state: impl MessageState<Context=C, Kind=K> + Send + 'static) {
    // Not a real move yet, just records the move to be acted upon after the message is handled.
    self.state = Some(Box::new(new_state));
  }
}

pub trait MessageState: Debug {
  type Kind;
  type Context;

  fn kind(&self) -> Self::Kind;
  fn handle_message(&self, args: &mut StateArgs<Self::Kind, Self::Context>) -> SmResult;
}

pub enum SmResult {
  SendReply(Result<Message, PayloadEncodeError>),
  HandledNoReply,
  NotHandled,
}
