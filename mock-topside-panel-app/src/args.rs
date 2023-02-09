use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use clap::Parser;

const DEFAULT_TCP_PORT: u16 = 4257;

#[derive(Parser, Debug)]
pub struct Args {
  /// Choose main board target (omit for in memory mock spa, use "-" to discover via broadcast)
  #[arg(short, long, value_parser = connect_mode_parser)]
  pub connect_to: ConnectMode,
}

#[derive(Debug, Clone)]
enum ConnectMode {
  MockSpa,
  ScanAndConnect,
  ConnectTo(SocketAddr),
}

impl Default for ConnectMode {
  fn default() -> Self {
    Self::MockSpa
  }
}

fn connect_mode_parser(s: &str) -> Result<ConnectMode, String> {
  if s == "-" {
    Ok(ConnectMode::ScanAndConnect)
  } else {
    let addr = s.parse()
        .map_err(|e| format!("Can't parse {s}: {e}"))?;
    Ok(ConnectMode::ConnectTo(addr))
  }
}
