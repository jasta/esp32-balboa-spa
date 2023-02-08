//! Pretty print a stream of Balboa spa packets for easy debugging of what's going on.

use std::io::stdin;
use balboa_spa_messages::framed_reader::FramedReader;
use balboa_spa_messages::message_types::{MessageType, PayloadParseError};

fn main() {
  let stdin = stdin().lock();
  let reader = FramedReader::new(stdin);

  for message in reader {
    match MessageType::try_from(&message) {
      Ok(mt) => {
        let channel = &message.channel;
        println!("[{channel:?}] {mt:?}");
      }
      Err(e) => {
        println!("Parse error {e} on: {message:?}");
      }
    }
  }
}