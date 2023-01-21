use std::fmt::Debug;
use std::iter::repeat;
use smart_leds::{RGB, SmartLedsWrite};
use std::marker::PhantomData;

pub trait StatusLed {
  type Error : Debug;

  fn set_color(&mut self, color: RGB<u8>) -> Result<(), Self::Error>;
}

pub struct SmartLedsStatusLed<L, LE> {
  writer: L,
  _phantom: PhantomData<LE>,
}

impl <L, LE> SmartLedsStatusLed<L, LE>
where
    L: SmartLedsWrite<Error = LE, Color = RGB<u8>>
{
  pub fn new(writer: L) -> Self {
    Self {
      writer,
      _phantom: PhantomData,
    }
  }
}

impl <L, LE> StatusLed for SmartLedsStatusLed<L, LE>
where
    L: SmartLedsWrite<Error = LE, Color = RGB<u8>>,
    LE: std::error::Error,
{
  type Error = LE;

  fn set_color(&mut self, color: RGB<u8>) -> Result<(), Self::Error> {
    self.writer.write(repeat(color).take(1))?;
    Ok(())
  }
}
