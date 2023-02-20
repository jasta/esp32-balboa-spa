use crate::model::key_event::{Key, KeyEvent};

pub enum UserInputEvent {
  Quit,
  KeyEvent(KeyEvent),
}
