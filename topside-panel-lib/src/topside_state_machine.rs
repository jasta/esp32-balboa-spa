use std::time::Instant;
use balboa_spa_messages::channel::Channel;
use balboa_spa_messages::message_types::{ConfigurationResponseMessage, InformationResponseMessage, MessageType, PreferencesResponseMessage, SettingsRequestMessage, StatusUpdateMessage};
use crate::message_state_machine::{MessageState, MessageStateMachine, SmResult, StateArgs};
use crate::message_state_machine::SmResult::{HandledNoReply, NotHandled, SendReply};

pub type TopsideStateMachine = MessageStateMachine<StateWaitingForCts>;

#[derive(Default, Debug)]
pub struct TopsideContext {
  pub info: Option<InformationResponseMessage>,
  pub config: Option<ConfigurationResponseMessage>,
  pub prefs: Option<PreferencesResponseMessage>,
  pub status: Option<ReceivedStatusMessage>,
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
    self.info.is_some() &&
        self.config.is_some() &&
        self.prefs.is_some()
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
        } else if args.context.config.is_none() {
          Some(SettingsRequestMessage::Information)
        } else if args.context.prefs.is_none() {
          Some(SettingsRequestMessage::Preferences)
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
        args.context.info = Some(m.clone());
        HandledNoReply
      }
      MessageType::ConfigurationResponse(m) => {
        args.context.config = Some(m.clone());
        HandledNoReply
      }
      MessageType::PreferencesResponse(m) => {
        args.context.prefs = Some(m.clone());
        HandledNoReply
      }
      _ => NotHandled,
    };

    if args.context.got_it_all() {
      args.sm.move_to_state(StateReadingStatus);
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
        SendReply(MessageType::NothingToSend().to_message(*args.channel))
      }
      MessageType::StatusUpdate(m) => {
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
