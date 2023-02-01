use num_traits::{FromPrimitive, ToPrimitive};

/// Attempt at a type-safe way of preserving the original raw value so that it could be inspected
/// in the event that the enum value is not known locally.  In hindsight, I think this is massive
/// overkill for a protocol like this one and we should abandon this in favor of just discarding
/// entire structures if any part is unparseable.  Not ideal in more robust protocols, but
/// this one is fairly rigidly defined and can be updated easily if errors are encountered in
/// the wild.
#[derive(Debug, Clone)]
pub struct ParsedEnum<TYPE, PRIMITIVE> {
  parsed: Option<TYPE>,
  raw: PRIMITIVE,
}

impl <TYPE, PRIMITIVE> ParsedEnum<TYPE, PRIMITIVE>
where
    TYPE: FromPrimitive + ToPrimitive,
    PRIMITIVE: ProtocolPrimitive<Primitive = PRIMITIVE> + Copy
{
  pub fn new(value: TYPE) -> Self {
    let raw = PRIMITIVE::from_protocol_u32(value.to_u32().unwrap()).unwrap();
    Self {
      parsed: Some(value),
      raw,
    }
  }

  pub fn from_raw(raw: PRIMITIVE) -> Self {
    Self {
      parsed: TYPE::from_u32(raw.to_protocol_u32()),
      raw,
    }
  }

  pub fn as_ref(&self) -> Option<&TYPE> {
    self.parsed.as_ref()
  }

  pub fn as_raw(&self) -> PRIMITIVE {
    self.raw
  }
}

impl<TYPE, PRIMITIVE: PartialEq> PartialEq for ParsedEnum<TYPE, PRIMITIVE> {
  fn eq(&self, other: &Self) -> bool {
    self.raw == other.raw
  }
}

// This trait ensures that it is safe for any ParsedEnum primitive type to go to/from u32 without
// loss.  Do not implement this trait for any type for which that isn't true!
pub trait ProtocolPrimitive {
  type Primitive: Copy;

  fn to_protocol_u32(&self) -> u32;
  fn from_protocol_u32(value: u32) -> Option<Self::Primitive>;
}

impl ProtocolPrimitive for u8 {
  type Primitive = u8;

  fn to_protocol_u32(&self) -> u32 { u32::from(*self) }
  fn from_protocol_u32(value: u32) -> Option<Self::Primitive> { u8::try_from(value).ok() }
}
