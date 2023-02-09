use balboa_spa_messages::channel::Channel;

#[derive(Debug)]
pub enum ChannelFilter {
  None,
  RelevantTo(Channel),
  BlockEverything,
}

impl ChannelFilter {
  pub fn apply(&self, channel: &Channel) -> FilterResult {
    match self {
      ChannelFilter::None => FilterResult::Any,
      ChannelFilter::RelevantTo(target) => {
        if target == channel {
          return FilterResult::MyChannel;
        }
        if channel == &Channel::MulticastBroadcast {
          return FilterResult::Broadcast;
        }
        FilterResult::Blocked
      }
      ChannelFilter::BlockEverything => FilterResult::Blocked,
    }
  }
}

#[derive(Debug, PartialEq)]
pub enum FilterResult {
  MyChannel,
  Broadcast,
  Any,
  Blocked,
}