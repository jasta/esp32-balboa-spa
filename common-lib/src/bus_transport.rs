//! Implements a way to share a single underlying transport by multiple parties, each
//! receiving the same data and sending along the same write path.  This maps well to how
//! some low-level transport buses work (e.g. RS485) and allows a single physical endpoint to
//! become two or more logical endpoints.

use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::io::{ErrorKind, Read, Write};
use std::sync::{Arc, Mutex, PoisonError};
use log::info;
use crate::transport::Transport;

pub struct BusTransport<R, W> {
  inner: SharedWrapper<R, W>,
}

impl <R, W> BusTransport<R, W>
where
    R: Read,
    W: Write,
{
  pub fn new(transport: impl Transport<R, W>, buffer_size: usize) -> Self {
    let (raw_reader, raw_writer) = transport.split();
    let shared_state = SharedState::new(raw_reader, raw_writer, buffer_size);
    Self {
      inner: SharedWrapper::new(shared_state),
    }
  }
}

impl <R, W> Clone for BusTransport<R, W> {
  fn clone(&self) -> Self {
    Self {
      inner: self.inner.clone_add_client(),
    }
  }
}

struct SharedWrapper<R, W> {
  my_index: usize,
  state: Arc<Mutex<SharedState<R, W>>>,
}

impl <R, W> SharedWrapper<R, W> {
  pub fn new(mut state: SharedState<R, W>) -> Self {
    let my_index = state.add_client();
    Self {
      my_index,
      state: Arc::new(Mutex::new(state)),
    }
  }

  pub fn clone_add_client(&self) -> Self {
    let next_index = self.state.lock().unwrap().add_client();
    Self {
      my_index: next_index,
      state: self.state.clone(),
    }
  }
}

impl <R, W> Clone for SharedWrapper<R, W> {
  fn clone(&self) -> Self {
    Self {
      my_index: self.my_index,
      state: self.state.clone(),
    }
  }
}

impl <R, W> Drop for SharedWrapper<R, W> {
  fn drop(&mut self) {
    self.state.lock().unwrap().drop_client(self.my_index);
  }
}

struct SharedState<R, W> {
  raw_reader: R,
  raw_writer: W,
  buffer_size: usize,
  all_buffers: BTreeMap<usize, VecDeque<u8>>,
  got_error: Option<RxTxError>,
}

impl <R, W> SharedState<R, W> {
  pub fn new(raw_reader: R, raw_writer: W, buffer_size: usize) -> Self {
    let all_buffers = BTreeMap::new();
    Self {
      buffer_size,
      raw_reader,
      raw_writer,
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

impl <R, W> Transport<BusTransportRx<R, W>, BusTransportTx<R, W>> for BusTransport<R, W>
where
    R: Read,
    W: Write,
{
  fn split(self) -> (BusTransportRx<R, W>, BusTransportTx<R, W>) {
    let rx = BusTransportRx {
      inner: self.inner.clone(),
    };
    let tx = BusTransportTx {
      inner: self.inner,
    };
    (rx, tx)
  }
}

pub struct BusTransportRx<R, W> {
  inner: SharedWrapper<R, W>,
}

pub type RxEvent = io::Result<Vec<u8>>;

impl <R: Read, W> Read for BusTransportRx<R, W> {
  fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
    if buf.is_empty() {
      return Ok(0);
    }

    let my_index = self.inner.my_index;
    let mut state = self.inner.state.lock().map_err(lock_io_err)?;

    let my_buffer = state.all_buffers.get_mut(&my_index)
        .unwrap_or_else(|| panic!("Internal error: my_index={my_index}"));
    match my_buffer.len() {
      0 => state.do_raw_read(my_index, buf),
      _ => {
        let result = my_buffer.read(buf);
        info!("got {result:?} from buf {my_index}");
        result
      },
    }
  }
}

pub struct BusTransportTx<R, W> {
  inner: SharedWrapper<R, W>,
}

impl <R, W: Write> Write for BusTransportTx<R, W> {
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    self.inner.state.lock()
        .map_err(lock_io_err)?
        .do_raw_write(self.inner.my_index, buf)
  }

  fn flush(&mut self) -> io::Result<()> {
    let raw_writer = &mut self.inner.state.lock()
        .map_err(lock_io_err)?
        .raw_writer;
    raw_writer.flush()
  }
}

fn lock_io_err<T>(error: PoisonError<T>) -> io::Error {
  io::Error::new(ErrorKind::BrokenPipe, format!("{error:?}"))
}

impl <R: Read, W> SharedState<R, W> {
  pub fn do_raw_read(&mut self, my_index: usize, user_buf: &mut [u8]) -> io::Result<usize> {
    if let Some(ref e) = self.got_error {
      return Err(e.into());
    }

    let raw_result = self.raw_reader.read(user_buf);
    self.handle_result(my_index, user_buf, raw_result)
  }
}

impl <R, W: Write> SharedState<R, W> {
  pub fn do_raw_write(&mut self, my_index: usize, user_buf: &[u8]) -> io::Result<usize> {
    if let Some(ref e) = self.got_error {
      return Err(e.into());
    }

    let raw_result = self.raw_writer.write(user_buf);
    self.handle_result(my_index, user_buf, raw_result)
  }
}

impl <R, W> SharedState<R, W> {
  fn handle_result(&mut self, my_index: usize, user_buf: &[u8], result: io::Result<usize>) -> io::Result<usize> {
    match result {
      Ok(n) => {
        for (&index, other_buf) in self.all_buffers.iter_mut() {
          if index != my_index {
            other_buf.extend(&user_buf[0..n]);
            info!("buf {index} is now {} len", other_buf.len());
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

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::{BufRead, BufReader, Write};
  use byteorder::{ReadBytesExt, WriteBytesExt};
  use log::{debug, LevelFilter};
  use crate::transport::{StdTransport, Transport};
  use ntest::timeout;

  #[test]
  #[timeout(10000)]
  fn test_happy_path() -> anyhow::Result<()> {
    let _ = env_logger::builder().filter_level(LevelFilter::Trace).is_test(true).try_init();

    let ((client_in, mut server_out), (server_in, client_out)) = (pipe::pipe(), pipe::pipe());
    let transport = StdTransport::new(client_in, client_out);

    let multiplex = BusTransport::new(transport, 32);
    let client1 = multiplex.clone();
    let client0 = multiplex;

    let (rx0, mut tx0) = client0.split();
    let (rx1, mut tx1) = client1.split();

    let mut server_lines = BufReader::new(server_in).lines();
    let mut client0_lines = BufReader::new(rx0).lines();
    let mut client1_lines = BufReader::new(rx1).lines();
    roundtrip_both(&mut server_out, &mut client0_lines, &mut client1_lines, "hello, clients!")?;
    roundtrip_both(&mut tx0, &mut server_lines, &mut client1_lines, "hello, client1+server!")?;
    roundtrip_both(&mut tx1, &mut server_lines, &mut client0_lines, "hello, client0+server!")?;
    Ok(())
  }

  fn roundtrip_both<'a, R1, R2, W>(out: &mut W, in0: &'a mut R1, in1: &'a mut R2, data: &str) -> io::Result<()>
    where
        R1: Iterator<Item=io::Result<String>>,
        R2: Iterator<Item=io::Result<String>>,
        W: Write + Send
  {
    struct HomogenousIter<'a, I> {
      inner: Box<dyn Iterator<Item=I> + 'a>,
    }
    impl <'a, I> Iterator for HomogenousIter<'a, I> {
      type Item = I;

      fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
      }
    }

    let inputs = [
      HomogenousIter { inner: Box::new(in0) },
      HomogenousIter { inner: Box::new(in1) },
    ];
    roundtrip_all(out, inputs.into_iter(), data)
  }

  fn roundtrip_all<R, W>(out: &mut W, inputs: impl Iterator<Item=R>, data: &str) -> io::Result<()>
  where
      R: Iterator<Item=io::Result<String>>,
      W: Write + Send
  {
    // Need to use another thread because PipeReader/Writer use a bounded channel of size 0 so
    // the write will block forever.
    crossbeam::thread::scope(|s| {
      let out_handle = s.spawn(|_| {
        let data_with_cr = data.to_owned() + "\n";
        out.write_all(data_with_cr.as_bytes())?;
        out.flush()?;
        debug!("Flushed to out: {data}");
        Ok(())
      });

      for (index, mut input) in inputs.enumerate() {
        assert_eq!(input.next().unwrap()?, data, "in{index} mismatch");
        debug!("Read successfully from in{index}");
      }

      out_handle.join().unwrap()
    }).unwrap()
  }

  #[test]
  fn test_drop_before_split() -> anyhow::Result<()> {
    let ((client_in, mut server_out), (_server_in, client_out)) = (pipe::pipe(), pipe::pipe());
    let transport = StdTransport::new(client_in, client_out);

    let multiplex = BusTransport::new(transport, 32);
    let client = multiplex.clone();
    drop(multiplex);

    let (rx, tx) = client.split();

    assert_eq!(rx.inner.state.lock().unwrap().all_buffers.len(), 1);
    let mut rx_lines = BufReader::new(rx).lines();
    roundtrip_all(&mut server_out, [&mut rx_lines].into_iter(), "meh")?;
    drop(tx);
    Ok(())
  }

  #[test]
  fn test_drop_after_split() -> anyhow::Result<()> {
    let ((client_in, mut server_out), (_server_in, client_out)) = (pipe::pipe(), pipe::pipe());
    let transport = StdTransport::new(client_in, client_out);

    let multiplex = BusTransport::new(transport, 32);
    let client = multiplex.clone();

    let (_, _) = multiplex.split();
    let (mut rx, tx) = client.split();

    assert_eq!(rx.inner.state.lock().unwrap().all_buffers.len(), 1);
    let mut rx_lines = BufReader::new(rx).lines();
    roundtrip_all(&mut server_out, [&mut rx_lines].into_iter(), "meh")?;
    drop(tx);
    Ok(())
  }
}