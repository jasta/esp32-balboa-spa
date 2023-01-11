use num_traits::{FromPrimitive, ToPrimitive};

#[derive(Debug, Clone)]
pub struct ParsedEnum<TYPE, PRIMITIVE> {
  parsed: Option<TYPE>,
  raw: PRIMITIVE,
}

impl <TYPE: FromPrimitive + ToPrimitive, PRIMITIVE: ProtocolPrimitive> ParsedEnum<TYPE, PRIMITIVE> {
  pub fn new(value: TYPE) -> Self {
    Self {
      parsed: Some(value),
      raw: ProtocolPrimitive::from_protocol_u32(value.to_u32().unwrap()).unwrap(),
    }
  }

  pub fn from_raw(raw: PRIMITIVE) -> Self {
    Self {
      parsed: TYPE::from_u32(raw.to_protocol_u32()),
      raw,
    }
  }

  pub fn borrow_value(&self) -> &TYPE {
    self.parsed
  }

  pub fn as_raw(&self) -> PRIMITIVE {
    self.raw
  }
}

// This trait ensures that it is safe for any ParsedEnum primitive type to go to/from u32 without
// loss.  Do not implement this trait for any type for which that isn't true!
trait ProtocolPrimitive {
  fn to_protocol_u32(&self) -> u32;
  fn from_protocol_u32(value: u32) -> Option<Self>;
}

impl ProtocolPrimitive for u8 {
  fn to_protocol_u32(&self) -> u32 { u32::from(self) }
  fn from_protocol_u32(value: u32) -> Option<Self> { u8::try_from(value).ok() }
}
