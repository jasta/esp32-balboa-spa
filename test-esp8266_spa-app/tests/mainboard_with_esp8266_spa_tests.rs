extern crate core;

use std::{io, thread};
use std::io::{BufRead};
use std::process::{Command, Stdio};
use std::time::Duration;
use log::LevelFilter;
use mock_mainboard_lib::main_board::MainBoard;
use mock_mainboard_lib::transport::StdTransport;

// Note that this test is _really_ about mock_mainboard_lib testing, but we're putting it here
// because assert_cmd assumes that we're testing a binary from the current crate.
#[test]
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

  // TODO: Manually drive init timing for testing purposes
  let main_board = MainBoard::new(StdTransport::new(server_in, server_out))
      .set_init_delay(Duration::from_secs(0));
  let (shutdown_handle, runner) = main_board.into_runner();

  let run_thread = thread::Builder::new()
      .name("ServerMainThread".into())
      .spawn(move || runner.run_loop())
      .unwrap();

  for line in io::BufReader::new(client_debug).lines() {
    // TODO: Actually drive some logic here... :)
    println!("{}", line?);
  }

  shutdown_handle.request_shutdown();
  child.kill()?;
  run_thread.join().unwrap()?;

  Ok(())
}
