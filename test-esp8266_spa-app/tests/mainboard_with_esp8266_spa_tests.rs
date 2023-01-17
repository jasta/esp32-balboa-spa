extern crate core;

use std::thread;
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use log::LevelFilter;
use mock_mainboard_lib::main_board::MainBoard;
use mock_mainboard_lib::transport::StdTransport;

// Note that this test is _really_ about mock_mainboard_lib testing, but we're putting it here
// because assert_cmd assumes that we're testing a binary from the current crate.
#[test]
#[ntest::timeout(10000)]
fn esp8266_spa_hello_world() -> anyhow::Result<()> {
  let _ = env_logger::builder().filter_level(LevelFilter::Debug).is_test(true).try_init();

  let bin_path = assert_cmd::cargo::cargo_bin("test-esp8266_spa-app");
  let mut child = Command::new(bin_path)
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .spawn()?;

  let server_out = child.stdin.take().unwrap();
  let server_in = child.stdout.take().unwrap();
  let client_debug = child.stderr.take().unwrap();

  let main_board = MainBoard::new(StdTransport::new(server_in, server_out));
  let (control_handle, runner) = main_board.into_runner();

  let run_thread = thread::Builder::new()
      .name("ServerMainThread".into())
      .spawn(move || runner.run_loop())
      .unwrap();

  let mut reader = ReaderHelper::new(client_debug);
  reader.expect("Spa/node/id:16")?;
  reader.expect_all(vec![
    "Spa/config/pumps1:2",
    "Spa/config/pumps2:0",
    "Spa/heatingmode/state:OFF",
  ])?;

  control_handle.complete_init();
  reader.expect_all(vec![
    "Spa/heatingmode/state:ON",
    "Spa/temperature/state:20.000000"
  ])?;

  control_handle.request_shutdown();
  child.kill()?;
  run_thread.join().unwrap()?;

  Ok(())
}

struct ReaderHelper<R> {
  reader: BufReader<R>
}

impl <R: Read> ReaderHelper<R> {
  pub fn new(reader: R) -> Self {
    Self { reader: BufReader::new(reader) }
  }

  pub fn expect(&mut self, line: &str) -> anyhow::Result<()> {
    self.expect_all(vec![line])
  }

  pub fn expect_all(&mut self, mut lines: Vec<&str>) -> anyhow::Result<()> {
    let mut buf = String::new();
    loop {
      buf.clear();
      let _ = self.reader.read_line(&mut buf);
      if buf.ends_with('\n') {
        buf.pop();
        if buf.ends_with('\r') {
          buf.pop();
        }
      }
      println!("{buf}");
      lines.retain(|line| {
        if line == &buf.as_str() {
          println!("Found: {line}");
          false
        } else {
          true
        }
      });
      if lines.is_empty() {
        break;
      }
    }
    Ok(())
  }
}
