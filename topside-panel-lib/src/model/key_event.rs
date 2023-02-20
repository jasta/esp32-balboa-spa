#[derive(Debug, Copy, Clone)]
pub enum KeyEvent {
  KeyDown { key: Key },
  KeyUp { key: Key },
}

#[derive(Debug, Copy, Clone)]
pub enum Key {
  Up,
  Down,
  Jets1,
  Light,
}
