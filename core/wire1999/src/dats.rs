//! The 1999 dat push (`0xF7B8` begin, `0xF7B7` records, `0xF7BB` touch list) and the cell request
//! (`0xF7A9`, answered by `0xF7AC` or `0xF7AB`), turned into the later data patch Pyreal's patcher
//! already applies: `0xF7E7` naming every record, one `0xF7E2` per record, then `0xF7EA`.
//!
//! The 1999 begin names counts, not ids, so the patch opens with an interrogation the server never
//! sent: each record is then an early save, stored as it arrives, and the later begin naming them
//! all follows when the 1999 client's counting rule ends the download, ahead of the roster.

use std::collections::{BTreeMap, BTreeSet};

use pyreal_dat::{ContainerEra, DatKind};
use pyreal_primitives::DataId;
use pyreal_primitives::NetQueue;
use pyreal_protocol::admin::{
    DddBeginDdd, DddData, DddError, DddInterrogation, DddRequestData, PatchRevision,
};
use pyreal_protocol::{Message, Opcode};

use crate::strings::{ReadError, Reader, Writer};
use crate::translate::{Context, Translate};

/// `0xF7B8`, the push's begin.
pub const BEGIN: u32 = 0xF7B8;
/// `0xF7B7`, one record of the push.
pub const RECORD: u32 = 0xF7B7;
/// `0xF7BB`, a touch list, which counts as one record.
pub const TOUCH_LIST: u32 = 0xF7BB;
/// `0xF7AC`, one cell record answering a request.
pub const CELL_RECORD: u32 = 0xF7AC;
/// `0xF7AB`, up to two cell records answering a request.
pub const CELL_PAIR: u32 = 0xF7AB;
/// `0xF7A9`, the client's cell request.
pub const CELL_REQUEST: u32 = 0xF7A9;
/// The 1999 queue for cell requests.
pub const DAT_QUEUE: u16 = 7;
/// The later error for a cell the 1999 wire cannot ask for.
const NOT_AVAILABLE: u32 = 1;

/// A record must inflate to fewer bytes than this, as the 1999 client requires.
pub const MAX_INFLATED: u32 = 0x40_0000;
/// The later wire's portal file pair.
const PORTAL_FILE: (u32, u32) = (0, 1);
/// The later wire's cell file pair.
const CELL_FILE: (u32, u32) = (1, 2);
/// The record version the later data message carries.
const RECORD_VERSION: u32 = 1;

/// `0xF7B8`: each file's iteration and record count, then two byte totals for a progress bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Begin {
    pub portal_iteration: u32,
    pub portal_records: u32,
    pub cell_iteration: u32,
    pub cell_records: u32,
    pub portal_bytes: u32,
    pub cell_bytes: u32,
}

impl Begin {
    /// # Errors
    /// [`ReadError`] for a short message or anything left over.
    pub fn read(message: &[u8]) -> Result<Self, ReadError> {
        let mut r = Reader::after_opcode(message);
        let b = Self {
            portal_iteration: r.u32()?,
            portal_records: r.u32()?,
            cell_iteration: r.u32()?,
            cell_records: r.u32()?,
            portal_bytes: r.u32()?,
            cell_bytes: r.u32()?,
        };
        r.finish()?;
        Ok(b)
    }

    #[must_use]
    pub fn write(&self) -> Vec<u8> {
        let mut w = Writer::new(BEGIN);
        w.u32(self.portal_iteration)
            .u32(self.portal_records)
            .u32(self.cell_iteration)
            .u32(self.cell_records)
            .u32(self.portal_bytes)
            .u32(self.cell_bytes);
        w.into_inner()
    }

    /// Records the download counts to, touch lists included.
    #[must_use]
    pub fn expected(&self) -> u64 {
        u64::from(self.portal_records) + u64::from(self.cell_records)
    }
}

/// One record, inflated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub id: u32,
    /// The 1999 cache number: 0 to 2 are `cell.dat`'s, any other `portal.dat`'s.
    pub cache: u32,
    pub bytes: Vec<u8>,
}

impl Record {
    /// `0xF7B7`: id, cache, compressed length, inflated length, the zlib stream, padding.
    ///
    /// # Errors
    /// [`ReadError`] for a short message, an inflated length out of range, or a stream that does
    /// not inflate to it.
    pub fn read(message: &[u8]) -> Result<Self, ReadError> {
        let mut r = Reader::after_opcode(message);
        let id = r.u32()?;
        let cache = r.u32()?;
        let bytes = read_stream(&mut r)?;
        r.finish()?;
        Ok(Self { id, cache, bytes })
    }

    /// `0xF7AC`: id, compressed length, inflated length, the stream, padding; the cache follows
    /// from the id.
    ///
    /// # Errors
    /// As [`Record::read`].
    pub fn read_cell(message: &[u8]) -> Result<Self, ReadError> {
        let mut r = Reader::after_opcode(message);
        let id = r.u32()?;
        let bytes = read_stream(&mut r)?;
        r.finish()?;
        Ok(Self {
            id,
            cache: cell_cache(id),
            bytes,
        })
    }

    /// `0xF7AB`: two cell records back to back, each an id (0 for none) then, when non-zero, its
    /// lengths and stream with no padding between; padding at the end.
    ///
    /// # Errors
    /// As [`Record::read`].
    pub fn read_pair(message: &[u8]) -> Result<Vec<Self>, ReadError> {
        let mut r = Reader::after_opcode(message);
        let mut out = Vec::with_capacity(2);
        for _ in 0..2 {
            let id = r.u32()?;
            if id != 0 {
                let bytes = read_unpadded_stream(&mut r)?;
                out.push(Self {
                    id,
                    cache: cell_cache(id),
                    bytes,
                });
            }
        }
        r.align()?;
        r.finish()?;
        Ok(out)
    }

    /// Whether the record goes in `cell.dat`.
    #[must_use]
    pub fn in_cell(&self) -> bool {
        self.cache <= 2
    }

    /// Whether a 1999 file of the kind the cache names can hold this id; Pyreal's patcher refuses
    /// the whole world over one that does not.
    #[must_use]
    pub fn fits(&self) -> bool {
        let id = DataId(self.id);
        if self.in_cell() {
            pyreal_dat::classify_cell_id(id).is_some() && cell_cache(self.id) == self.cache
        } else {
            pyreal_dat::divine_type_in(ContainerEra::Classic, id)
                .is_some_and(|t| t.dat() == DatKind::Portal)
        }
    }

    /// The later data message: uncompressed, at `iteration`, under `resource_type`.
    #[must_use]
    pub fn to_later(&self, iteration: u32, resource_type: u32) -> DddData {
        let (dat_file_type, dat_file_id) = if self.in_cell() {
            CELL_FILE
        } else {
            PORTAL_FILE
        };
        DddData {
            dat_file_type,
            dat_file_id,
            resource_type,
            resource_id: self.id,
            iteration,
            compressed: 0,
            version: RECORD_VERSION,
            data_size: u32::try_from(self.bytes.len() + 4).unwrap_or(u32::MAX),
            data: self.bytes.clone(),
        }
    }
}

/// The compressed and inflated lengths, the stream, then padding to four.
fn read_stream(r: &mut Reader<'_>) -> Result<Vec<u8>, ReadError> {
    let bytes = read_unpadded_stream(r)?;
    r.align()?;
    Ok(bytes)
}

fn read_unpadded_stream(r: &mut Reader<'_>) -> Result<Vec<u8>, ReadError> {
    let packed = usize::try_from(r.u32()?).map_err(|_| ReadError::Bad("compressed length"))?;
    let inflated = r.u32()?;
    if inflated == 0 || inflated >= MAX_INFLATED {
        return Err(ReadError::Bad("inflated length"));
    }
    let stream = r.bytes(packed)?;
    let want = usize::try_from(inflated).map_err(|_| ReadError::Bad("inflated length"))?;
    pyreal_dat::inflate_zlib(stream, want).map_err(|_| ReadError::Bad("zlib stream"))
}

/// `0xF7BB`: a count, then that many record ids.
///
/// # Errors
/// [`ReadError`].
pub fn read_touch_list(message: &[u8]) -> Result<Vec<u32>, ReadError> {
    let mut r = Reader::after_opcode(message);
    let count = r.u32()?;
    let mut ids = Vec::new();
    for _ in 0..count {
        ids.push(r.u32()?);
    }
    r.finish()?;
    Ok(ids)
}

/// The cache a cell id is asked for under: landblock 0, landblock information 1, cell 2.
#[must_use]
pub fn cell_cache(id: u32) -> u32 {
    match id & 0xFFFF {
        0xFFFF => 0,
        0xFFFE => 1,
        _ => 2,
    }
}

/// `0xF7A9`: the id, then its cache.
#[must_use]
pub fn cell_request(id: u32) -> Vec<u8> {
    let mut w = Writer::new(CELL_REQUEST);
    w.u32(id).u32(cell_cache(id));
    w.into_inner()
}

/// What the push hands up, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatOut {
    /// Opens the patch, so the records that follow are early saves and the patch screen shows.
    Interrogation(DddInterrogation),
    Begin(DddBeginDdd),
    Data(Box<DddData>),
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Phase {
    /// No begin with iterations yet.
    #[default]
    Idle,
    /// Armed by `begin`: records in so far, touch lists counted.
    Armed { begin: Begin, got: u64 },
    /// Over for this connection. The server repeats the same push after a deletion and after a
    /// log-off, from the same login iterations, so a repeat is passed over.
    Done,
}

/// One connection's dat push and cell requests. No sockets and no clock.
#[derive(Debug, Default)]
pub struct DatPush {
    /// The portal and cell iterations the login reported, once known.
    login: Option<(u32, u32)>,
    phase: Phase,
    /// The ids handed up so far in each file, which the later begin names at the end.
    portal: BTreeSet<u32>,
    cell: BTreeSet<u32>,
    /// Record bytes handed up so far.
    bytes: u64,
    /// The cell iteration a cell answer is stored at: the push's, else the login's.
    cell_iteration: u32,
    /// Outstanding cell requests by id, with the later resource type they were asked under.
    requests: BTreeMap<u32, u32>,
    /// Records not handed up, for a log line.
    dropped: u32,
}

impl DatPush {
    /// A push for a login that reported `login` (portal, cell).
    #[must_use]
    pub fn new(login: (u32, u32)) -> Self {
        Self {
            login: Some(login),
            cell_iteration: login.1,
            ..Self::default()
        }
    }

    /// Whether `opcode` is one of the push's or a cell answer.
    #[must_use]
    pub fn handles(opcode: u32) -> bool {
        matches!(
            opcode,
            BEGIN | RECORD | TOUCH_LIST | CELL_RECORD | CELL_PAIR
        )
    }

    /// Records read but not handed up.
    #[must_use]
    pub fn dropped(&self) -> u32 {
        self.dropped
    }

    /// Whether the download is over.
    #[must_use]
    pub fn done(&self) -> bool {
        self.phase == Phase::Done
    }

    /// One server message whose opcode [`Self::handles`].
    ///
    /// # Errors
    /// [`ReadError`] for a begin or cell answer that does not read. A push record or touch list
    /// that does not read still counts, so the download can end, and is dropped.
    pub fn on_message(&mut self, opcode: u32, message: &[u8]) -> Result<Vec<DatOut>, ReadError> {
        match opcode {
            BEGIN => Ok(self.on_begin(Begin::read(message)?)),
            RECORD => {
                let record = Record::read(message);
                Ok(self.on_counted(record.map(Some)))
            }
            TOUCH_LIST => {
                let list = read_touch_list(message);
                Ok(self.on_counted(list.map(|_| None)))
            }
            CELL_RECORD => Ok(self.on_cells(vec![Record::read_cell(message)?])),
            CELL_PAIR => Ok(self.on_cells(Record::read_pair(message)?)),
            _ => Ok(Vec::new()),
        }
    }

    fn on_begin(&mut self, begin: Begin) -> Vec<DatOut> {
        if begin.portal_iteration == 0 || begin.cell_iteration == 0 {
            return Vec::new();
        }
        match self.phase {
            Phase::Idle => {
                tracing::info!(
                    "1999: dat push of {} portal and {} cell records, iterations {} and {}",
                    begin.portal_records,
                    begin.cell_records,
                    begin.portal_iteration,
                    begin.cell_iteration
                );
                self.phase = Phase::Armed { begin, got: 0 };
                if begin.expected() == 0 {
                    Vec::new()
                } else {
                    vec![DatOut::Interrogation(DddInterrogation::default())]
                }
            }
            Phase::Armed { begin, got } if got >= begin.expected() => self.finish(begin),
            Phase::Armed { .. } | Phase::Done => Vec::new(),
        }
    }

    /// A record or touch list, counted toward the begin's total. A record is handed up at once.
    fn on_counted(&mut self, record: Result<Option<Record>, ReadError>) -> Vec<DatOut> {
        let Phase::Armed { begin, got } = self.phase else {
            self.dropped += 1;
            tracing::debug!("1999: a dat record outside a download is passed over");
            return Vec::new();
        };
        let mut out = Vec::new();
        match record {
            Ok(Some(r)) if r.fits() => {
                let (ids, iteration) = if r.in_cell() {
                    (&mut self.cell, begin.cell_iteration)
                } else {
                    (&mut self.portal, begin.portal_iteration)
                };
                ids.insert(r.id);
                self.bytes += r.bytes.len() as u64;
                out.push(DatOut::Data(Box::new(r.to_later(iteration, 0))));
            }
            Ok(Some(r)) => {
                self.dropped += 1;
                tracing::warn!(
                    "1999: record {:#010X} under cache {} has no place in a 1999 file",
                    r.id,
                    r.cache
                );
            }
            Ok(None) => {}
            Err(e) => {
                self.dropped += 1;
                tracing::warn!("1999: a dat record did not read: {e:?}");
            }
        }
        let got = got + 1;
        self.phase = Phase::Armed { begin, got };
        if got >= begin.expected() {
            out.extend(self.finish(begin));
        }
        out
    }

    /// The download is over: the later begin naming every record handed up, then the end. The
    /// begin is also what records each file's new iteration.
    fn finish(&mut self, begin: Begin) -> Vec<DatOut> {
        self.phase = Phase::Done;
        self.cell_iteration = begin.cell_iteration;
        let login = self.login.unwrap_or_default();
        let mut revisions = Vec::new();
        for (file, iteration, login, ids) in [
            (
                PORTAL_FILE,
                begin.portal_iteration,
                login.0,
                std::mem::take(&mut self.portal),
            ),
            (
                CELL_FILE,
                begin.cell_iteration,
                login.1,
                std::mem::take(&mut self.cell),
            ),
        ] {
            // An iteration the login already reported, with nothing sent, is no revision: the
            // server echoes the client's own when it cannot bring it up to date.
            if ids.is_empty() && iteration <= login {
                continue;
            }
            revisions.push(PatchRevision {
                dat_file_type: file.0,
                dat_file_id: file.1,
                iteration,
                ids_to_download: ids.into_iter().collect(),
                ids_to_purge: Vec::new(),
            });
        }
        let mut out = Vec::new();
        if !revisions.is_empty() {
            out.push(DatOut::Begin(DddBeginDdd {
                data_expected: u32::try_from(self.bytes).unwrap_or(u32::MAX),
                revisions,
            }));
        }
        out.push(DatOut::End);
        out
    }

    /// Cell answers, which count toward no download and are stored at once.
    fn on_cells(&mut self, records: Vec<Record>) -> Vec<DatOut> {
        let mut out = Vec::new();
        for r in records {
            let resource_type = self.requests.remove(&r.id);
            if !r.fits() {
                self.dropped += 1;
                tracing::warn!("1999: cell answer {:#010X} is no cell id", r.id);
                continue;
            }
            if resource_type.is_none() {
                tracing::debug!("1999: cell answer {:#010X} was not asked for", r.id);
            }
            out.push(DatOut::Data(Box::new(
                r.to_later(self.cell_iteration, resource_type.unwrap_or(0)),
            )));
        }
        out
    }

    /// Pyreal's run-time `0xF7E3` as the 1999 cell request, or `None` for an id no cell file
    /// holds, which the 1999 wire cannot ask for.
    pub fn request(&mut self, req: DddRequestData) -> Option<Vec<u8>> {
        pyreal_dat::classify_cell_id(DataId(req.resource_id))?;
        self.requests.insert(req.resource_id, req.resource_type);
        Some(cell_request(req.resource_id))
    }
}

impl Translate for DatPush {
    fn down(
        &mut self,
        opcode: u32,
        _queue: u16,
        message: &[u8],
        cx: &mut Context<'_>,
    ) -> Result<bool, ReadError> {
        if !Self::handles(opcode) {
            return Ok(false);
        }
        if self.login.is_none() {
            let login = cx.conn.iterations();
            self.login = Some(login);
            self.cell_iteration = login.1;
        }
        for out in self.on_message(opcode, message)? {
            let pushed = match out {
                DatOut::Interrogation(i) => cx.push(NetQueue::ClientCache, &i),
                DatOut::Begin(b) => cx.push(NetQueue::ClientCache, &b),
                DatOut::Data(d) => cx.push(NetQueue::ClientCache, d.as_ref()),
                DatOut::End => {
                    cx.push_raw(Opcode::DDD_ON_END_DDD, NetQueue::ClientCache, Vec::new());
                    Ok(())
                }
            };
            if let Err(e) = pushed {
                tracing::warn!("1999: a later data patch message did not encode: {e}");
            }
        }
        Ok(true)
    }

    fn up(
        &mut self,
        opcode: Opcode,
        _queue: NetQueue,
        _ordered: bool,
        payload: &[u8],
        cx: &mut Context<'_>,
    ) -> bool {
        match opcode {
            Opcode::DDD_REQUEST_DATA_MESSAGE => {
                let Ok(req) = pyreal_protocol::read_body::<DddRequestData>(
                    payload.get(4..).unwrap_or_default(),
                ) else {
                    return false;
                };
                if let Some(m) = self.request(req) {
                    return cx.send(DAT_QUEUE, &m);
                }
                let error = DddError {
                    resource_type: req.resource_type,
                    resource_id: req.resource_id,
                    error: NOT_AVAILABLE,
                };
                if let Err(e) = cx.push(NetQueue::ClientCache, &error) {
                    tracing::warn!("1999: {:#06X} did not encode: {e}", DddError::OPCODE.0);
                }
                true
            }
            // The interrogation answer and the end of the patch have no 1999 counterpart.
            Opcode::DDD_INTERROGATION_RESPONSE_MESSAGE | Opcode::DDD_ON_END_DDD => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stored-block zlib stream of `data`, which needs no compressor.
    fn zlib_stored(data: &[u8]) -> Vec<u8> {
        let len = u16::try_from(data.len()).unwrap();
        let mut out = vec![0x78, 0x01, 0x01];
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(data);
        out.extend_from_slice(&pyreal_dat::inflate::adler32(data).to_be_bytes());
        out
    }

    fn record(id: u32, cache: u32, data: &[u8]) -> Vec<u8> {
        let z = zlib_stored(data);
        let mut w = Writer::new(RECORD);
        w.u32(id)
            .u32(cache)
            .u32(u32::try_from(z.len()).unwrap())
            .u32(u32::try_from(data.len()).unwrap())
            .bytes(&z)
            .align();
        w.into_inner()
    }

    fn begin(portal: (u32, u32), cell: (u32, u32)) -> Vec<u8> {
        Begin {
            portal_iteration: portal.0,
            portal_records: portal.1,
            cell_iteration: cell.0,
            cell_records: cell.1,
            portal_bytes: 0,
            cell_bytes: 0,
        }
        .write()
    }

    #[test]
    fn a_record_reads_its_stream_and_pads_to_four() {
        let m = record(0x0600_1234, 6, b"hello");
        assert_eq!(m.len() % 4, 0);
        let r = Record::read(&m).unwrap();
        assert_eq!(
            (r.id, r.cache, r.bytes.as_slice()),
            (0x0600_1234, 6, &b"hello"[..])
        );
        assert!(!r.in_cell());
        assert!(r.fits());

        let mut bad = m.clone();
        bad[16] = 0; // inflated length 0
        bad[17] = 0;
        assert_eq!(Record::read(&bad), Err(ReadError::Bad("inflated length")));
        let mut bad = m;
        bad[24] ^= 0xFF; // a stream byte
        assert!(Record::read(&bad).is_err());
    }

    #[test]
    fn cells_are_asked_for_under_the_cache_their_low_word_names() {
        assert_eq!(cell_cache(0xA9B4_FFFF), 0);
        assert_eq!(cell_cache(0xA9B4_FFFE), 1);
        assert_eq!(cell_cache(0xA9B4_0105), 2);
        assert_eq!(
            cell_request(0xA9B4_FFFE),
            [0xA9, 0xF7, 0, 0, 0xFE, 0xFF, 0xB4, 0xA9, 1, 0, 0, 0]
        );
        let mut p = DatPush::new((102, 67));
        assert!(p
            .request(DddRequestData {
                resource_type: 0x0D,
                resource_id: 0x0600_0001
            })
            .is_none());
    }

    #[test]
    fn a_full_push_opens_the_patch_streams_its_records_and_ends_at_the_last() {
        let mut p = DatPush::new((102, 67));
        let b = begin((105, 2), (70, 1));
        assert_eq!(
            p.on_message(BEGIN, &b).unwrap(),
            [DatOut::Interrogation(DddInterrogation::default())]
        );
        assert!(p.on_message(BEGIN, &b).unwrap().is_empty(), "records owed");
        let data = |out: Vec<DatOut>| match out.as_slice() {
            [DatOut::Data(d)] => d.as_ref().clone(),
            _ => panic!("{out:?}"),
        };
        let a = data(
            p.on_message(RECORD, &record(0x0600_0002, 6, b"two"))
                .unwrap(),
        );
        assert_eq!((a.dat_file_type, a.dat_file_id, a.iteration), (0, 1, 105));
        assert_eq!(a.data, b"two");
        let c = data(
            p.on_message(RECORD, &record(0xA9B4_FFFF, 0, b"land"))
                .unwrap(),
        );
        assert_eq!((c.dat_file_type, c.dat_file_id, c.iteration), (1, 2, 70));
        assert_eq!((c.compressed, c.version, c.data_size), (0, 1, 8));
        let out = p
            .on_message(RECORD, &record(0x0100_0001, 6, b"one"))
            .unwrap();
        assert!(p.done());
        let [DatOut::Data(last), DatOut::Begin(begin), DatOut::End] = out.as_slice() else {
            panic!("{out:?}");
        };
        assert_eq!(last.resource_id, 0x0100_0001);
        assert_eq!(begin.revisions.len(), 2);
        assert_eq!(
            (
                begin.revisions[0].dat_file_type,
                begin.revisions[0].dat_file_id
            ),
            (0, 1)
        );
        assert_eq!(begin.revisions[0].iteration, 105);
        assert_eq!(
            begin.revisions[0].ids_to_download,
            [0x0100_0001, 0x0600_0002]
        );
        assert_eq!(begin.revisions[1].iteration, 70);
        assert_eq!(begin.revisions[1].ids_to_download, [0xA9B4_FFFF]);
        assert_eq!(begin.data_expected, 3 + 3 + 4);

        // The same push again after a deletion or a log-off is passed over.
        assert!(p.on_message(BEGIN, &b_again()).unwrap().is_empty());
        assert!(p
            .on_message(RECORD, &record(0x0600_0002, 6, b"two"))
            .unwrap()
            .is_empty());
    }

    fn b_again() -> Vec<u8> {
        begin((105, 2), (70, 1))
    }

    #[test]
    fn the_later_data_message_is_one_pyreal_reads_back() {
        let r = Record {
            id: 0xA9B4_0105,
            cache: 2,
            bytes: b"cell".to_vec(),
        };
        let later = r.to_later(70, 3);
        let blob = pyreal_protocol::write_blob(&later).unwrap();
        let back: DddData = pyreal_protocol::read_body(&blob[4..]).unwrap();
        assert_eq!(back, later);
        assert_eq!(back.data_size as usize, back.data.len() + 4);
    }

    #[test]
    fn an_empty_push_at_the_login_iterations_is_only_the_end() {
        let mut p = DatPush::new((102, 67));
        let b = begin((102, 0), (67, 0));
        assert!(p.on_message(BEGIN, &b).unwrap().is_empty());
        assert_eq!(p.on_message(BEGIN, &b).unwrap(), [DatOut::End]);
        assert!(p.on_message(BEGIN, &b).unwrap().is_empty());
    }

    #[test]
    fn a_newer_iteration_with_nothing_to_send_is_still_recorded() {
        let mut p = DatPush::new((102, 67));
        let b = begin((103, 0), (67, 0));
        p.on_message(BEGIN, &b).unwrap();
        let out = p.on_message(BEGIN, &b).unwrap();
        let [DatOut::Begin(begin), DatOut::End] = out.as_slice() else {
            panic!("{out:?}");
        };
        assert_eq!(begin.revisions.len(), 1);
        assert_eq!(begin.revisions[0].iteration, 103);
        assert!(begin.revisions[0].ids_to_download.is_empty());
    }

    #[test]
    fn records_that_fit_no_1999_file_still_count_but_are_dropped() {
        let mut p = DatPush::new((102, 67));
        let b = begin((105, 1), (67, 2));
        assert_eq!(
            p.on_message(BEGIN, &b).unwrap().len(),
            1,
            "the interrogation"
        );
        // A cell id under the portal's cache, and a portal id under a cell cache.
        p.on_message(RECORD, &record(0xA9B4_FFFF, 6, b"x")).unwrap();
        p.on_message(RECORD, &record(0x0600_0001, 2, b"y")).unwrap();
        // A touch list counts as one.
        let mut touch = Writer::new(TOUCH_LIST);
        touch.u32(1).u32(0x0600_0001);
        let out = p.on_message(TOUCH_LIST, &touch.into_inner()).unwrap();
        assert_eq!(p.dropped(), 2);
        let [DatOut::Begin(begin), DatOut::End] = out.as_slice() else {
            panic!("{out:?}");
        };
        assert_eq!(
            begin.revisions.len(),
            1,
            "the portal's newer iteration only"
        );
        assert!(begin.revisions[0].ids_to_download.is_empty());
    }

    #[test]
    fn a_cell_answer_carries_the_type_it_was_asked_under() {
        let mut p = DatPush::new((102, 67));
        let ask = p
            .request(DddRequestData {
                resource_type: 3,
                resource_id: 0xA9B4_0105,
            })
            .unwrap();
        assert_eq!(&ask[..4], &CELL_REQUEST.to_le_bytes());
        let z = zlib_stored(b"env cell");
        let mut w = Writer::new(CELL_RECORD);
        w.u32(0xA9B4_0105)
            .u32(u32::try_from(z.len()).unwrap())
            .u32(8)
            .bytes(&z)
            .align();
        let out = p.on_message(CELL_RECORD, &w.into_inner()).unwrap();
        let [DatOut::Data(d)] = out.as_slice() else {
            panic!("{out:?}");
        };
        assert_eq!(
            (d.resource_type, d.resource_id, d.iteration),
            (3, 0xA9B4_0105, 67)
        );
        assert_eq!(d.data, b"env cell");
    }

    #[test]
    fn a_cell_pair_reads_both_records_without_padding_between() {
        let z1 = zlib_stored(b"land!");
        let z2 = zlib_stored(b"info");
        let mut w = Writer::new(CELL_PAIR);
        w.u32(0xA9B4_FFFF)
            .u32(u32::try_from(z1.len()).unwrap())
            .u32(5)
            .bytes(&z1)
            .u32(0xA9B4_FFFE)
            .u32(u32::try_from(z2.len()).unwrap())
            .u32(4)
            .bytes(&z2)
            .align();
        let pair = Record::read_pair(&w.into_inner()).unwrap();
        assert_eq!(pair.len(), 2);
        assert_eq!(
            (pair[0].cache, pair[0].bytes.as_slice()),
            (0, &b"land!"[..])
        );
        assert_eq!((pair[1].cache, pair[1].bytes.as_slice()), (1, &b"info"[..]));

        let mut w = Writer::new(CELL_PAIR);
        w.u32(0).u32(0);
        assert!(Record::read_pair(&w.into_inner()).unwrap().is_empty());
    }

    fn parts() -> (
        std::collections::VecDeque<pyreal_primitives::IncomingMessage>,
        crate::conn::Connection,
        crate::translate::Shared,
    ) {
        let cred = crate::login::Credential::new("Demo", "secret").unwrap();
        let shared = crate::translate::Shared {
            account: "demo".to_owned(),
            world_name: "AC1999".to_owned(),
            ..Default::default()
        };
        (
            Default::default(),
            crate::conn::Connection::new(cred, (102, 67)),
            shared,
        )
    }

    #[test]
    fn through_the_link_the_patch_goes_up_on_the_cache_queue_in_the_later_order() {
        let (mut inbox, mut conn, mut shared) = parts();
        let mut cx = Context {
            inbox: &mut inbox,
            conn: &mut conn,
            shared: &mut shared,
            wrapped: None,
        };
        let mut p = DatPush::default();
        let b = begin((103, 1), (67, 0));
        assert_eq!(p.down(BEGIN, 7, &b, &mut cx), Ok(true));
        assert_eq!(
            p.down(RECORD, 7, &record(0x0600_0002, 6, b"two"), &mut cx),
            Ok(true)
        );
        assert_eq!(p.down(0xF658, 4, &[0x58, 0xF6, 0, 0], &mut cx), Ok(false));
        let got: Vec<_> = inbox.iter().map(|m| (m.opcode, m.queue)).collect();
        assert_eq!(
            got,
            [
                (0xF7E5, NetQueue::ClientCache),
                (0xF7E2, NetQueue::ClientCache),
                (0xF7E7, NetQueue::ClientCache),
                (0xF7EA, NetQueue::ClientCache)
            ]
        );
        let begin: DddBeginDdd = pyreal_protocol::read_body(&inbox[2].body).unwrap();
        assert_eq!(
            begin.revisions.len(),
            1,
            "the cell file is at the login's iteration"
        );
    }

    #[test]
    fn a_request_for_no_cell_is_answered_here_and_the_patch_end_is_swallowed() {
        let (mut inbox, mut conn, mut shared) = parts();
        let mut cx = Context {
            inbox: &mut inbox,
            conn: &mut conn,
            shared: &mut shared,
            wrapped: None,
        };
        let mut p = DatPush::default();
        let req = pyreal_protocol::write_blob(&DddRequestData {
            resource_type: 0x0D,
            resource_id: 0x0600_0001,
        })
        .unwrap();
        assert!(p.up(
            Opcode::DDD_REQUEST_DATA_MESSAGE,
            NetQueue::ClientCache,
            false,
            &req,
            &mut cx
        ));
        assert!(p.up(
            Opcode::DDD_ON_END_DDD,
            NetQueue::ClientCache,
            false,
            &[0xEA, 0xF7, 0, 0],
            &mut cx
        ));
        assert_eq!(inbox.len(), 1);
        let e: DddError = pyreal_protocol::read_body(&inbox[0].body).unwrap();
        assert_eq!((e.resource_type, e.resource_id), (0x0D, 0x0600_0001));
    }
}
