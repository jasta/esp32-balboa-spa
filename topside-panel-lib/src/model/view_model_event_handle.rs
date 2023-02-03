use std::sync::mpsc::{Receiver, TryRecvError};
use crate::model::view_model::ViewModel;

pub struct ViewModelEventHandle {
  pub events_rx: Receiver<Event>,
}

impl ViewModelEventHandle {
  pub fn try_recv_latest(&self) -> Result<Option<ViewModel>, TryRecvError> {
    let mut latest = None;
    loop {
      match self.events_rx.try_recv() {
        Ok(Event::ModelUpdated(model)) => {
          latest = Some(model);
        },
        Err(TryRecvError::Empty) => return Ok(latest),
        Err(e) => return Err(e),
      }
    }
  }
}

pub enum Event {
  ModelUpdated(ViewModel),
}
