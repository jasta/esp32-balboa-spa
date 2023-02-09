use std::fmt::Write;

pub struct Advertisement {
  pub(crate) payload: Vec<u8>,
}

impl Advertisement {
  pub fn fake_balboa() -> Self {
    Self::new("MckSpa", &[0x00, 0x15, 0x27, 0x01, 0x02, 0x03])
  }

  pub fn new(name: &str, mac: &[u8; 6]) -> Self {
    let mac_str = mac.iter().fold(String::new(), |mut out, b| {
      if !out.is_empty() {
        out.push('-');
      }
      write!(out, "{:02X}", b).unwrap();
      out
    });

    let payload = format!("{name}\r\n{mac_str}\r\n").as_bytes().to_vec();
    Self { payload }
  }
}
