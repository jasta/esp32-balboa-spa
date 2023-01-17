extern crate core;

use std::io::{Read, Write};
use std::thread;
use std::time::Duration;
use anyhow::anyhow;
use log::LevelFilter;
use pipe::{PipeReader, PipeWriter};
use balboa_spa_messages::channel::Channel;
use balboa_spa_messages::frame_decoder::FrameDecoder;
use balboa_spa_messages::frame_encoder::FrameEncoder;
use balboa_spa_messages::framed_reader::FramedReader;
use balboa_spa_messages::framed_writer::FramedWriter;
use balboa_spa_messages::message::Message;
use balboa_spa_messages::message_types::{MessageType, MessageTypeKind, PayloadParseError, SettingsRequestMessage};
use mock_mainboard_lib::main_board::MainBoard;
use mock_mainboard_lib::transport::{StdTransport, Transport};

#[test]
fn mainboard_get_version() -> anyhow::Result<()> {
  let _ = env_logger::builder().filter_level(LevelFilter::Debug).is_test(true).try_init();

  let ((mut client_in, server_out), (server_in, client_out)) = (pipe::pipe(), pipe::pipe());
  let main_board = MainBoard::new(StdTransport::new(server_in, server_out))
      .set_clear_to_send_window(Duration::MAX);
  let (shutdown_handle, runner) = main_board.into_runner();

  let run_thread = thread::Builder::new()
      .name("ServerMainThread".into())
      .spawn(move || runner.run_loop())
      .unwrap();

  let mut framed_reader = FramedReader::new(client_in);
  let mut framed_writer = FramedWriter::new(client_out);

  let mut state = GetVersionTestState::NeedChannel_WaitingCTS;
  let mut my_channel = None;

  // Note that we're using an event loop similar to the non-test implementations because it is more
  // flexible and can detect stateful errors earlier, more clearly, and more consistently.
  let board_model = loop {
    let message = framed_reader.next_message()?;
    match (message.channel, MessageType::try_from(&message)?) {
      (Channel::MulticastChannelAssignment, MessageType::NewClientClearToSend()) => {
        assert_eq!(state, GetVersionTestState::NeedChannel_WaitingCTS);
        framed_writer.write(
          &MessageType::ChannelAssignmentRequest {
            device_type: 0x0,
            client_hash: 0xcafe,
          }.to_message(Channel::MulticastChannelAssignment)?)?;
        state = GetVersionTestState::NeedChannel_WaitingAssignment;
      }
      (Channel::MulticastChannelAssignment, MessageType::ChannelAssignmentResponse { channel, .. }) => {
        assert_eq!(state, GetVersionTestState::NeedChannel_WaitingAssignment);
        my_channel = Some(channel);
        framed_writer.write(&MessageType::ChannelAssignmentAck().to_message(channel)?)?;
        state = GetVersionTestState::NeedInfo_WaitingCTS;
      }
      (channel, MessageType::ClearToSend()) => {
        assert_eq!(state, GetVersionTestState::NeedInfo_WaitingCTS);
        assert_eq!(Some(channel), my_channel);
        framed_writer.write(
          &MessageType::SettingsRequest(SettingsRequestMessage::Information)
              .to_message(channel)?)?;
        state = GetVersionTestState::NeedInfo_WaitingInfo;
      }
      (channel, MessageType::InformationResponse(info)) => {
        assert_eq!(state, GetVersionTestState::NeedInfo_WaitingInfo);
        assert_eq!(Some(channel), my_channel);
        break info.system_model_number;
      }
      (channel, MessageType::StatusUpdate(status)) => {
        // Ignore...
      }
      _ => panic!("Unhandled message={message:?}"),
    }
  };

  assert_eq!(board_model, "Mock Spa");

  shutdown_handle.request_shutdown();
  drop(framed_reader);
  drop(framed_writer);
  run_thread.join().unwrap()?;

  Ok(())
}

#[derive(Debug, PartialEq, Clone)]
enum GetVersionTestState {
  NeedChannel_WaitingCTS,
  NeedChannel_WaitingAssignment,
  NeedInfo_WaitingCTS,
  NeedInfo_WaitingInfo,
}
