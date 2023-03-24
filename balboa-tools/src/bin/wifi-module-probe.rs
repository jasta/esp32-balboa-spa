use std::fmt::{Display, Formatter};
use std::io::{BufRead, BufReader, Cursor, Read};
use std::net::{IpAddr, SocketAddr, TcpStream, UdpSocket};
use std::time::Duration;
use anyhow::anyhow;
use balboa_spa_messages::channel::Channel;
use balboa_spa_messages::framed_reader::FramedReader;
use balboa_spa_messages::framed_writer::FramedWriter;
use balboa_spa_messages::message_types::{MessageType, MessageTypeKind, SettingsRequestMessage};

use clap::Parser;
use std::fmt::Write;

const DISCOVERY_PORT: u16 = 30303;
const TCP_PORT: u16 = 4257;

#[derive(Parser, Debug)]
pub struct Args {
  /// Scan only, do not connect to the discovered hosts
  #[arg(short, long, default_value_t = false)]
  pub scan_only: bool,
}

fn main() -> anyhow::Result<()> {
  let args = Args::parse();

  let target = find_wifi_modules()?;
  if !args.scan_only {
    for t in target {
      if let Err(e) = probe_target(&t) {
        eprintln!("Error probing {}: {e}", t.ip_address);
      }
    }
  }
  Ok(())
}

fn probe_target(host: &ModuleHost) -> anyhow::Result<()> {
  let target = host.ip_address;

  println!("Probing {target}...");

  let socket = TcpStream::connect((target, TCP_PORT))?;
  println!("Connected to {target}");
  let mut writer = FramedWriter::new(&socket);
  let mut reader = FramedReader::new(&socket);

  writer.write(&MessageType::ExistingClientRequest()
      .to_message(Channel::WifiModule)?)?;
  expect(&mut reader, MessageTypeKind::WifiModuleConfigurationResponse)?;
  println!("Got WiFi module identification");

  writer.write(&MessageType::SettingsRequest(SettingsRequestMessage::Settings0x04)
      .to_message(Channel::WifiModule)?)?;
  expect(&mut reader, MessageTypeKind::Settings0x04Response)?;
  println!("Got Settings response");

  writer.write(&MessageType::SettingsRequest(SettingsRequestMessage::Configuration)
      .to_message(Channel::WifiModule)?)?;
  expect(&mut reader, MessageTypeKind::ConfigurationResponse)?;
  println!("Got Configuration response");

  writer.write(&MessageType::SettingsRequest(SettingsRequestMessage::Information)
      .to_message(Channel::WifiModule)?)?;
  expect(&mut reader, MessageTypeKind::InformationResponse)?;
  println!("Got Information response");

  writer.write(&MessageType::SettingsRequest(SettingsRequestMessage::FaultLog { entry_num: 0 })
      .to_message(Channel::WifiModule)?)?;
  expect(&mut reader, MessageTypeKind::FaultLogResponse)?;
  println!("Got FaultLog response");

  expect(&mut reader, MessageTypeKind::StatusUpdate)?;
  println!("Got status update, exiting...");

  Ok(())
}

fn expect<R: Read>(reader: &mut FramedReader<R>, expected: MessageTypeKind) -> anyhow::Result<(Channel, MessageType)> {
  loop {
    let message = reader.next_message()?;
    println!("<= {message:?}");
    let mt = MessageType::try_from(&message)?;
    println!(" `-- [{:?}] {mt:?}", message.channel);
    let kind = MessageTypeKind::from(&mt);
    if kind == expected {
      return Ok((message.channel, mt));
    }
  }
}

fn find_wifi_modules() -> anyhow::Result<Vec<ModuleHost>> {
  let socket = UdpSocket::bind("0.0.0.0:0")?;
  socket.set_read_timeout(Some(Duration::from_secs(5)))?;
  socket.set_broadcast(true)?;

  let discovery_msg = "Discovery: Who is out there?";
  socket.send_to(discovery_msg.as_bytes(), format!("255.255.255.255:{}", DISCOVERY_PORT))?;

  let mut found = vec![];
  let mut buf = [0u8; 2048];
  loop {
    match socket.recv_from(&mut buf) {
      Ok((n, addr)) => {
        match ModuleHost::from_discovery_packet(addr.ip(), &buf[0..n]) {
          Ok(host) => {
            println!("{} {} {}", host.ip_address, host.format_mac(), host.name);
            found.push(host)
          },
          Err(e) => eprintln!("Failed to parse {}: {e}", addr.ip()),
        }
      }
      Err(e) if found.is_empty() => return Err(e.into()),
      Err(_) => return Ok(found),
    }
  }
}

#[derive(Debug, Clone)]
struct ModuleHost {
  ip_address: IpAddr,
  name: String,
  mac: [u8; 6],
}

impl ModuleHost {
  pub fn from_discovery_packet(ip_address: IpAddr, packet: &[u8]) -> anyhow::Result<Self> {
    let (name, mac) = {
      let cursor = Cursor::new(packet);
      let mut lines = cursor.lines();
      let name = lines.next().ok_or_else(|| anyhow!("Missing hostname"))??;
      let mac_str = lines.next().ok_or_else(|| anyhow!("Missing MAC"))??;

      let mac_slice: Vec<_> = mac_str
          .split('-')
          .filter_map(|x| u8::from_str_radix(x, 16).ok())
          .collect();
      let mut mac = [0u8; 6];
      mac.copy_from_slice(&mac_slice);
      (name, mac)
    };

    Ok(Self { ip_address, name, mac })
  }

  fn format_mac(&self) -> String {
    let mut formatted = self.mac.iter()
        .fold(String::new(), |mut out, x| {
          write!(out, "{:02x}:", x).unwrap();
          out
        });
    formatted.pop();
    formatted
  }
}