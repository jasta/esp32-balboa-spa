use std::fmt::{Debug, Formatter};
use std::mem;
use std::ops::AddAssign;
use std::time::{Duration, Instant};

use log::{error, warn};

use balboa_spa_messages::channel::Channel;
use balboa_spa_messages::message::Message;

/// Amount of time to wait when we issue NewClientClearToSend or ClearToSend for a reply
/// before we can resume sending messages.
const DEFAULT_CLEAR_TO_SEND_WINDOW: Duration = Duration::from_millis(20);

#[derive(Debug)]
pub(crate) struct ClearToSendTracker {
  authorized_sender: Option<AuthorizedSender>,
  allowed_delay: Duration,
}

impl Default for ClearToSendTracker {
  fn default() -> Self {
    Self {
      allowed_delay: DEFAULT_CLEAR_TO_SEND_WINDOW,
      authorized_sender: None,
    }
  }
}

impl ClearToSendTracker {
  pub fn new() -> Self {
    Default::default()
  }

  pub fn with_policy(cts_window: Duration) -> Self {
    Self {
      allowed_delay: cts_window,
      ..Default::default()
    }
  }

  pub fn try_accept_incoming_message(&mut self, message: &Message) -> Result<(), IncomingMessageError> {
    // Note that this means a denial of service is trivially possible if an unauthorized
    // sender spams the signal line.  That's already going to break RS485 communication though,
    // so nothing we can do about it.
    let authorized_sender = mem::take(&mut self.authorized_sender);

    let channel = &message.channel;
    match authorized_sender {
      Some(authorized_sender) => {
        if &authorized_sender.channel != channel {
          return Err(IncomingMessageError::new(
              *channel,
              Some(authorized_sender.channel),
              NoCtsReason::ConflictsWithOther));
        }
        if authorized_sender.is_expired() {
          Err(IncomingMessageError::new(
              *channel,
              Some(authorized_sender.channel),
              NoCtsReason::ExpiredWindow))
        } else {
          Ok(())
        }
      }
      None => Err(IncomingMessageError::new(
          *channel,
          None,
          NoCtsReason::NoAuthorizedSenders)),
    }
  }

  pub fn start_send_message(&self) -> Result<SendMessageFactory, TrySendMessageError> {
    match &self.authorized_sender {
      Some(authorized) => {
        if authorized.authorized_at.elapsed() > self.allowed_delay {
          if let Channel::Client(_) = authorized.channel {
            Err(TrySendMessageError::ClientError(authorized.channel))
          } else {
            Ok(SendMessageFactory {})
          }
        } else {
          Err(TrySendMessageError::WaitingToClear)
        }
      }
      None => Ok(SendMessageFactory {}),
    }
  }

  pub fn on_send(&mut self, sm: &SendMessage) {
    if let Some(authorized) = &self.authorized_sender {
      // Hmm, is this even possible given the typesafe logic we have around SendMessageFactory?
      warn!("Existing authorized sender on channel={:?} dropped implicitly!", authorized.channel);
    }

    let authorized_sender = sm.expect_reply_on.map(|channel| {
      AuthorizedSender::from_now(channel, self.allowed_delay)
    });
    self.authorized_sender = authorized_sender;
  }
}

#[derive(Debug)]
struct AuthorizedSender {
  authorized_at: Instant,
  allowed_delay: Duration,
  channel: Channel,
}

#[derive(Debug)]
pub(crate) struct SendMessageFactory {
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum TrySendMessageError {
  #[error("Existing authorization has not exceeded CTS window")]
  WaitingToClear,

  #[error("Client on channel {0:?} expired its window")]
  ClientError(Channel)
}

#[derive(Debug)]
pub(crate) struct IncomingMessageError {
  pub attempted_channel: Channel,
  pub authorized_channel: Option<Channel>,
  pub reason: NoCtsReason,
}

#[derive(Debug)]
pub(crate) enum NoCtsReason {
  NoAuthorizedSenders,
  ConflictsWithOther,
  ExpiredWindow,
}

impl AuthorizedSender {
  pub fn from_now(channel: Channel, allowed_delay: Duration) -> Self {
    Self {
      channel,
      authorized_at: Instant::now(),
      allowed_delay,
    }
  }

  pub fn is_expired(&self) -> bool {
    self.authorized_at.elapsed() > self.allowed_delay
  }
}

impl IncomingMessageError {
  pub fn new(
      attempted_channel: Channel,
      authorized_channel: Option<Channel>,
      reason: NoCtsReason
  ) -> Self {
    Self { attempted_channel, authorized_channel, reason }
  }
}

impl SendMessageFactory {
  pub fn expect_reply(self, message: Message) -> SendMessage {
    let channel = message.channel;
    SendMessage { message, expect_reply_on: Some(channel) }
  }

  pub fn expect_reply_on_channel(self, message: Message, channel: Channel) -> SendMessage {
    SendMessage { message, expect_reply_on: Some(channel) }
  }

  pub fn no_reply(self, message: Message) -> SendMessage {
    SendMessage { message, expect_reply_on: None }
  }
}

#[derive(Debug)]
pub struct SendMessage {
  pub message: Message,
  expect_reply_on: Option<Channel>,
}
