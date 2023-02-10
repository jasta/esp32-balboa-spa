use std::io;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, UdpSocket};
use std::time::Duration;
use log::info;
use balboa_spa_messages::channel::Channel;
use balboa_spa_messages::framed_reader::FramedReader;
use balboa_spa_messages::framed_writer::FramedWriter;
use balboa_spa_messages::message::Message;
use balboa_spa_messages::message_types::{MessageType, MessageTypeKind, SettingsRequestMessage};

const DISCOVERY_PORT: u16 = 30303;
const TCP_PORT: u16 = 4257;

fn main() -> anyhow::Result<()> {
  let target = find_wifi_module()?;
  println!("Found {target}");

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

fn find_wifi_module() -> anyhow::Result<IpAddr> {
  let socket = UdpSocket::bind("0.0.0.0:0")?;
  socket.set_read_timeout(Some(Duration::from_secs(10)))?;
  socket.set_broadcast(true)?;

  let discovery_msg = "Discovery: Who is out there?";
  socket.send_to(discovery_msg.as_bytes(), format!("255.255.255.255:{}", DISCOVERY_PORT))?;

  let mut buf = [0u8; 2048];
  let (n, addr) = socket.recv_from(&mut buf)?;
  let response = String::from_utf8(buf[0..n].to_owned())?;
  println!("Got from {}: {}", addr, response);
  Ok(addr.ip())
}