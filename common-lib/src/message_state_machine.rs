use balboa_spa_messages::channel::Channel;
use balboa_spa_messages::message_types::{MessageType, PayloadEncodeError};
use std::io::Write;
use balboa_spa_messages::framed_writer::FramedWriter;
use log::debug;
use balboa_spa_messages::message::Message;
use std::fmt::{Debug};
use crate::channel_filter::{ChannelFilter, FilterResult};
use crate::message_logger::{MessageDirection, MessageLogger};

#[derive(Debug)]
pub struct MessageStateMachine<IS: MessageState> {
  state: Box<dyn MessageState<Context=IS::Context, Kind=IS::Kind> + Send + 'static>,
  state_mover: StateMover<IS::Kind, IS::Context>,
  pub context: IS::Context,
  channel_filter: ChannelFilter,
}

impl <IS> Default for MessageStateMachine<IS>
where
    IS: MessageState + Default + Send + 'static,
    IS::Context: Default,
{
  fn default() -> Self {
    Self {
      state: Box::new(IS::default()),
      state_mover: Default::default(),
      context: Default::default(),
      channel_filter: ChannelFilter::None,
    }
  }
}

impl <IS> MessageStateMachine<IS>
where
    IS: MessageState + Default + Send + 'static,
    IS::Context: Default,
{
  pub fn new() -> Self {
    Default::default()
  }

  pub fn set_channel_filter(&mut self, channel_filter: ChannelFilter) {
    self.channel_filter = channel_filter;
  }
}

impl <IS: MessageState> MessageStateMachine<IS> {
  pub fn state_kind(&self) -> IS::Kind {
    self.state.kind()
  }
}

impl <IS> MessageStateMachine<IS>
where
    IS: MessageState,
    IS::Kind: PartialEq,
{
  pub fn handle_message<W: Write>(
      &mut self,
      writer: &mut FramedWriter<W>,
      message_logger: &MessageLogger,
      channel: &Channel,
      mt: &MessageType,
  ) -> Result<(), MessageHandlingError> {
    let filter_result = self.channel_filter.apply(channel);
    if filter_result == FilterResult::Blocked {
      debug!("Filtered out message on {channel:?}");
      return Ok(());
    }

    let state_mover = &mut self.state_mover;
    state_mover.state = None;
    let mut args = StateArgs {
      sm: state_mover,
      channel,
      mt,
      context: &mut self.context,
      channel_match: filter_result,
    };
    let result = Self::dispatch_handle_message(
        &self.state,
        writer,
        message_logger,
        &mut args);
    if let Some(new_state) = std::mem::take(&mut state_mover.state) {
      self.maybe_move_to_state(new_state);
    }
    result
  }

  fn dispatch_handle_message(
      to_state: &Box<dyn MessageState<Context=IS::Context, Kind=IS::Kind> + Send + 'static>,
      writer: &mut FramedWriter<impl Write>,
      message_logger: &MessageLogger,
      args: &mut StateArgs<IS::Kind, IS::Context>,
  ) -> Result<(), MessageHandlingError> {
    match to_state.handle_message(args) {
      SmResult::HandledNoReply => Ok(()),
      SmResult::SendReply(message_result) => {
        match message_result {
          Ok(message) => {
            message_logger.log(MessageDirection::Outbound, &message);
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

  fn maybe_move_to_state(
      &mut self,
      new_state: Box<dyn MessageState<Context=IS::Context, Kind=IS::Kind> + Send + 'static>
  ) {
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
  pub channel_match: FilterResult,
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