//! The login request, the credential it carries, and the server's grant or refusal.

use crate::packet::{flags, unkeyed, Datagram};

/// The client version the server compares.
pub const CLIENT_VERSION: &str = "4.35752";
/// The longest `account:password` the login carries.
pub const CREDENTIAL_LEN: usize = 20;
/// The 1999 retail CD's portal and cell iterations.
pub const CD_ITERATIONS: (u32, u32) = (102, 67);

const LOGIN_MAGIC: u32 = 0x075B_CD15;
const ACCOUNT_MARKER: u32 = 0xBEEF_BEEF;

/// Why a credential cannot be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialError {
    /// `account:password` is longer than [`CREDENTIAL_LEN`].
    TooLong,
    /// The account name or the password is empty.
    Empty,
    /// The account name holds a colon.
    ColonInAccount,
    /// A character outside printable ASCII.
    NotPrintable,
}

/// `account:password`, lowercased, at most 20 bytes. Never logged.
#[derive(Clone, PartialEq, Eq)]
pub struct Credential {
    field: [u8; CREDENTIAL_LEN],
    account: String,
}

impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credential")
            .field("account", &self.account)
            .finish_non_exhaustive()
    }
}

impl Credential {
    /// The pair the login carries. With no `password`, `account` may be `account:password` whole.
    ///
    /// # Errors
    /// [`CredentialError`] when the pair cannot be sent as it is; nothing is truncated.
    pub fn new(account: &str, password: &str) -> Result<Self, CredentialError> {
        let (account, password) = if password.is_empty() {
            account.split_once(':').unwrap_or((account, ""))
        } else {
            (account, password)
        };
        if account.is_empty() || password.is_empty() {
            return Err(CredentialError::Empty);
        }
        if account.contains(':') {
            return Err(CredentialError::ColonInAccount);
        }
        let printable = |s: &str| s.bytes().all(|b| (0x20..0x7F).contains(&b));
        if !printable(account) || !printable(password) {
            return Err(CredentialError::NotPrintable);
        }
        let pair = format!("{account}:{password}").to_ascii_lowercase();
        if pair.len() > CREDENTIAL_LEN {
            return Err(CredentialError::TooLong);
        }
        let mut field = [0u8; CREDENTIAL_LEN];
        field[..pair.len()].copy_from_slice(pair.as_bytes());
        Ok(Self {
            field,
            account: account.to_ascii_lowercase(),
        })
    }

    /// The account name, lowercased.
    #[must_use]
    pub fn account(&self) -> &str {
        &self.account
    }
}

/// The login datagram: sequence 1, flags `0x2`, recipient 0, the generic checksum and a 96-byte
/// body. Iterations of 0 are sent as the CD's, since the server logs a login that reports 0.
#[must_use]
pub fn login_datagram(credential: &Credential, portal: u32, cell: u32) -> Vec<u8> {
    let portal = if portal == 0 { CD_ITERATIONS.0 } else { portal };
    let cell = if cell == 0 { CD_ITERATIONS.1 } else { cell };
    let mut body = Vec::with_capacity(96);
    body.extend_from_slice(&LOGIN_MAGIC.to_le_bytes());
    let version_len = u16::try_from(CLIENT_VERSION.len() + 1).unwrap_or(0);
    body.extend_from_slice(&version_len.to_le_bytes());
    body.extend_from_slice(CLIENT_VERSION.as_bytes());
    body.push(0);
    body.resize(body.len().next_multiple_of(4), 0);
    body.extend_from_slice(&ACCOUNT_MARKER.to_le_bytes());
    body.extend_from_slice(&credential.field);
    body.extend_from_slice(&0u32.to_le_bytes());
    body.extend_from_slice(&ACCOUNT_MARKER.to_le_bytes());
    body.extend_from_slice(&[0u8; 24]);
    for word in [1, 0, portal, cell, 0, u32::MAX] {
        body.extend_from_slice(&word.to_le_bytes());
    }
    unkeyed(1, flags::CHECKSUM, 0, 0, 0, &body)
}

/// The server's grant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grant {
    /// The server's connection id, from the header: every packet it sends carries it.
    pub server_id: u32,
    /// The table byte every later packet repeats.
    pub table: u8,
    /// The world clock's epoch.
    pub epoch: f64,
    /// The recipient id the client puts in every packet it sends.
    pub client_id: u32,
    /// Keys what the server sends.
    pub server_seed: u32,
    /// Keys what the client sends.
    pub client_seed: u32,
}

/// Why a grant was not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantError {
    /// Not a grant, or a malformed one.
    Malformed,
    /// The server's version is not this client's.
    VersionMismatch,
}

impl Grant {
    /// Read a grant datagram whose checksum has been checked.
    ///
    /// # Errors
    /// [`GrantError`] when it is not a grant this client can take.
    pub fn read(d: &Datagram<'_>) -> Result<Self, GrantError> {
        let h = d.header;
        if !h.has(flags::GRANT) || h.has(flags::GRANT_ERROR) || !(1..400).contains(&h.recipient) {
            return Err(GrantError::Malformed);
        }
        let b = d.body;
        let word = |at: usize| -> Result<u32, GrantError> {
            b.get(at..at + 4)
                .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
                .ok_or(GrantError::Malformed)
        };
        let double = |at: usize| -> Result<f64, GrantError> {
            let v = b
                .get(at..at + 8)
                .and_then(|w| <[u8; 8]>::try_from(w).ok())
                .map(f64::from_le_bytes)
                .ok_or(GrantError::Malformed)?;
            if v.is_finite() {
                Ok(v)
            } else {
                Err(GrantError::Malformed)
            }
        };
        let _server_time = double(0)?;
        let epoch = double(8)?;
        let client_id = word(16)?;
        let version_len = usize::try_from(word(20)?).map_err(|_| GrantError::Malformed)?;
        let version = b.get(24..24 + version_len).ok_or(GrantError::Malformed)?;
        let seeds_at = 24 + version_len;
        let server_seed = word(seeds_at)?;
        let client_seed = word(seeds_at + 4)?;
        if !same_version(version) {
            return Err(GrantError::VersionMismatch);
        }
        Ok(Self {
            server_id: h.recipient,
            table: h.table,
            epoch,
            client_id,
            server_seed,
            client_seed,
        })
    }
}

/// Whether the decimal number `version` starts with is this client's, read as C's `strtod` reads
/// it: `4.357520` and `4.35752x` match, `4.35753` does not.
#[must_use]
pub fn same_version(version: &[u8]) -> bool {
    let text: String = version
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| char::from(b))
        .collect();
    let text = text.trim_start();
    let mut end = 0;
    let mut seen_digit = false;
    let mut seen_dot = false;
    for (i, c) in text.char_indices() {
        match c {
            '+' | '-' if i == 0 => {}
            '0'..='9' => seen_digit = true,
            '.' if !seen_dot => seen_dot = true,
            _ => break,
        }
        end = i + 1;
    }
    seen_digit && text[..end].parse::<f64>().ok() == CLIENT_VERSION.parse::<f64>().ok()
}

/// A refusal before the grant: `0x100000` without `0x200`, an 8-byte body of string and table ids.
#[must_use]
pub fn read_refusal(d: &Datagram<'_>) -> Option<(u32, u32)> {
    if !d.header.has(flags::NET_ERROR) || d.header.has(flags::GRANT) || d.body.len() < 8 {
        return None;
    }
    let w = |at: usize| {
        u32::from_le_bytes([d.body[at], d.body[at + 1], d.body[at + 2], d.body[at + 3]])
    };
    Some((w(0), w(4)))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::packet::{generic_checksum, Header};

    #[test]
    fn the_credential_is_lowercased_and_refused_rather_than_cut() {
        let c = Credential::new("Ab", "CdEf").expect("fits");
        assert_eq!(&c.field[..8], b"ab:cdef\0");
        assert_eq!(c.account(), "ab");
        assert_eq!(
            Credential::new("Ab:CdEf", ""),
            Ok(c),
            "-a account:password whole"
        );
        assert!(
            Credential::new("abcdefghij", "123456789").is_ok(),
            "20 exactly"
        );
        assert_eq!(
            Credential::new("abcdefghij", "1234567890"),
            Err(CredentialError::TooLong)
        );
        assert_eq!(Credential::new("", "x"), Err(CredentialError::Empty));
        assert_eq!(Credential::new("a", ""), Err(CredentialError::Empty));
        assert_eq!(
            Credential::new("a:b", "c"),
            Err(CredentialError::ColonInAccount)
        );
        assert_eq!(
            Credential::new("caf\u{e9}", "x"),
            Err(CredentialError::NotPrintable)
        );
        assert!(!format!("{:?}", Credential::new("demo", "secret").unwrap()).contains("secret"));
    }

    #[test]
    fn the_login_datagram_has_the_spec_layout() {
        let c = Credential::new("demo", "secret").expect("fits");
        let d = login_datagram(&c, 102, 67);
        let parsed = Datagram::parse(&d).expect("parses");
        assert_eq!(parsed.header.seq, 1);
        assert_eq!(parsed.header.flags, 0x2);
        assert_eq!(parsed.header.recipient, 0);
        assert_eq!(parsed.header.len, 96);
        assert!(parsed.generic_ok());
        let b = parsed.body;
        let word = |at: usize| u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
        assert_eq!(word(0), 0x075B_CD15);
        assert_eq!(&b[4..16], b"\x08\x004.35752\0\0\0");
        assert_eq!(word(16), 0xBEEF_BEEF);
        assert_eq!(&b[20..31], b"demo:secret");
        assert!(b[31..44].iter().all(|&x| x == 0));
        assert_eq!(word(44), 0xBEEF_BEEF);
        assert!(b[48..72].iter().all(|&x| x == 0));
        assert_eq!(
            [word(72), word(76), word(80), word(84), word(88), word(92)],
            [1, 0, 102, 67, 0, u32::MAX]
        );
        let zero = login_datagram(&c, 0, 0);
        let zb = &zero[20..];
        assert_eq!(&zb[80..88], &[102, 0, 0, 0, 67, 0, 0, 0], "never zeros");
    }

    /// The spec's grant vector: header recipient 11, table 3.
    pub(crate) fn grant_datagram(version: &[u8], server_seed: u32, client_seed: u32) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&1.0f64.to_le_bytes());
        body.extend_from_slice(&2.0f64.to_le_bytes());
        body.extend_from_slice(&0x1234_5678u32.to_le_bytes());
        body.extend_from_slice(&u32::try_from(version.len()).unwrap().to_le_bytes());
        body.extend_from_slice(version);
        body.extend_from_slice(&server_seed.to_le_bytes());
        body.extend_from_slice(&client_seed.to_le_bytes());
        unkeyed(0, flags::GRANT | flags::CHECKSUM, 11, 0, 3, &body)
    }

    #[test]
    fn the_grant_vector_reads_and_the_version_is_compared_as_a_number() {
        let d = grant_datagram(b"4.35752", 0x5566_7788, 0x1122_3344);
        let parsed = Datagram::parse(&d).expect("parses");
        assert!(parsed.generic_ok());
        let g = Grant::read(&parsed).expect("a grant");
        assert_eq!(g.server_id, 11);
        assert_eq!(g.table, 3);
        assert_eq!(g.client_id, 0x1234_5678);
        assert_eq!((g.server_seed, g.client_seed), (0x5566_7788, 0x1122_3344));
        assert!((g.epoch - 2.0).abs() < f64::EPSILON);

        assert!(same_version(b"4.357520"));
        assert!(same_version(b"4.35752x"));
        assert!(same_version(b"4.35752\0\0"));
        assert!(!same_version(b"4.35753"));
        assert!(!same_version(b"x4.35752"));
        let d = grant_datagram(b"4.35753", 1, 2);
        assert_eq!(
            Grant::read(&Datagram::parse(&d).unwrap()),
            Err(GrantError::VersionMismatch)
        );
        // An odd-length version leaves the seeds unaligned.
        let d = grant_datagram(b"4.357520", 7, 8);
        let g = Grant::read(&Datagram::parse(&d).unwrap()).expect("unaligned seeds");
        assert_eq!((g.server_seed, g.client_seed), (7, 8));
    }

    #[test]
    fn a_refusal_carries_two_ids() {
        let mut body = 5u32.to_le_bytes().to_vec();
        body.extend(8u32.to_le_bytes());
        let d = unkeyed(0, flags::NET_ERROR | flags::CHECKSUM, 0, 0, 0, &body);
        let parsed = Datagram::parse(&d).unwrap();
        assert_eq!(read_refusal(&parsed), Some((5, 8)));
        let h = Header {
            len: 8,
            flags: 0x10_0002,
            ..Header::default()
        };
        assert_eq!(parsed.header.checksum, generic_checksum(h, &body));
    }
}
