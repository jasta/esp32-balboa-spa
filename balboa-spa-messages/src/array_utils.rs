use std::io::{Cursor, Read};
use anyhow::anyhow;
use crate::message_types::{PayloadEncodeError, PayloadParseError};

pub fn encode_array<const N: usize>(field_name: &str, src: &[u8]) -> Result<[u8; N], PayloadEncodeError> {
  let mut arr = [0u8; N];
  let _ = encode_array_internal(field_name, &mut arr, src)?;
  Ok(arr)
}

fn encode_array_internal<'a>(field_name: &str, dst: &'a mut [u8], src: &[u8]) -> Result<&'a [u8], PayloadEncodeError> {
  let max = dst.len();
  if src.len() > max {
    return Err(PayloadEncodeError::GenericError(anyhow!("{field_name}={src:?} exceeds max size {max}")));
  }
  for (index, b) in src.iter().take(dst.len()).enumerate() {
    dst[index] = *b;
  }
  Ok(dst)
}
