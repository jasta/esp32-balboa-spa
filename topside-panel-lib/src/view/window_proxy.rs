use crate::view::user_input_event::UserInputEvent;

pub trait WindowProxy<D> {
  fn events(&mut self) -> Vec<UserInputEvent>;
  fn update(&mut self, display: &D);
}
