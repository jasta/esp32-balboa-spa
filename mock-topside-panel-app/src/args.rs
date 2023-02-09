use std::fmt::{Display, Formatter};
use std::net::{AddrParseError, IpAddr, SocketAddr};
use std::str::FromStr;
use clap::Parser;

const DEFAULT_TCP_PORT: u16 = 4257;

#[derive(Parser, Debug)]
pub struct Args {
  /// Choose main board target (omit for in memory mock spa, use "-" to discover via broadcast)
  #[arg(short, long, value_parser = connect_mode_parser, default_value_t = ConnectMode::MockSpa)]
  pub connect_to: ConnectMode,
}

#[derive(Debug, Clone)]
pub enum ConnectMode {
  MockSpa,
  ScanAndConnect,
  ConnectTo(SocketAddr),
}

impl Display for ConnectMode {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    match self {
      ConnectMode::MockSpa => write!(f, "mock"),
      ConnectMode::ScanAndConnect => write!(f, "-"),
      ConnectMode::ConnectTo(a) => write!(f, "{a}"),
    }
  }
}

fn connect_mode_parser(s: &str) -> Result<ConnectMode, String> {
  match s {
    "mock" => Ok(ConnectMode::MockSpa),
    "-" => Ok(ConnectMode::ScanAndConnect),
    _ => {
      let addr = parse_with_default_port(s, DEFAULT_TCP_PORT)
          .map_err(|e| format!("Can't parse {s}: {e}"))?;
      Ok(ConnectMode::ConnectTo(addr))
    }
  }
}

fn parse_with_default_port(s: &str, default_port: u16) -> Result<SocketAddr, AddrParseError> {
  if !s.contains(':') {
    SocketAddr::from_str(&format!("{s}:{default_port}"))
  } else {
    SocketAddr::from_str(s)
  }
}
