use std::collections::VecDeque;
use std::time::Instant;
use log::{debug, info};
use balboa_spa_messages::message_types::{ConfigurationResponseMessage, InformationResponseMessage, MessageType, PreferencesResponseMessage, Settings0x04ResponseMessage, SettingsRequestMessage, StatusUpdateMessage};
use common_lib::message_state_machine::{MessageState, MessageStateMachine, SmResult, StateArgs};
use common_lib::message_state_machine::SmResult::{HandledNoReply, NotHandled, SendReply};

pub type TopsideStateMachine = MessageStateMachine<StateWaitingForCts>;

#[derive(Default, Debug)]
pub struct TopsideContext {
  pub info: Option<InformationResponseMessage>,
  pub settings0x04: Option<Settings0x04ResponseMessage>,
  pub config: Option<ConfigurationResponseMessage>,
  pub status: Option<ReceivedStatusMessage>,
  pub outbound_messages: VecDeque<MessageType>,
}

#[derive(Debug)]
pub struct ReceivedStatusMessage {
  pub message: StatusUpdateMessage,
  pub received_at: Instant,
}

impl ReceivedStatusMessage {
  pub fn received(message: StatusUpdateMessage) -> Self {
    Self {
      message,
      received_at: Instant::now(),
    }
  }
}

impl TopsideContext {
  pub fn got_it_all(&self) -> bool {
    self.info.is_some() && self.settings0x04.is_some() && self.config.is_some()
  }
}

#[derive(Default, Debug)]
pub struct StateWaitingForCts;

impl MessageState for StateWaitingForCts {
  type Kind = TopsideStateKind;
  type Context = TopsideContext;

  fn kind(&self) -> Self::Kind {
    TopsideStateKind::WaitingForCts
  }

  fn handle_message(&self, args: &mut StateArgs<Self::Kind, Self::Context>) -> SmResult {
    match args.mt {
      MessageType::ClearToSend() => {
        let request = if args.context.info.is_none() {
          Some(SettingsRequestMessage::Information)
        } else if args.context.settings0x04.is_none() {
          Some(SettingsRequestMessage::Settings0x04)
        } else if args.context.config.is_none() {
          Some(SettingsRequestMessage::Configuration)
        } else {
          None
        };
        match request {
          Some(request) => {
            args.sm.move_to_state(StateWaitingForResponse);
            SendReply(MessageType::SettingsRequest(request).to_message(*args.channel))
          }
          None => NotHandled,
        }
      }
      _ => NotHandled,
    }
  }
}

#[derive(Default, Debug)]
pub struct StateWaitingForResponse;

impl MessageState for StateWaitingForResponse {
  type Kind = TopsideStateKind;
  type Context = TopsideContext;

  fn kind(&self) -> Self::Kind {
    TopsideStateKind::WaitingForResponse
  }

  fn handle_message(&self, args: &mut StateArgs<Self::Kind, Self::Context>) -> SmResult {
    let reply = match args.mt {
      MessageType::InformationResponse(m) => {
        debug!("Got information: {m:?}");
        args.context.info = Some(m.clone());
        HandledNoReply
      }
      MessageType::Settings0x04Response(m) => {
        debug!("Got settings 0x04: {m:?}");
        args.context.settings0x04 = Some(m.clone());
        HandledNoReply
      }
      MessageType::ConfigurationResponse(m) => {
        debug!("Got configuration: {m:?}");
        args.context.config = Some(m.clone());
        HandledNoReply
      }
      _ => NotHandled,
    };

    if args.context.got_it_all() {
      info!("Got everything, moving to continuously reading status...");
      args.sm.move_to_state(StateReadingStatus);
    } else {
      args.sm.move_to_state(StateWaitingForCts);
    }

    reply
  }
}

#[derive(Default, Debug)]
pub struct StateReadingStatus;

impl MessageState for StateReadingStatus {
  type Kind = TopsideStateKind;
  type Context = TopsideContext;

  fn kind(&self) -> Self::Kind {
    TopsideStateKind::ReadingStatus
  }

  fn handle_message(&self, args: &mut StateArgs<Self::Kind, Self::Context>) -> SmResult {
    match args.mt {
      MessageType::ClearToSend() => {
        let reply = args.context.outbound_messages.pop_front()
            .unwrap_or_else(|| MessageType::NothingToSend());
        SendReply(reply.to_message(*args.channel))
      }
      MessageType::StatusUpdate(m) => {
        info!("Got status update: {m:?}");
        args.context.status = Some(ReceivedStatusMessage::received(m.clone()));
        HandledNoReply
      }
      _ => NotHandled,
    }
  }
}

#[derive(Debug, PartialEq)]
pub enum TopsideStateKind {
  WaitingForCts,
  WaitingForResponse,
  ReadingStatus,
}
