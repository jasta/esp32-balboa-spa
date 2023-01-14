use std::io::{Read, Write};
use std::thread;
use anyhow::anyhow;
use pipe::{PipeReader, PipeWriter};
use balboa_spa_messages::channel::Channel;
use balboa_spa_messages::framing::{FramedReader, FramedWriter};
use balboa_spa_messages::message::Message;
use balboa_spa_messages::message_types::{MessageType, MessageTypeKind, PayloadParseError, SettingsRequestMessage};
use balboa_spa_protocol::main_board::MainBoard;
use balboa_spa_protocol::transport::Transport;

#[test]
fn mainboard_get_version() -> anyhow::Result<()> {
  let _ = env_logger::builder().is_test(true).try_init();

  let ((mut client_in, server_out), (server_in, client_out)) = (pipe::pipe(), pipe::pipe());
  let main_board = MainBoard::new(PipeTransport::new(server_in, server_out));
  let (shutdown_handle, runner) = main_board.into_runner();

  let run_thread = thread::Builder::new()
      .name("ServerMainThread".into())
      .spawn(move || runner.run_loop())
      .unwrap();

  let mut reader_helper = ReaderHelper::new(client_in);
  let mut writer_helper = WriterHelper::new(client_out);

  let _ = reader_helper.expect(
      Channel::MulticastChannelAssignment, MessageTypeKind::NewClientClearToSend)?;
  writer_helper.write(
      MessageType::ChannelAssignmentRequest {
        device_type: 0x0,
        client_hash: 0xcafe,
      }.to_message(Channel::MulticastChannelAssignment)?)?;
  let channel_assignment = reader_helper.expect(
      Channel::MulticastChannelAssignment, MessageTypeKind::ChannelAssignmentResponse)?;
  let channel = match channel_assignment {
    MessageType::ChannelAssignmentResponse { channel, client_hash } => channel,
    _ => panic!(),
  };
  writer_helper.write(MessageType::ChannelAssignmentAck().to_message(channel)?)?;
  let _ = reader_helper.expect(channel, MessageTypeKind::ClearToSend)?;
  writer_helper.write(
      MessageType::SettingsRequest(SettingsRequestMessage::Information)
      .to_message(channel)?)?;
  let info = reader_helper.expect(channel, MessageTypeKind::InformationResponse)?;
  let model_number = match info {
    MessageType::InformationResponse(message) => {
      message.system_model_number
    }
    _ => panic!(),
  };

  assert_eq!(model_number, "MockSpa 3000");

  shutdown_handle.request_shutdown();
  run_thread.join().unwrap()?;

  Ok(())
}

#[derive(Debug)]
struct ReaderHelper<R> {
  raw_reader: R,
  framed_reader: FramedReader,
  buf: [u8; 32],
}

impl<R: Read> ReaderHelper<R> {
  pub fn new(raw_reader: R) -> Self {
    Self {
      raw_reader,
      framed_reader: FramedReader::new(),
      buf: [0u8; 32],
    }
  }

  pub fn expect(&mut self, channel: Channel, kind: MessageTypeKind) -> anyhow::Result<MessageType> {
    loop {
      let message = self.next_message()?;
      if message.channel != channel {
        continue;
      }
      let parsed = MessageType::try_from(&message)?;
      if MessageTypeKind::from(&parsed) != kind {
        continue;
      }

      return Ok(parsed);
    }
  }

  pub fn next_message(&mut self) -> anyhow::Result<Message> {
    loop {
      match self.raw_reader.read(self.buf.as_mut_slice())? {
        n if n == 0 => return Err(anyhow!("Unexpected EOF")),
        n => {
          for b in &self.buf[0..n] {
            if let Some(message) = self.framed_reader.accept(*b) {
              return Ok(message);
            }
          }
        }
      }
    }
  }
}

#[derive(Debug)]
struct WriterHelper<W> {
  raw_writer: W,
  framed_writer: FramedWriter,
}

impl<W: Write> WriterHelper<W> {
  pub fn new(raw_writer: W) -> Self {
    Self {
      raw_writer,
      framed_writer: FramedWriter::new(),
    }
  }

  pub fn write(&mut self, message: Message) -> anyhow::Result<()> {
    let encoded = self.framed_writer.encode(&message)?;
    self.raw_writer.write_all(&encoded)?;
    Ok(())
  }
}

struct PipeTransport {
  reader: PipeReader,
  writer: PipeWriter,
}

impl PipeTransport {
  pub fn new(reader: PipeReader, writer: PipeWriter) -> Self {
    Self { reader, writer }
  }
}

impl Transport<PipeReader, PipeWriter> for PipeTransport {
  fn split(self) -> (PipeReader, PipeWriter) {
    (self.reader, self.writer)
  }
}