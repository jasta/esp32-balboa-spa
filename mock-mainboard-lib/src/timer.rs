use std::time::Duration;
use std::fmt::Debug;

pub trait SimpleTimerService: ErrorType {
  type Timer: OnceTimer<Error=Self::Error> + PeriodicTimer<Error=Self::Error> + 'static;

  fn timer(
    &mut self,
    callback: impl FnMut() + Send + 'static,
  ) -> Result<Self::Timer, Self::Error>;
}

#[must_use]
pub trait Timer: ErrorType {
  fn is_scheduled(&self) -> Result<bool, Self::Error>;

  fn cancel(&mut self) -> Result<bool, Self::Error>;
}

pub trait ErrorType {
  type Error: Debug;
}

#[must_use]
pub trait OnceTimer: Timer {
  fn after(&mut self, duration: Duration) -> Result<(), Self::Error>;
}

#[must_use]
pub trait PeriodicTimer: Timer {
  fn every(&mut self, duration: Duration) -> Result<(), Self::Error>;
}