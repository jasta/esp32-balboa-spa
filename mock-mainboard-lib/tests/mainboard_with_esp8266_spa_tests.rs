extern crate core;

use std::{env, fs, io, thread};
use std::fs::DirEntry;
use std::io::{BufRead, Stdin, Stdout, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};
use anyhow::anyhow;
use log::LevelFilter;
use mock_mainboard_lib::main_board::MainBoard;
use mock_mainboard_lib::transport::{StdTransport, Transport};

#[test]
fn esp8266_spa_hello_world() -> anyhow::Result<()> {
  let _ = env_logger::builder().filter_level(LevelFilter::Debug).is_test(true).try_init();

  let cmd_path = RunCBinaryHack::compile("esp8266_spa")?;
  let mut child = Command::new(cmd_path)
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .spawn()?;

  let mut server_out = child.stdin.take().unwrap();
  let mut server_in = child.stdout.take().unwrap();
  let mut client_debug = child.stderr.take().unwrap();

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

/// We can exploit a few side effects of the `cc` crate to get it to run our library as a binary
/// and then hook up stdin/stdout as intended.  This is actually less hacky than it seems as
/// integrating the code as a library would likely make our tests unreliable as the code
/// we're using to test wasn't concerning itself with parallelism and reentry at all, but of
/// course `cargo test` very much would exploit this details to optimize test runs.
///
/// An alternative would've been to create a separate Rust binary that just called our C code's
/// main() function.  We could then use `assert_cmd` as discussed here:
/// https://rust-cli.github.io/book/tutorial/testing.html#testing-cli-applications-by-running-them
/// to invoke the C code as a shelled out sub-command and interact with its stdin/stdout that way.
/// Doing this however still looks a bit hacky to me.
#[derive(Default)]
struct RunCBinaryHack {
}

impl RunCBinaryHack {
  pub fn compile(basename: &str) -> anyhow::Result<PathBuf> {
    let ar_file = Self::find_ar_file(basename)?;
    let out_path = ar_file.parent().unwrap().join(basename);
    let cpp = "g++"; // TODO: this could surely be improved :)
    let status = Command::new(&cpp)
        .arg("-o")
        .arg(&out_path)
        .arg(&ar_file)
        .status()?;
    match status.success() {
      true => Ok(out_path),
      false => Err(anyhow!("{cpp} failed: {status}"))
    }
  }

  fn find_ar_file(basename: &str) -> anyhow::Result<PathBuf> {
    let target = format!("lib{basename}.a");
    Self::find_ar_file_by_ld(&target)
        .or(Self::find_ar_file_by_scan(&target))
  }

  fn find_ar_file_by_ld(target: &str) -> anyhow::Result<PathBuf> {
    let paths = env::var("LD_LIBRARY_PATH")?;
    for path_str in paths.split(":") {
      let possible_path = Path::new(path_str).join(&target);
      if possible_path.exists() {
        return Ok(possible_path);
      }
    }
    Err(anyhow!("Could not find {target} in {paths}"))
  }

  fn find_ar_file_by_scan(target: &str) -> anyhow::Result<PathBuf> {
    let basepath = env::var("CARGO_MANIFEST_DIR").or(env::var("PWD"))?;
    Self::find_file_recursively(&basepath, target)
        .ok_or_else(|| anyhow!("Could not find {target} in {basepath}"))
  }

  fn find_file_recursively(base: impl AsRef<Path>, target: &str) -> Option<PathBuf> {
    if let Ok(entries) = fs::read_dir(base) {
      for entry in entries {
        if let Ok(entry) = entry {
          let entry_path = entry.path();
          let result = if entry_path.is_dir() {
            Self::find_file_recursively(entry_path, target)
          } else if entry.file_name() == target {
            Some(entry.path())
          } else {
            None
          };
          if result.is_some() {
            return result;
          }
        }
      }
    }
    None
  }
}
