use std::time::Duration;
use log::{info, warn};
use balboa_spa_messages::channel::Channel;
use balboa_spa_messages::message::Message;
use crate::channel_tracker::{ChannelTracker, CtsFailureAction, DeviceKey};
use crate::clear_to_send_tracker::{ClearToSendTracker, NoCtsReason, SendMessage, SendMessageFactory, TrySendMessageError};
use crate::main_board::HandlingError;

/// Encapsulates the policy-based interaction between channel tracking and cts tracking.
#[derive(Debug)]
pub(crate) struct ChannelManager {
  policy: CtsEnforcementPolicy,
  channel_tracker: ChannelTracker,
  clear_to_send_tracker: ClearToSendTracker,
}

/// Specifies the policy we should use when approving messages based on the protocol's
/// clear-to-send definition (including NewClientClearToSend).
#[derive(Debug, PartialEq, Eq)]
pub enum CtsEnforcementPolicy {
  /// Enforced strictly for all clients at all times.  Messages sent that are not cleared by
  /// the main board will be rejected.
  Always,

  /// Enforced only when there are more than one clients on the serial bus.  This makes it easier
  /// to reverse engineer the official hardware which doesn't seem to actually correctly
  /// respect ClearToSend rules.
  ForMultipleClients,

  /// Never enforced, any correctly formed packet was receive will be accepted.
  Never,
}

impl Default for ChannelManager {
  fn default() -> Self {
    Self {
      // Conservative default for integration tests
      policy: CtsEnforcementPolicy::Always,
      channel_tracker: Default::default(),
      clear_to_send_tracker: Default::default(),
    }
  }
}

impl ChannelManager {
  pub fn new() -> Self {
    Default::default()
  }

  pub fn with_policy(policy: CtsEnforcementPolicy, cts_window: Duration) -> Self {
    Self {
      policy,
      clear_to_send_tracker: ClearToSendTracker::with_window(cts_window),
      ..Default::default()
    }
  }

  pub fn num_channels(&self) -> usize {
    self.channel_tracker.len()
  }

  pub fn select_channel(&mut self, key: DeviceKey) -> Result<Channel, HandlingError> {
    self.channel_tracker.select_channel(key)
  }

  pub fn handle_presend(&mut self, sm: &SendMessage) {
    self.clear_to_send_tracker.on_send(sm)
  }

  pub fn validate_message(&mut self, message: &Message) -> Result<(), HandlingError> {
    let cts_result = self.clear_to_send_tracker
        .try_accept_incoming_message(message);
    let channel = &message.channel;
    let result = match cts_result {
      Ok(_) => {
        self.channel_tracker.record_cts_success(channel);
        Ok(())
      },
      Err(e) => {
        let reason = e.reason;
        let cts_action = self.channel_tracker.record_cts_failure(*channel);
        info!("CTS violation on channel={channel:?} for {reason:?}: {cts_action:?}");
        let err_msg = match reason {
          NoCtsReason::NoAuthorizedSenders => "No authorized senders".to_owned(),
          NoCtsReason::ConflictsWithOther => format!("Waiting for sender on {:?}", e.authorized_channel),
          NoCtsReason::ExpiredWindow => format!("Window expired on {:?}", e.attempted_channel),
        };
        Err(match cts_action {
          CtsFailureAction::ChannelNotFound |
          CtsFailureAction::ChannelRemoved => HandlingError::ClientNeedsReconnect(err_msg),
          CtsFailureAction::Tolerated => HandlingError::ClientRecoverable(err_msg),
        })
      }
    };

    match channel {
      Channel::Client(_) => {
        if !self.channel_tracker.is_allocated(channel) {
          return Err(HandlingError::ClientNeedsReconnect(
            format!("Received message on unassigned channel={channel:?}, ignoring...")));
        }
      }
      Channel::MulticastChannelAssignment => {}
      _ => {
        return Err(HandlingError::ClientUnsupported(
          format!("Received message on unexpected channel={channel:?}, ignoring...")));
      }
    }

    result.or_else(|e| {
      match self.resolve_policy() {
        ResolvedCtsPolicy::Always => Err(e),
        ResolvedCtsPolicy::Never => {
          warn!("Suppressing CTS error by policy: {e:?}");
          Ok(())
        }
      }
    })
  }

  pub fn start_send_message(&mut self) -> Result<Option<SendMessageFactory>, HandlingError> {
    match self.clear_to_send_tracker.start_send_message() {
      Ok(smf) => Ok(Some(smf)),
      Err(TrySendMessageError::WaitingToClear) => Ok(None),
      Err(TrySendMessageError::ClientError(channel)) => {
        let cts_action = self.channel_tracker.record_cts_failure(channel);
        info!("CTS window expired on channel={channel:?}: {cts_action:?}");
        match cts_action {
          CtsFailureAction::ChannelNotFound => {
            Err(HandlingError::FatalError(
              format!("{channel:?} not found but it is authorized???")))
          }
          CtsFailureAction::ChannelRemoved => {
            Ok(Some(self.clear_to_send_tracker.force_send_message()))
          }
          CtsFailureAction::Tolerated => Ok(None),
        }
      }
    }
  }

  fn resolve_policy(&self) -> ResolvedCtsPolicy {
    match self.policy {
      CtsEnforcementPolicy::ForMultipleClients => {
        if self.num_channels() == 0 { ResolvedCtsPolicy::Never } else { ResolvedCtsPolicy::Always }
      }
      CtsEnforcementPolicy::Always => ResolvedCtsPolicy::Always,
      CtsEnforcementPolicy::Never => ResolvedCtsPolicy::Never,
    }
  }
}

#[derive(Debug, Copy, Clone)]
pub enum ResolvedCtsPolicy {
  Always,
  Never,
}