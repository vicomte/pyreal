//! The patch as one TCP stream (client divergence CD-044).
//!
//! A server that knows it (Empyrean, V516) answers a client that asks (`FLAG_BULK` in `0xF7E6`)
//! with `0xF7ED` in place of the patch's `0xF7E2`s: a port and a token. A thread connects to that
//! port at the server's address, sends the magic, the version and the token, and reads the records
//! as `DddData` bodies, each behind its length, until a length of 0. TCP's flow control paces it;
//! the channel to the frame loop holds a bounded number of records, so a client that applies them
//! more slowly than they arrive slows the stream rather than piling them up. The frame loop
//! applies each one as the `0xF7E2` it stands for. A stream that cannot be opened or breaks is
//! declined (`0xF7EE`) with the records still pending, which the server then sends as `0xF7E2`s.
//! Any other server never offers one, and the patch is retail's.

use std::io::{BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, TryRecvError};
#[cfg(test)]
use std::sync::Arc;
use std::time::{Duration, Instant};

use pyreal_protocol::admin::{DddData, BULK_MAGIC, BULK_VERSION};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// The records waiting for the frame loop at most.
const QUEUED: usize = 256;
/// The largest record body the stream may carry.
const LARGEST: usize = 64 << 20;

/// How long a frame may spend applying the stream's records: the rest of the frame draws the
/// screens and keeps the session's messages moving, so the progress shows and the server's
/// messages are answered while the patch is written. A frame that may apply any applies at least
/// one.
pub const FRAME_SHARE: Duration = Duration::from_millis(8);

/// The clock a frame's share of the stream is measured by: the time since some fixed moment.
/// The client's reads the machine's clock; a test's moves as the test says.
pub struct WorkClock(Box<dyn FnMut() -> Duration + Send>);

impl WorkClock {
    /// A clock that reads `now`.
    pub fn new(now: impl FnMut() -> Duration + Send + 'static) -> Self {
        Self(Box::new(now))
    }

    /// The time since the clock's fixed moment.
    pub fn now(&mut self) -> Duration {
        (self.0)()
    }
}

impl Default for WorkClock {
    /// The machine's clock.
    fn default() -> Self {
        let start = Instant::now();
        Self::new(move || start.elapsed())
    }
}

impl std::fmt::Debug for WorkClock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkClock")
    }
}

#[derive(Debug)]
enum Msg {
    Record(Box<DddData>),
    Done,
    Failed(String),
}

/// A patch stream being read.
#[derive(Debug)]
pub struct BulkDownload {
    rx: Receiver<Msg>,
    end: Option<Result<(), String>>,
    started: Instant,
    records: u64,
    bytes: u64,
    /// The record bytes the stream's thread has read.
    #[cfg(test)]
    read: Arc<AtomicUsize>,
}

impl BulkDownload {
    /// Open the stream at `addr` with `token`, on a thread of its own.
    ///
    /// # Errors
    /// The thread cannot start.
    pub fn start(addr: SocketAddr, token: [u8; 16]) -> std::io::Result<Self> {
        let (tx, rx) = sync_channel(QUEUED);
        #[cfg(test)]
        let read_bytes = Arc::new(AtomicUsize::new(0));
        #[cfg(test)]
        let counted = Arc::clone(&read_bytes);
        std::thread::Builder::new()
            .name("patch stream".to_owned())
            .spawn(move || {
                let end = match read(addr, token, |d| {
                    #[cfg(test)]
                    counted.fetch_add(d.data.len(), Ordering::Relaxed);
                    tx.send(Msg::Record(d)).is_ok()
                }) {
                    Ok(()) => Msg::Done,
                    Err(e) => Msg::Failed(e.to_string()),
                };
                let _ = tx.send(end);
            })?;
        Ok(Self {
            rx,
            end: None,
            started: Instant::now(),
            records: 0,
            bytes: 0,
            #[cfg(test)]
            read: read_bytes,
        })
    }

    /// The next record that has arrived, if one has.
    pub fn next_record(&mut self) -> Option<Box<DddData>> {
        if self.end.is_some() {
            return None;
        }
        match self.rx.try_recv() {
            Ok(Msg::Record(d)) => {
                self.records += 1;
                self.bytes += d.data.len() as u64;
                Some(d)
            }
            Ok(Msg::Done) => {
                self.end = Some(Ok(()));
                None
            }
            Ok(Msg::Failed(e)) => {
                self.end = Some(Err(e));
                None
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.end = Some(Err("the stream's thread ended".to_owned()));
                None
            }
        }
    }

    /// The record bytes the stream has read so far, applied or not.
    #[cfg(test)]
    pub(crate) fn queued(&self) -> usize {
        self.read.load(Ordering::Relaxed)
    }

    /// How the stream ended, once every record it brought has been drained.
    #[must_use]
    pub fn ended(&self) -> Option<&Result<(), String>> {
        self.end.as_ref()
    }

    /// Records and bytes drained, and the seconds since the stream started.
    #[must_use]
    pub fn progress(&self) -> (u64, u64, f64) {
        (
            self.records,
            self.bytes,
            self.started.elapsed().as_secs_f64(),
        )
    }
}

/// Read the stream at `addr`, handing each record to `take` until it answers false or the stream
/// ends with a length of 0.
fn read(
    addr: SocketAddr,
    token: [u8; 16],
    mut take: impl FnMut(Box<DddData>) -> bool,
) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};
    let mut s = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)?;
    s.set_read_timeout(Some(READ_TIMEOUT))?;
    let mut hello = Vec::with_capacity(24);
    hello.extend_from_slice(&BULK_MAGIC);
    hello.extend_from_slice(&BULK_VERSION.to_le_bytes());
    hello.extend_from_slice(&token);
    s.write_all(&hello)?;
    let mut r = BufReader::with_capacity(1 << 20, s);
    loop {
        let mut len = [0u8; 4];
        r.read_exact(&mut len)?;
        let len = u32::from_le_bytes(len) as usize;
        if len == 0 {
            return Ok(());
        }
        if len > LARGEST {
            return Err(Error::new(ErrorKind::InvalidData, "a record too large"));
        }
        let mut body = vec![0u8; len];
        r.read_exact(&mut body)?;
        let d = pyreal_protocol::read_body::<DddData>(&body)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e.to_string()))?;
        if !take(Box::new(d)) {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn record(id: u32) -> DddData {
        DddData {
            dat_file_type: 0,
            dat_file_id: 1,
            resource_type: 6,
            resource_id: id,
            iteration: 2073,
            compressed: 0,
            version: 3,
            data_size: 4 + 3,
            data: vec![1, 2, 3],
        }
    }

    /// A server that checks the hello, then writes `ids` and, when `finish`, the closing 0.
    fn serve(ids: Vec<u32>, finish: bool) -> (SocketAddr, [u8; 16]) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let token = [9u8; 16];
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut hello = [0u8; 24];
            s.read_exact(&mut hello).unwrap();
            assert_eq!(&hello[..4], b"DBLK");
            assert_eq!(&hello[8..], &[9u8; 16]);
            for id in ids {
                let body = pyreal_protocol::write_body(&record(id)).unwrap();
                let len = u32::try_from(body.len()).unwrap();
                s.write_all(&len.to_le_bytes()).unwrap();
                s.write_all(&body).unwrap();
            }
            if finish {
                s.write_all(&0u32.to_le_bytes()).unwrap();
            }
        });
        (addr, token)
    }

    fn all(d: &mut BulkDownload) -> Vec<u32> {
        let mut ids = Vec::new();
        let until = Instant::now() + Duration::from_secs(10);
        while d.ended().is_none() && Instant::now() < until {
            while let Some(r) = d.next_record() {
                ids.push(r.resource_id);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        ids
    }

    #[test]
    fn a_stream_brings_every_record_then_ends_and_a_cut_one_fails() {
        let (addr, token) = serve((1..=600).collect(), true);
        let mut d = BulkDownload::start(addr, token).unwrap();
        assert_eq!(all(&mut d), (1..=600).collect::<Vec<_>>());
        assert_eq!(d.ended(), Some(&Ok(())));
        assert_eq!(d.progress().0, 600);

        let (addr, token) = serve(vec![7, 8], false);
        let mut d = BulkDownload::start(addr, token).unwrap();
        assert_eq!(all(&mut d), vec![7, 8]);
        assert!(matches!(d.ended(), Some(Err(_))), "closed without its 0");
    }

    #[test]
    fn a_stream_nobody_answers_fails() {
        let addr = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let mut d = BulkDownload::start(addr, [0; 16]).unwrap();
        assert!(all(&mut d).is_empty());
        assert!(matches!(d.ended(), Some(Err(_))));
    }
}
