use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};

pub struct ViewModelEventHandle<VM> {
  pub events_rx: Receiver<ViewEvent<VM>>,
}

impl<VM> ViewModelEventHandle<VM> {
  pub fn new() -> (Sender<ViewEvent<VM>>, Self) {
    let (tx, rx) = channel();
    (tx, ViewModelEventHandle { events_rx: rx })
  }

  pub fn try_recv_latest(&self) -> Result<Option<VM>, TryRecvError> {
    let mut latest = None;
    loop {
      match self.events_rx.try_recv() {
        Ok(ViewEvent::ModelUpdated(model)) => {
          latest = Some(model);
        },
        Err(TryRecvError::Empty) => return Ok(latest),
        Err(e) => return Err(e),
      }
    }
  }
}

pub enum ViewEvent<VM> {
  ModelUpdated(VM),
}
