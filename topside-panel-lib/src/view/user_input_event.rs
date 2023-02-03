use crate::model::button::Button;

pub enum UserInputEvent {
  Quit,
  ButtonPressed(Button),
}
