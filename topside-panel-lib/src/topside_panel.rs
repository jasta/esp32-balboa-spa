use std::collections::HashMap;
use std::fmt::Debug;
use std::io::{BufRead, Read, Write};
use std::time::{Duration, Instant};
use anyhow::anyhow;
use log::{debug, info, warn};
use measurements::Temperature;
use balboa_spa_messages::channel::Channel;
use balboa_spa_messages::framed_reader::FramedReader;
use balboa_spa_messages::framed_writer::FramedWriter;
use balboa_spa_messages::message::Message;
use balboa_spa_messages::message_types::{MessageType, PayloadEncodeError, PayloadParseError};
use common_lib::transport::Transport;
use crate::cts_state_machine::{CtsHandlingError, CTSStateMachine, SendStatus};
use crate::handling_error::HandlingError;
use crate::handling_error::HandlingError::{FatalError, UnexpectedPayload};

pub struct TopsidePanel<R, W> {
  framed_reader: FramedReader<R>,
  framed_writer: FramedWriter<W>,
  cts_state_machine: CTSStateMachine,
}

impl<R: Read, W: Write> TopsidePanel<R, W> {
  pub fn new(transport: impl Transport<R, W>) -> Self {
    let (raw_reader, raw_writer) = transport.split();
    let framed_reader = FramedReader::new(raw_reader);
    let framed_writer = FramedWriter::new(raw_writer);
    Self {
      framed_reader,
      framed_writer,
      cts_state_machine: CTSStateMachine::new(),
    }
  }

  pub fn run_loop(mut self) -> anyhow::Result<()> {
    loop {
      match self.handle_next_message() {
        Ok(_) => {},
        Err(FatalError(m)) => return Err(anyhow!("{m}")),
        Err(UnexpectedPayload(m)) => warn!("{m}"),
      }
    }
  }

  fn handle_next_message(&mut self) -> Result<(), HandlingError> {
    let message = self.framed_reader.next_message()
        .map_err(|e| FatalError(e.to_string()))?;

    let mt = MessageType::try_from(&message)
        .map_err(|e| UnexpectedPayload(e.to_string()))?;

    match self.cts_state_machine.handle_message(&mut self.framed_writer, &message.channel, &mt)? {
      SendStatus::Clear => {
        warn!("Clear to send, but nothing to say...");
        self.framed_writer.write(&MessageType::NothingToSend().to_message(message.channel)?)
            .map_err(|e| HandlingError::FatalError(e.to_string()))?;
        Ok(())
      }
      SendStatus::NotClear => Ok(()),
    }
  }
}

impl From<CtsHandlingError> for HandlingError {
  fn from(value: CtsHandlingError) -> Self {
    match value {
      CtsHandlingError::FatalError(m) => FatalError(m),
    }
  }
}

impl From<PayloadEncodeError> for HandlingError {
  fn from(value: PayloadEncodeError) -> Self {
    match value {
      PayloadEncodeError::GenericError(e) => FatalError(format!("{e:?})")),
      PayloadEncodeError::GenericIoError(e) => FatalError(format!("{e:?}")),
      PayloadEncodeError::NotSupported => FatalError("Not supported".to_owned()),
    }
  }
}
