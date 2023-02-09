use std::net::UdpSocket;
use std::io;
use log::{error, info};
use crate::advertisement::Advertisement;

const DISCOVERY_PORT: u16 = 30303;

pub struct DiscoveryHandler {
  advertisement: Advertisement,
  socket: UdpSocket,
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
      info!("{addr} looking for us: {received}");

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
