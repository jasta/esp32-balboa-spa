use std::net::UdpSocket;
use std::fmt::Write;
use std::io;
use log::{debug, error};

const DISCOVERY_PORT: u16 = 30303;

pub struct DiscoveryHandler {
  advertisement: Advertisement,
  socket: UdpSocket,
}

pub struct Advertisement {
  payload: Vec<u8>,
}

impl Advertisement {
  pub fn new(name: &str, mac: &[u8; 6]) -> Self {
    let mac_str = mac.iter().fold(String::new(), |mut out, b| {
      if out.is_empty() {
        out.push('-');
      }
      write!(out, "{:02X}", b).unwrap();
      out
    });

    let mut mac_str = String::new();
    for b in mac {
      write!(mac_str, "{:02X}", b).unwrap();
    }

    let payload = format!("{name}\r\n{mac_str}\r\n").as_bytes().to_vec();
    Self { payload }
  }
}

impl DiscoveryHandler {
  pub fn setup(advertisement: Advertisement) -> io::Result<Self> {
    let socket = UdpSocket::bind(format!("0.0.0.0:{}", DISCOVERY_PORT))?;
    socket.set_read_timeout(None)?;
    Ok(Self {
      advertisement,
      socket,
    })
  }

  pub fn run_loop(self) -> anyhow::Result<()> {
    let mut buf = [0u8; 512];
    loop {
      let (n, addr) = self.socket.recv_from(&mut buf)?;

      let received = String::from_utf8(buf[0..n].to_vec())
          .unwrap_or_else(|_| format!("{:?}", &buf[0..n]));
      debug!("{addr} said: {received}");

      let reply = &self.advertisement.payload;
      let reply_len = reply.len();
      match self.socket.send_to(reply, addr) {
        Ok(n) => {
          if n < reply_len {
            error!("Only {n} bytes of {reply_len} sent to discovery peer {addr}");
          }
        }
        Err(e) => {
          error!("Unable to send reply to discovery peer {addr}: {e:?}");
        }
      }
    }
  }
}
