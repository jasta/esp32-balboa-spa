//! Implements a way to share a single underlying transport by multiple parties, each
//! receiving the same data and sending along the same write path.  This maps well to how
//! some low-level transport buses work (e.g. RS485) and allows a single physical endpoint to
//! become two or more logical endpoints.
//!
//! Note that this implementation is quite buffer heavy in order to be user friendly and avoid
//! falling into nasty thread safety traps.  Might need some tuning if memory gets tight.

use std::{io, mem, thread};
use std::cmp::min;
use std::io::{BufRead, ErrorKind, Read, Write};
use std::sync::mpsc::{Receiver, sync_channel, SyncSender};
use log::debug;

use crate::transport::Transport;

/// Kind of a silly large value to encourage callers to call flush frequently between writes.
/// This is pretty typical for the kinds of serial lines you find RS485 deployed in so this should
/// work perfectly fine in practice to never be reached.
const MAX_WRITE_BUFFER_SIZE: usize = 2048;

/// Amount of data to buffer when reading from the underlying read stream.  Typically this will
/// be a serial rx line which has a fairly small buffer that we generally want to try to drain
/// as efficiently as possible.
const DEFAULT_RECV_BUFFER_SIZE: usize = 128;

/// Amount of read data segments that we can queue up for each reader.  This is important as each
/// reader is being scheduled independently and we don't want to starve active readers
/// because inactive ones aren't getting CPU time to actually clear their queues.  Uses
/// considerably more memory as this value goes up however as it applies to each
/// attachment to the bus.
const DEFAULT_RECV_QUEUE_LEN: usize = 8;

pub struct BusSwitch<R, W> {
  raw_reader: R,
  raw_writer: W,
  recv_buffer_size: usize,
  recv_queue_len: usize,
  read_listeners: ReadListeners,
  writer_rx: Receiver<WriteAndFlushEvent>,
  writer_tx_tmp: SyncSender<WriteAndFlushEvent>,
}

#[derive(Debug)]
struct ReadEvent(io::Result<Vec<u8>>);

#[derive(Debug)]
struct WriteAndFlushEvent {
  data: Vec<u8>,
  listener_handle: ListenerHandle,
  ack: SyncSender<WriteAckEvent>,
}

#[derive(Debug)]
struct WriteAckEvent(io::Result<()>);

impl<R, W> BusSwitch<R, W>
  where
      R: Read + Send + 'static,
      W: Write + Send + 'static,
{
  pub fn from_existing(
    transport: impl Transport<R, W>,
    recv_buffer_size: usize,
    recv_queue_len: usize
  ) -> Self {
    let (raw_reader, raw_writer) = transport.split();
    let (writer_tx_tmp, writer_rx) = sync_channel(0);
    Self {
      raw_reader,
      raw_writer,
      recv_buffer_size,
      recv_queue_len,
      read_listeners: Default::default(),
      writer_rx,
      writer_tx_tmp,
    }
  }

  pub fn new_connection(&mut self) -> BusTransport {
    let (reader_tx, rx) = sync_channel(self.recv_queue_len);
    let listener_handle = self.read_listeners.add_listener(reader_tx);
    let tx = self.writer_tx_tmp.clone();
    BusTransport { rx, tx, listener_handle }
  }

  pub fn start(self) {
    drop(self.writer_tx_tmp);
    let listeners_for_reader = self.read_listeners.clone();
    let listeners_for_writer = self.read_listeners;
    thread::spawn(move || {
      let reader = ReaderRunner {
        reader: self.raw_reader,
        listeners: listeners_for_reader,
        buffer_size: self.recv_buffer_size,
      };
      reader.run_loop()
    });

    thread::spawn(move || {
      let writer = WriterRunner {
        writer: self.raw_writer,
        listeners: listeners_for_writer,
        rx: self.writer_rx,
      };
      writer.run_loop()
    });
  }
}

struct ReaderRunner<R> {
  reader: R,
  listeners: ReadListeners,
  buffer_size: usize,
}

impl<R: Read> ReaderRunner<R> {
  pub fn run_loop(mut self) -> io::Result<()> {
    let mut buf = vec![0u8; self.buffer_size];

    loop {
      match self.reader.read(buf.as_mut_slice()) {
        Err(e) => {
          self.listeners.broadcast_to_all(Err(copy_io_error(&e)));
          return Err(e);
        }
        Ok(n) => {
          self.listeners.broadcast_to_all(Ok(&buf[0..n]));
          if !self.listeners.has_listeners() || n == 0 {
            return Ok(());
          }
        }
      }
    }
  }
}

struct WriterRunner<W> {
  writer: W,
  rx: Receiver<WriteAndFlushEvent>,
  listeners: ReadListeners,
}

impl<W: Write> WriterRunner<W> {
  pub fn run_loop(mut self) -> io::Result<()> {
    let mut first_error = None;
    loop {
      let event = match self.rx.recv() {
        Ok(e) => e,
        Err(_) => return Ok(()),
      };

      let result = self.write_and_flush(&event.data);
      if first_error.is_none() {
        if let Err(e) = &result {
          first_error = Some(copy_io_error(e));
        }
      }
      let _ = event.ack.send(WriteAckEvent(result));

      self.listeners.broadcast_to_some(Ok(&event.data), Some(event.listener_handle));

      // Do not exit the loop on error!  We only exit when all writer handles are dropped so
      // we can continue to communicate errors accurately.
    }
  }

  fn write_and_flush(&mut self, data: &[u8]) -> io::Result<()> {
    self.writer.write_all(&data)?;
    self.writer.flush()
  }
}

#[derive(Debug, Clone, Default)]
struct ReadListeners {
  listeners: Vec<Listener<ReadEvent>>,
}

impl ReadListeners {
  pub fn add_listener(&mut self, tx: SyncSender<ReadEvent>) -> ListenerHandle {
    let next_index = self.listeners.len();
    let handle = ListenerHandle(next_index);
    self.listeners.push(Listener { handle, tx });
    handle
  }

  pub fn broadcast_to_all(&mut self, result: io::Result<&[u8]>) {
    self.broadcast_to_some(result, None)
  }

  pub fn broadcast_to_some(&mut self, result: io::Result<&[u8]>, exclude: Option<ListenerHandle>) {
    self.listeners.retain(|listener| {
      if exclude != Some(listener.handle) {
        let result_owned = result
            .as_ref()
            .map(|x| x.to_vec())
            .map_err(copy_io_error);
        listener.tx.send(ReadEvent(result_owned)).is_ok()
      } else {
        true
      }
    });
  }

  pub fn has_listeners(&self) -> bool {
    self.listeners.is_empty()
  }
}

#[derive(Debug)]
struct Listener<T> {
  handle: ListenerHandle,
  tx: SyncSender<T>,
}

impl<T> Clone for Listener<T> {
  fn clone(&self) -> Self {
    Self {
      handle: self.handle.clone(),
      tx: self.tx.clone(),
    }
  }
}

#[derive(Debug, Copy, Clone, PartialEq)]
struct ListenerHandle(usize);

fn copy_io_error(e: &io::Error) -> io::Error {
  io::Error::new(e.kind(), format!("forwarded: {e}"))
}

pub struct BusTransport {
  rx: Receiver<ReadEvent>,
  tx: SyncSender<WriteAndFlushEvent>,
  listener_handle: ListenerHandle,
}

impl BusTransport {
  pub fn new_switch<T, R, W>(transport: T) -> BusSwitch<R, W>
    where
        T: Transport<R, W>,
        R: Read + Send + 'static,
        W: Write + Send + 'static,
  {
    BusSwitch::from_existing(transport, DEFAULT_RECV_BUFFER_SIZE, DEFAULT_RECV_QUEUE_LEN)
  }
}

impl Transport<BusTransportRx, BusTransportTx> for BusTransport {
  fn split(self) -> (BusTransportRx, BusTransportTx) {
    let bus_rx = BusTransportRx {
      rx: self.rx,
      buffer: vec![],
      position: 0,
    };
    let bus_tx = BusTransportTx {
      tx: self.tx,
      listener_handle: self.listener_handle,
      buffer: vec![],
      max_buffer_size: MAX_WRITE_BUFFER_SIZE,
    };
    (bus_rx, bus_tx)
  }
}

struct BusTransportRx {
  rx: Receiver<ReadEvent>,
  buffer: Vec<u8>,
  position: usize,
}

impl Read for BusTransportRx {
  fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
    if buf.is_empty() {
      return Ok(0);
    }

    let internal = self.fill_buf()?;

    let len = min(buf.len(), internal.len());
    if len > 0 {
      buf[..len].copy_from_slice(&internal[..len]);
      self.consume(len);
    }
    Ok(len)
  }
}

impl BufRead for BusTransportRx {
  fn fill_buf(&mut self) -> io::Result<&[u8]> {
    while self.position >= self.buffer.len() {
      match self.rx.recv() {
        Err(_) => break,
        Ok(data) => {
          match data.0 {
            Ok(data) => {
              self.buffer = data;
              self.position = 0;
            }
            Err(e) => return Err(e),
          }
        }
      }
    }

    Ok(&self.buffer[self.position..])
  }

  fn consume(&mut self, amt: usize) {
    debug_assert!(self.buffer.len() - self.position >= amt);
    self.position += amt
  }
}

struct BusTransportTx {
  tx: SyncSender<WriteAndFlushEvent>,
  listener_handle: ListenerHandle,
  buffer: Vec<u8>,
  max_buffer_size: usize,
}

impl Write for BusTransportTx {
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    if self.buffer.len() + buf.len() > self.max_buffer_size {
      Err(io::Error::new(ErrorKind::Other, "write buffer exceeded!"))
    } else {
      self.buffer.extend(buf);
      Ok(buf.len())
    }
  }

  fn flush(&mut self) -> io::Result<()> {
    let (ack_tx, ack_rx) = sync_channel(0);
    let mut buffer = vec![];
    mem::swap(&mut self.buffer, &mut buffer);
    let event = WriteAndFlushEvent {
      data: buffer,
      listener_handle: self.listener_handle,
      ack: ack_tx,
    };
    self.tx.send(event).expect("WriterRunner exited before our Tx was dropped?");

    let ack = ack_rx.recv()
        .expect("WriterRunner exited before our ack rx was dropped?");
    ack.0
  }
}

#[cfg(test)]
mod tests {
  use std::collections::hash_map::DefaultHasher;
  use std::collections::HashMap;
  use std::fmt::{Display, Formatter};
  use std::hash::{Hash, Hasher};
  use super::*;
  use std::io::{BufRead, BufReader, Write};
  use std::sync::mpsc;
  use std::sync::mpsc::{channel, Receiver, sync_channel, SyncSender};
  use std::thread;
  use anyhow::anyhow;
  use log::{debug, LevelFilter};
  use crate::transport::{StdTransport, Transport};
  use ntest::timeout;

  #[test]
  #[timeout(10000)]
  fn test_happy_path() -> anyhow::Result<()> {
    let _ = env_logger::builder().filter_level(LevelFilter::Trace).is_test(true).try_init();

    let ((client_in, server_out), (server_in, client_out)) = (pipe::pipe(), pipe::pipe());
    let transport = StdTransport::new(client_in, client_out);

    let mut switch = BusTransport::new_switch(transport);
    let client1 = switch.new_connection();
    let client0 = switch.new_connection();
    switch.start();

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
  #[timeout(7000)]
  fn stress_test() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(LevelFilter::Debug)
        .is_test(true)
        .format(|buf, record| {
          let ts = buf.timestamp_micros();
          writeln!(
            buf,
            "{}: {:?}: {}: {}",
            ts,
            std::thread::current().id(),
            buf.default_level_style(record.level())
                .value(record.level()),
            record.args()
          )
        })
        .init();

    let ((cx_in, s_out), (s_in, cx_out)) = (pipe::pipe(), pipe::pipe());
//     let ((cx_in, s_out), (s_in, cx_out)) = (simple_pipe(), simple_pipe());
    let transport = StdTransport::new(cx_in, cx_out);

    let num_transports = 3;

    let mut harness = BusTestHarness::new();
    harness.add_splits(s_in, s_out);

    let mut switch = BusTransport::new_switch(transport);
    for _ in 1..num_transports {
      harness.add_transport(switch.new_connection());
    }
    switch.start();
    assert_eq!(harness.pairs.len(), num_transports);

    harness.rw_stress(40, 20)?;
    Ok(())
  }

  fn simple_pipe() -> (SimplePipeReader, SimplePipeWriter) {
    let (tx, rx) = sync_channel(128);
    (SimplePipeReader { rx }, SimplePipeWriter { tx })
  }

  struct SimplePipeReader {
    rx: Receiver<SimpleEvent>,
  }

  impl Read for SimplePipeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
      if buf.is_empty() {
        return Ok(0);
      }
      let event = self.rx.recv()
          .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "sender side closed"))?;
      match event {
        SimpleEvent::WriteChar(b) => {
          buf[0] = b;
          Ok(1)
        }
      }
    }
  }

  struct SimplePipeWriter {
    tx: SyncSender<SimpleEvent>
  }

  impl Write for SimplePipeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
      for b in buf {
        self.tx.send(SimpleEvent::WriteChar(*b))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "reader side closed"))?;
      }
      Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
      Ok(())
    }
  }

  pub enum SimpleEvent {
    WriteChar(u8),
  }

  struct BusTestHarness<'d> {
    pairs: Vec<BusTestPair<'d>>,
  }

  struct BusTestPair<'d> {
    index: PairIndex,
    reader: Box<dyn BusTestReadline + Send + 'd>,
    writer: Box<dyn BusTestWriteline + Send + 'd>,
  }

  trait BusTestReadline {
    fn next_line(&mut self) -> Option<io::Result<String>>;
  }

  struct HomogenousReadline<'d, I> {
    reader: Box<dyn Iterator<Item=I> + Send + 'd>,
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
        R: Read + Send + 'd,
        W: Write + Send + 'd
    {
      let (rx, tx) = transport.split();
      self.add_splits(rx, tx)
    }

    pub fn add_splits<R: Read + Send + 'd, W: Write + Send + 'd>(&mut self, rx: R, tx: W) -> PairIndex {
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

    pub fn rw_stress(self, approx_block_size: usize, num_blocks_per: usize) -> anyhow::Result<()> {
      let num_other_writers = self.pairs.len().saturating_sub(1);
      crossbeam::thread::scope(|s| {
        let mut threads = vec![];
        let mut coord_tx_tmp = vec![];
        let mut coord_rx_tmp = vec![];
        for pair in self.pairs.iter() {
          let (tx, rx) = sync_channel(0);
          coord_tx_tmp.push(tx);
          coord_rx_tmp.push(rx);
        }
        let mut coord_tx = HashMap::new();
        let mut coord_rx = HashMap::new();
        for (pair_index, (pair, rx)) in self.pairs.iter().zip(coord_rx_tmp.into_iter()).enumerate() {
          coord_tx.insert(pair.index, vec![]);
          coord_rx.insert(pair.index, rx);
          for (coord_index, tx) in coord_tx_tmp.iter().enumerate() {
            if coord_index != pair_index {
              let tx_vec = coord_tx.get_mut(&pair.index).unwrap();
              tx_vec.push(tx.clone());
            }
          }
        }
        for pair in self.pairs.into_iter() {
          let my_rx = coord_rx.remove(&pair.index).unwrap();
          let other_txs = coord_tx.remove(&pair.index).unwrap();

          let mut reader = pair.reader;
          let mut writer = pair.writer;

          let pair_index = pair.index;
          let writer_thread = s.spawn(move |_| {
            for i in 0..num_blocks_per {
              let block = Block(pair_index.0, i);
              let block_str = block.to_string() + ":";
              let num_repetitions = approx_block_size / block_str.len();
              let data = block_str.repeat(num_repetitions);

              debug!("[{}] => {block}...", pair_index);
              writer.write_line(data.as_str())?;
              debug!("[{}] => sent: {block}", pair_index);

              // Due to the configuration here, this logically waits for all readers to ack
              // the sent message.  This is extremely important because BusTransport is designed
              // for electrical buses that don't guarantee any isolation at the protocol level
              // so we have to put something in even for test.
              let data_hash = hash(data);
              for other_tx in &other_txs {
                let _ = other_tx.send(data_hash);
              }
            }
            debug!("[{}] => FINISHED Writer #{}!", pair_index, pair_index);
            Ok(())
          });
          let reader_thread = s.spawn(move |_| {
            for other in 0..num_other_writers {
              for i in 0..num_blocks_per {
                let block = Block(other, i);
                debug!("[{}] <= trying to read...", pair_index);
                let got_data = reader.next_line()
                    .unwrap_or_else(|| {
                      Err(
                        io::Error::new(
                          ErrorKind::UnexpectedEof,
                          format!("pair #{}, block {block}", pair_index)))
                    })?;
                let got_block = got_data.split(':').next().unwrap();
                debug!("[{}] <= {got_block}", pair_index);

                // if got_block != Some(block.to_string().as_str()) {
                //   return Err(anyhow!("[{}] Mismatch @ block {block}: got={got_block:?}", pair_index));
                // }
                //
                let compute_hash = hash(&got_data);
                let got_hash = my_rx.recv()?;

                if got_hash != compute_hash {
                  return Err(anyhow!("[{}] Mismatch @ block {block}: got={got_hash} ({got_data}), expected={compute_hash}", pair.index));
                }
              }
            }
            debug!("[{}] <= FINISHED Reader #{}!", pair.index, pair.index);
            Ok(())
          });

          threads.push(writer_thread);
          threads.push(reader_thread);
        }

        for thread in threads {
          thread.join().unwrap()?;
        }

        Ok(())
      }).unwrap()
    }
  }

  fn hash<T: Hash>(value: T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
  }

  fn gen_data(len: usize) -> String {
    let data_chars = ('0'..='9')
        .chain('a'..='z')
        .chain('A'..='Z')
        .take(len);
    let data = String::from_iter(data_chars);
    assert_eq!(data.len(), len);
    data
  }

  struct Block(usize, usize);

  impl Display for Block {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
      write!(f, "#{},{}", self.0, self.1)
    }
  }

  #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
  struct PairIndex(usize);

  impl Display for PairIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
      write!(f, "{}", self.0)
    }
  }
}