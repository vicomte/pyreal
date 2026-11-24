//! The 20-byte header, the flags, the checksum and the fragments of a 1999 datagram.

use pyreal_transport::crc::hash32;
use pyreal_transport::wire::FragmentHeader;

/// Header bytes.
pub const HEADER_SIZE: usize = 20;
/// The largest datagram either side sends, header included.
pub const MAX_DATAGRAM: usize = 484;
/// Fragment header bytes.
pub const FRAGMENT_HEADER_SIZE: usize = 16;
/// Payload bytes in one client fragment: 20 header, 4 flow word, 16 fragment header, 444.
pub const CLIENT_FRAGMENT_PAYLOAD: usize = 444;
/// Written over the checksum field while the header is hashed.
pub const CHECKSUM_SEED: u32 = 0x0329_1975;

/// The header flags. One bit means the same in both directions.
pub mod flags {
    /// A resent data packet.
    pub const RESEND: u32 = 0x1;
    /// On every packet either side sends or accepts.
    pub const CHECKSUM: u32 = 0x2;
    /// A list of sequences the sender wants again.
    pub const RESEND_REQUEST: u32 = 0x8;
    /// From the server an 8-byte time; from the client a request bit.
    pub const CLOCK_SYNC: u32 = 0x10;
    /// The header's sequence is the sender's latest.
    pub const FLOW: u32 = 0x20;
    /// The client logs off.
    pub const LOG_OFF: u32 = 0x40;
    /// A list of sequences the sender will never resend.
    pub const SKIP: u32 = 0x80;
    /// Server switch, 8-byte server time.
    pub const SERVER_SWITCH: u32 = 0x100;
    /// The grant, and after it a keepalive.
    pub const GRANT: u32 = 0x200;
    /// Application fragments follow.
    pub const DATA: u32 = 0x800;
    /// Bytes received in the finished period.
    pub const FLOW_REPORT: u32 = 0x1000;
    /// AC1999's own "will not resend" list.
    pub const DENY: u32 = 0x2000;
    /// The client's reset; AC1999's server ends the session on it.
    pub const RESET: u32 = 0x8000;
    /// A grant carrying an error instead of parameters.
    pub const GRANT_ERROR: u32 = 0x1_0000;
    /// The server ends the session.
    pub const DISCONNECT: u32 = 0x2_0000;
    /// Network error.
    pub const NET_ERROR: u32 = 0x10_0000;
    /// AC1999's server also sends a later-wire acknowledgement every two seconds, which the 1999
    /// client passes over.
    pub const LATER_ACK: u32 = 0x4000;
    /// And a later-wire time update, likewise.
    pub const LATER_TIME: u32 = 0x100_0000;
}

/// The 20-byte header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Header {
    pub seq: u32,
    pub flags: u32,
    /// The peer's connection id.
    pub recipient: u32,
    pub checksum: u32,
    /// Body bytes after the header, not counting the client's flow word.
    pub len: u16,
    /// The server's period byte.
    pub time: u8,
    /// Set by the grant, then repeated on every packet.
    pub table: u8,
}

impl Header {
    #[must_use]
    pub fn read(b: &[u8]) -> Option<Self> {
        let b: &[u8; HEADER_SIZE] = b.get(..HEADER_SIZE)?.try_into().ok()?;
        let word = |at: usize| u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
        Some(Self {
            seq: word(0),
            flags: word(4),
            recipient: word(8),
            checksum: word(12),
            len: u16::from_le_bytes([b[16], b[17]]),
            time: b[18],
            table: b[19],
        })
    }

    #[must_use]
    pub fn to_bytes(self) -> [u8; HEADER_SIZE] {
        let mut b = [0u8; HEADER_SIZE];
        b[0..4].copy_from_slice(&self.seq.to_le_bytes());
        b[4..8].copy_from_slice(&self.flags.to_le_bytes());
        b[8..12].copy_from_slice(&self.recipient.to_le_bytes());
        b[12..16].copy_from_slice(&self.checksum.to_le_bytes());
        b[16..18].copy_from_slice(&self.len.to_le_bytes());
        b[18] = self.time;
        b[19] = self.table;
        b
    }

    /// The header's share of every checksum: the header hashed with the seed in the checksum field.
    #[must_use]
    pub fn hash(self) -> u32 {
        hash32(
            &Header {
                checksum: CHECKSUM_SEED,
                ..self
            }
            .to_bytes(),
        )
    }

    #[must_use]
    pub fn has(self, flag: u32) -> bool {
        self.flags & flag != 0
    }
}

/// A body's share of a checksum: its hash with the length term taken back out.
#[must_use]
pub fn body_hash(body: &[u8]) -> u32 {
    let len = u32::try_from(body.len()).unwrap_or(u32::MAX);
    hash32(body).wrapping_sub(len.wrapping_shl(16))
}

/// A fragment's hash: its 16-byte header and its payload, each with its own length term.
#[must_use]
pub fn fragment_hash(header: &[u8], payload: &[u8]) -> u32 {
    hash32(header).wrapping_add(hash32(payload))
}

/// The checksum of an unkeyed packet: header plus body.
#[must_use]
pub fn generic_checksum(header: Header, body: &[u8]) -> u32 {
    header.hash().wrapping_add(body_hash(body))
}

/// A whole datagram, split into its header and its declared body.
#[derive(Debug, Clone, Copy)]
pub struct Datagram<'a> {
    pub header: Header,
    pub body: &'a [u8],
}

impl<'a> Datagram<'a> {
    /// `None` for a datagram longer than [`MAX_DATAGRAM`], shorter than a header, or whose declared
    /// body runs past its end. Bytes after the declared body are ignored.
    #[must_use]
    pub fn parse(raw: &'a [u8]) -> Option<Self> {
        if raw.len() > MAX_DATAGRAM {
            return None;
        }
        let header = Header::read(raw)?;
        let body = raw.get(HEADER_SIZE..HEADER_SIZE + usize::from(header.len))?;
        Some(Self { header, body })
    }

    /// The generic checksum holds.
    #[must_use]
    pub fn generic_ok(&self) -> bool {
        generic_checksum(self.header, self.body) == self.header.checksum
    }

    /// The header-only checksum holds.
    #[must_use]
    pub fn header_only_ok(&self) -> bool {
        self.header.hash() == self.header.checksum
    }
}

/// One fragment read from a packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fragment {
    pub header: FragmentHeader,
    pub payload: Vec<u8>,
}

impl Fragment {
    #[must_use]
    pub fn hash(&self) -> u32 {
        fragment_hash(&self.header.to_bytes(), &self.payload)
    }

    /// The fragment as it goes on the wire.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = self.header.to_bytes().to_vec();
        out.extend_from_slice(&self.payload);
        out
    }
}

/// The fragments of a data body: count at least 1, index below count, a payload that is not empty
/// and fits. `None` when any of them is malformed, or bytes are left over.
#[must_use]
pub fn read_fragments(mut body: &[u8]) -> Option<Vec<Fragment>> {
    let mut out = Vec::new();
    while !body.is_empty() {
        let header = FragmentHeader::from_bytes(body).ok()?;
        let size = usize::from(header.blob_frag_size);
        if header.num_frags == 0
            || header.blob_num >= header.num_frags
            || size <= FRAGMENT_HEADER_SIZE
            || size > body.len()
        {
            return None;
        }
        out.push(Fragment {
            header,
            payload: body[FRAGMENT_HEADER_SIZE..size].to_vec(),
        });
        body = &body[size..];
    }
    (!out.is_empty()).then_some(out)
}

/// A list body (resend request, skip, deny): the count, then as many sequences as it says and the
/// body holds, zeros skipped. `None` past 114 entries.
#[must_use]
pub fn read_list(body: &[u8]) -> Option<Vec<u32>> {
    let words: Vec<u32> = body
        .as_chunks::<4>()
        .0
        .iter()
        .map(|w| u32::from_le_bytes(*w))
        .collect();
    let (&count, rest) = words.split_first()?;
    if count > MAX_LIST_READ {
        return None;
    }
    Some(
        rest.iter()
            .take(usize::try_from(count).unwrap_or(0))
            .copied()
            .filter(|&s| s != 0)
            .collect(),
    )
}

/// The server's resend request: `[count * 4 + 4][count][sequences]`, the leading length being
/// what the original client's framing reads off before its list reader. A body without it is read
/// as a plain list.
#[must_use]
pub fn read_resend_request(body: &[u8]) -> Option<Vec<u32>> {
    let word = |at: usize| {
        body.get(at..at + 4)
            .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
    };
    let (first, second) = (word(0)?, word(4));
    let framed = second
        .is_some_and(|count| count.checked_mul(4).and_then(|n| n.checked_add(4)) == Some(first));
    let list = if framed { &body[4..] } else { body };
    if word(usize::from(framed) * 4)? == 0 {
        return None;
    }
    read_list(list)
}

/// The longest list either side accepts.
pub const MAX_LIST_READ: u32 = 0x72;
/// The longest list the client writes.
pub const MAX_LIST_WRITE: usize = 0x71;

/// A list body, at most [`MAX_LIST_WRITE`] sequences.
#[must_use]
pub fn write_list(seqs: &[u32]) -> Vec<u8> {
    let seqs = &seqs[..seqs.len().min(MAX_LIST_WRITE)];
    let mut out = Vec::with_capacity(4 + seqs.len() * 4);
    out.extend_from_slice(&u32::try_from(seqs.len()).unwrap_or(0).to_le_bytes());
    for s in seqs {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// A bodiless packet with the generic checksum.
#[must_use]
pub fn bare(seq: u32, flags: u32, recipient: u32, time: u8, table: u8) -> Vec<u8> {
    unkeyed(seq, flags, recipient, time, table, &[])
}

/// A packet with a body and the generic checksum, and no flow word.
#[must_use]
pub fn unkeyed(seq: u32, flags: u32, recipient: u32, time: u8, table: u8, body: &[u8]) -> Vec<u8> {
    let mut header = Header {
        seq,
        flags,
        recipient,
        checksum: 0,
        len: u16::try_from(body.len()).unwrap_or(u16::MAX),
        time,
        table,
    };
    header.checksum = generic_checksum(header, body);
    let mut out = header.to_bytes().to_vec();
    out.extend_from_slice(body);
    out
}

/// A client control packet: the flow word between header and body, both hashed as bodies. The
/// length counts the flow word, as the original client's does on these (a data packet's does not).
#[must_use]
pub fn control(header: Header, flow: u32, body: &[u8]) -> Vec<u8> {
    let mut header = Header {
        len: u16::try_from(body.len() + 4).unwrap_or(u16::MAX),
        ..header
    };
    let flow = flow.to_le_bytes();
    header.checksum = header
        .hash()
        .wrapping_add(body_hash(&flow))
        .wrapping_add(body_hash(body));
    let mut out = header.to_bytes().to_vec();
    out.extend_from_slice(&flow);
    out.extend_from_slice(body);
    out
}

/// A client data packet: the flow word, then one fragment whose keyed hash is `keyed`.
#[must_use]
pub fn client_data(header: Header, flow: u32, keyed: u32, fragment: &[u8]) -> Vec<u8> {
    let mut header = Header {
        len: u16::try_from(fragment.len()).unwrap_or(u16::MAX),
        ..header
    };
    let flow = flow.to_le_bytes();
    header.checksum = header
        .hash()
        .wrapping_add(body_hash(&flow))
        .wrapping_add(keyed);
    let mut out = header.to_bytes().to_vec();
    out.extend_from_slice(&flow);
    out.extend_from_slice(fragment);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generic_checksum_matches_the_spec_vector() {
        let header = Header {
            flags: 0x2,
            len: 3,
            ..Header::default()
        };
        assert_eq!(header.hash(), 0x033D_197A);
        assert_eq!(body_hash(&[0x11, 0x22, 0x33]), 0x1122_3300);
        let datagram = unkeyed(0, 0x2, 0, 0, 0, &[0x11, 0x22, 0x33]);
        assert_eq!(&datagram[12..16], &[0x7a, 0x4c, 0x5f, 0x14]);
        assert!(Datagram::parse(&datagram).expect("parses").generic_ok());
    }

    #[test]
    fn the_keyed_data_checksum_matches_the_spec_vector() {
        let header = Header {
            seq: 2,
            flags: 0x802,
            recipient: 0x0004_0000,
            len: 19,
            ..Header::default()
        };
        assert_eq!(header.hash(), 0x0341_218C);
        let fragment = Fragment {
            header: FragmentHeader {
                blob_id_low: 1,
                blob_id_high: 0,
                num_frags: 1,
                blob_frag_size: 19,
                blob_num: 0,
                queue_id: 3,
            },
            payload: vec![0x11, 0x22, 0x33],
        };
        assert_eq!(hash32(&fragment.header.to_bytes()), 0x0026_0002);
        assert_eq!(hash32(&fragment.payload), 0x1125_3300);
        assert_eq!(fragment.hash(), 0x114B_3302);
        let key = pyreal_transport::isaac::CryptoSystem::new(0).next();
        assert_eq!(key, 0x1826_00F3);
        assert_eq!(fragment.hash() ^ key, 0x096D_33F1);
        assert_eq!(
            header.hash().wrapping_add(fragment.hash() ^ key),
            0x0CAE_557D
        );
    }

    #[test]
    fn the_heartbeat_checksum_adds_the_flow_word_and_body() {
        let header = Header {
            flags: 0x22,
            recipient: 5,
            ..Header::default()
        };
        let datagram = control(header, 0x40, &[9, 0, 0, 0]);
        let read = Header::read(&datagram).expect("header");
        assert_eq!(read.len, 8, "the length counts the flow word");
        assert_eq!(
            read.checksum,
            Header { len: 8, ..header }
                .hash()
                .wrapping_add(0x40)
                .wrapping_add(9)
        );
        assert_eq!(&datagram[20..24], &[0x40, 0, 0, 0]);
    }

    #[test]
    fn a_datagram_whose_body_runs_past_its_end_or_that_is_too_long_is_refused() {
        let mut d = unkeyed(0, 0x2, 0, 0, 0, &[1, 2, 3, 4]);
        assert!(Datagram::parse(&d).is_some());
        d.extend_from_slice(&[0, 0, 0, 0]);
        let parsed = Datagram::parse(&d).expect("extra bytes are ignored");
        assert_eq!(parsed.body, &[1, 2, 3, 4]);
        assert!(Datagram::parse(&d[..22]).is_none());
        assert!(Datagram::parse(&[0u8; MAX_DATAGRAM + 1]).is_none());
    }

    #[test]
    fn fragments_must_be_whole_and_lists_are_bounded() {
        let f = Fragment {
            header: FragmentHeader {
                blob_id_low: 7,
                num_frags: 2,
                blob_frag_size: 20,
                blob_num: 1,
                queue_id: 4,
                ..FragmentHeader::default()
            },
            payload: vec![1, 2, 3, 4],
        };
        let mut body = f.to_bytes();
        body.extend(f.to_bytes());
        assert_eq!(read_fragments(&body).expect("two").len(), 2);
        assert!(read_fragments(&body[..30]).is_none(), "a cut fragment");
        let mut bad = f.clone();
        bad.header.blob_num = 2;
        assert!(
            read_fragments(&bad.to_bytes()).is_none(),
            "index past count"
        );
        assert!(read_fragments(&[]).is_none());

        assert_eq!(read_list(&write_list(&[3, 0, 5])), Some(vec![3, 5]));
        // The server's form leads with the list's length; a count of 0 asks for nothing.
        let framed = [12u32, 2, 7, 9]
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect::<Vec<_>>();
        assert_eq!(read_resend_request(&framed), Some(vec![7, 9]));
        assert_eq!(read_resend_request(&write_list(&[7, 9])), Some(vec![7, 9]));
        assert_eq!(read_resend_request(&write_list(&[])), None);
        let mut long = 0x73u32.to_le_bytes().to_vec();
        long.extend([0u8; 8]);
        assert!(read_list(&long).is_none());
        assert_eq!(
            read_list(&write_list(&[9; 200])).map(|l| l.len()),
            Some(0x71)
        );
    }
}
