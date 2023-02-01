//! Implements a way to share a single underlying transport by multiple parties, each
//! receiving the same data and sending along the same write path.  This maps well to how
//! some low-level transport buses work (e.g. RS485) and allows a single physical endpoint to
//! become two or more logical endpoints.

use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::io::{ErrorKind, Read, Write};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use crate::transport::Transport;

pub struct BusTransport<R, W> {
  raw_reader: Arc<Mutex<R>>,
  raw_writer: Arc<Mutex<W>>,
  shared: SharedWrapper,
}

impl <R, W> BusTransport<R, W>
where
    R: Read,
    W: Write,
{
  pub fn new(transport: impl Transport<R, W>, buffer_size: usize) -> Self {
    let (raw_reader, raw_writer) = transport.split();
    let shared_state = SharedState::new(buffer_size);
    Self {
      raw_reader: Arc::new(Mutex::new(raw_reader)),
      raw_writer: Arc::new(Mutex::new(raw_writer)),
      shared: SharedWrapper::new(shared_state),
    }
  }
}

impl <R, W> Clone for BusTransport<R, W> {
  fn clone(&self) -> Self {
    Self {
      raw_reader: self.raw_reader.clone(),
      raw_writer: self.raw_writer.clone(),
      shared: self.shared.clone_add_client(),
    }
  }
}

struct SharedWrapper {
  my_index: usize,
  state: Arc<RwLock<SharedState>>,
}

impl SharedWrapper {
  pub fn new(mut state: SharedState) -> Self {
    let my_index = state.add_client();
    Self {
      my_index,
      state: Arc::new(RwLock::new(state)),
    }
  }

  pub fn clone_add_client(&self) -> Self {
    let next_index = self.state.write().unwrap().add_client();
    Self {
      my_index: next_index,
      state: self.state.clone(),
    }
  }

  pub fn check_error(&self) -> io::Result<()> {
    self.state.read()
        .map_err(lock_io_err)?
        .check_error()
  }

  pub fn needs_raw_read(&self) -> io::Result<bool> {
    let state = self.state.read().map_err(lock_io_err)?;
    let buffer = state.all_buffers.get(&self.my_index)
        .ok_or_else(dropped_err)?;
    state.check_error()?;
    Ok(buffer.is_empty())
  }

  pub fn buffer_read(&self, buf: &mut [u8]) -> io::Result<usize> {
    let mut state = self.state.write().map_err(lock_io_err)?;
    let buffer = state.all_buffers.get_mut(&self.my_index)
        .ok_or_else(dropped_err)?;

    buffer.read(buf)
  }

  pub fn buffer_append(&self, buf: &[u8], from_result: io::Result<usize>) -> io::Result<usize> {
    let mut state = self.state.write().map_err(lock_io_err)?;
    state.handle_result(self.my_index, buf, from_result)
  }

  pub fn set_error(&self, error: RxTxError) -> io::Result<()> {
    let mut state = self.state.write().map_err(lock_io_err)?;
    state.set_error(error.clone());
    Err(error.into())
  }
}

impl Clone for SharedWrapper {
  fn clone(&self) -> Self {
    Self {
      my_index: self.my_index,
      state: self.state.clone(),
    }
  }
}

impl Drop for SharedWrapper {
  fn drop(&mut self) {
    self.state.write().unwrap().drop_client(self.my_index);
  }
}

struct SharedState {
  buffer_size: usize,
  all_buffers: BTreeMap<usize, VecDeque<u8>>,
  got_error: Option<RxTxError>,
}

impl SharedState {
  pub fn new(buffer_size: usize) -> Self {
    let all_buffers = BTreeMap::new();
    Self {
      buffer_size,
      all_buffers,
      got_error: None,
    }
  }

  pub fn add_client(&mut self) -> usize {
    let next_index = self.all_buffers.len();
    self.all_buffers.insert(next_index, VecDeque::with_capacity(self.buffer_size));
    next_index
  }

  pub fn drop_client(&mut self, client_index: usize) {
    self.all_buffers.remove(&client_index);
  }
}

impl <R, W> Transport<BusTransportRx<R>, BusTransportTx<W>> for BusTransport<R, W>
where
    R: Read,
    W: Write,
{
  fn split(self) -> (BusTransportRx<R>, BusTransportTx<W>) {
    let rx = BusTransportRx {
      reader: self.raw_reader,
      shared: self.shared.clone(),
    };
    let tx = BusTransportTx {
      writer: self.raw_writer,
      shared: self.shared,
    };
    (rx, tx)
  }
}

pub struct BusTransportRx<R> {
  reader: Arc<Mutex<R>>,
  shared: SharedWrapper,
}

pub type RxEvent = io::Result<Vec<u8>>;

impl <R: Read> Read for BusTransportRx<R> {
  fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
    if buf.is_empty() {
      return Ok(0);
    }

    if self.shared.needs_raw_read()? {
      let mut raw_reader = self.reader.lock().map_err(lock_io_err)?;

      // Gotta check again because we don't hold the lock during the read operation -- we could
      // have since modified the shared buffer.
      if self.shared.needs_raw_read()? {
        let result = match raw_reader.read(buf) {
          Ok(0) => self.shared.set_error(RxTxError::UnexpectedEof).map(|_| 0),
          r => self.shared.buffer_append(buf, r),
        };

        // We must hold this lock past when we take the shared write lock so that we can
        // gaurantee once a thread holds the raw reader/writer lock that the shared state
        // is fully consistent with any previous writes.
        drop(raw_reader);

        return result;
      }
    }

    // Didn't need the raw reader, read from the buffer...
    self.shared.buffer_read(buf)
  }
}

pub struct BusTransportTx<W> {
  writer: Arc<Mutex<W>>,
  shared: SharedWrapper,
}

impl <W: Write> Write for BusTransportTx<W> {
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    let mut writer = self.writer.lock().map_err(lock_io_err)?;
    let raw_result = writer.write(buf);

    // Here we can drop the write lock before we take the shared lock because we don't
    // guarantee any ordering rules in the order of concurrent writes (as would be the case
    // if our bus was externally connected).
    drop(writer);

    self.shared.buffer_append(buf, raw_result)
  }

  fn flush(&mut self) -> io::Result<()> {
    let mut writer = self.writer.lock().map_err(lock_io_err)?;
    writer.flush()
  }
}

fn lock_io_err<T>(error: PoisonError<T>) -> io::Error {
  io::Error::new(ErrorKind::BrokenPipe, format!("{error:?}"))
}

fn dropped_err() -> io::Error {
  io::Error::new(ErrorKind::BrokenPipe, "dropped rx or tx")
}

impl SharedState {
  fn check_error(&self) -> io::Result<()> {
    match self.got_error {
      Some(ref e) => Err(e.into()),
      None => Ok(()),
    }
  }

  fn handle_result(&mut self, my_index: usize, user_buf: &[u8], result: io::Result<usize>) -> io::Result<usize> {
    match result {
      Ok(0) => Ok(0),
      Ok(n) => {
          for (&index, other_buf) in self.all_buffers.iter_mut() {
            if index != my_index {
              other_buf.extend(&user_buf[0..n]);
            }
          }
          Ok(n)
      }
      Err(e) => {
        self.got_error = Some(RxTxError::IoError(e.kind(), e.to_string()));
        Err(e)
      }
    }
  }

  fn set_error(&mut self, error: RxTxError) {
    self.got_error = Some(error);
  }
}

#[derive(thiserror::Error, Debug, Clone)]
enum RxTxError {
  #[error("Expected non-zero length read")]
  UnexpectedEof,

  #[error("I/O error: {0} {1}")]
  IoError(ErrorKind, String),
}

impl From<&RxTxError> for io::Error {
  fn from(value: &RxTxError) -> Self {
    let (kind, msg) = match value {
      RxTxError::UnexpectedEof => (ErrorKind::UnexpectedEof, value.to_string()),
      RxTxError::IoError(k, m) => (*k, m.clone()),
    };
    io::Error::new(kind, msg)
  }
}

impl From<RxTxError> for io::Error {
  fn from(value: RxTxError) -> Self {
    (&value).into()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::{BufRead, BufReader, Write};
  use log::{debug, LevelFilter};
  use crate::transport::{StdTransport, Transport};
  use ntest::timeout;

  #[test]
  #[timeout(10000)]
  fn test_happy_path() -> anyhow::Result<()> {
    let _ = env_logger::builder().filter_level(LevelFilter::Trace).is_test(true).try_init();

    let ((client_in, server_out), (server_in, client_out)) = (pipe::pipe(), pipe::pipe());
    let transport = StdTransport::new(client_in, client_out);

    let multiplex = BusTransport::new(transport, 32);
    let client1 = multiplex.clone();
    let client0 = multiplex;

    let mut harness = BusTestHarness::new();
    let s = harness.add_splits(server_in, server_out);
    let c0 = harness.add_transport(client0);
    let c1 = harness.add_transport(client1);

    harness.send_from(s, "hello, clients!")?;
    harness.send_from(c0, "hello client1+server!")?;
    harness.send_from(c1, "hello client0+server!")?;
    Ok(())
  }

  #[test]
  fn test_drop_before_split() -> anyhow::Result<()> {
    let ((client_in, server_out), (server_in, client_out)) = (pipe::pipe(), pipe::pipe());
    let transport = StdTransport::new(client_in, client_out);

    let multiplex = BusTransport::new(transport, 32);
    let client = multiplex.clone();
    drop(multiplex);

    let (rx, tx) = client.split();

    assert_eq!(rx.shared.state.read().unwrap().all_buffers.len(), 1);
    let mut harness = BusTestHarness::new();
    let s = harness.add_splits(server_in, server_out);
    let _c = harness.add_splits(rx, tx);
    harness.send_from(s, "meh")?;
    Ok(())
  }

  #[test]
  fn test_drop_after_split() -> anyhow::Result<()> {
    let ((client_in, server_out), (server_in, client_out)) = (pipe::pipe(), pipe::pipe());
    let transport = StdTransport::new(client_in, client_out);

    let multiplex = BusTransport::new(transport, 32);
    let client = multiplex.clone();

    let (_, _) = multiplex.split();
    let (rx, tx) = client.split();

    assert_eq!(rx.shared.state.read().unwrap().all_buffers.len(), 1);
    let mut harness = BusTestHarness::new();
    let s = harness.add_splits(server_in, server_out);
    let _c = harness.add_splits(rx, tx);
    harness.send_from(s, "meh")?;
    Ok(())
  }

  struct BusTestHarness<'d> {
    pairs: Vec<BusTestPair<'d>>,
  }

  struct BusTestPair<'d> {
    index: PairIndex,
    reader: Box<dyn BusTestReadline + 'd>,
    writer: Box<dyn BusTestWriteline + Send + 'd>,
  }

  trait BusTestReadline {
    fn next_line(&mut self) -> Option<io::Result<String>>;
  }

  struct HomogenousReadline<'d, I> {
    reader: Box<dyn Iterator<Item=I> + 'd>,
  }

  impl <'d> BusTestReadline for HomogenousReadline<'d, io::Result<String>>
  {
    fn next_line(&mut self) -> Option<io::Result<String>> {
      self.reader.next()
    }
  }

  trait BusTestWriteline {
    fn write_line(&mut self, data: &str) -> io::Result<()>;
  }

  struct HomogenousWriteline<'d> {
    writer: Box<dyn Write + Send + 'd>,
  }

  impl <'d> BusTestWriteline for HomogenousWriteline<'d> {
    fn write_line(&mut self, data: &str) -> io::Result<()> {
      self.writer.write_all((data.to_owned() + "\n").as_bytes())?;
      self.writer.flush()?;
      Ok(())
    }
  }

  impl <'d> BusTestHarness<'d> {
    pub fn new() -> Self {
      BusTestHarness { pairs: vec![] }
    }

    pub fn add_transport<T, R, W>(&mut self, transport: T) -> PairIndex
    where
        T: Transport<R, W>,
        R: Read + 'd,
        W: Write + Send + 'd
    {
      let (rx, tx) = transport.split();
      self.add_splits(rx, tx)
    }

    pub fn add_splits<R: Read + 'd, W: Write + Send + 'd>(&mut self, rx: R, tx: W) -> PairIndex {
      let reader = Box::new(HomogenousReadline {
        reader: Box::new(BufReader::new(rx).lines()),
      });
      let writer = Box::new(HomogenousWriteline {
        writer: Box::new(tx),
      });
      let index = PairIndex(self.pairs.len());
      self.pairs.push(BusTestPair { index, reader, writer });
      index
    }

    pub fn send_from(&mut self, pair_index: PairIndex, data: &str) -> io::Result<()> {
      // Need to use another thread because PipeReader/Writer use a bounded channel of size 0 so
      // the write will block forever.
      crossbeam::thread::scope(|s| {
        let (inputs_left, inputs_right) = self.pairs
            .split_at_mut(pair_index.0);
        let mut inputs_right_iter = inputs_right.iter_mut();
        let out = inputs_right_iter.next().unwrap();
        let x =
            inputs_left.iter_mut().chain(inputs_right_iter);
        let out_handle = s.spawn(|_| {
          out.writer.write_line(data)?;
          debug!("Flushed to out: {data}");
          Ok(())
        });

        for input in x {
          let index = input.index.0;
          assert_eq!(input.reader.next_line().unwrap()?, data, "in{index} mismatch");
          debug!("Read successfully from in{index}");
        }

        out_handle.join().unwrap()
      }).unwrap()
    }
  }

  #[derive(Debug, Copy, Clone)]
  struct PairIndex(usize);
}