use std::fmt::Write;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advertisement {
  pub name: String,
  pub mac: [u8; 6],
  pub(crate) payload: Vec<u8>,
}

impl Advertisement {
  /// Create an advertisement sufficient to convince the real Wi-Fi app that we're legit.  The
  /// app is looking specifically for "BWGS" in the name and _I think_ 00-15-27 as the prefix of
  /// the MAC.
  pub fn fake_balboa() -> Self {
    Self::new("BWGS99", &[0x00, 0x15, 0x27, 0x01, 0x02, 0x03])
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
    Self {
      name: name.to_owned(),
      mac: mac.to_owned(),
      payload
    }
  }
}
