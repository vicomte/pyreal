//! Which wire a world speaks, and whether it is up, asked without an account.
//!
//! Servers of both wires answer one login for the server-tracker account, `acservertracker`, and
//! nothing else about it; none creates an account for it. A server of the later wire answers the
//! later form of that login. A server of the 1999 wire does not answer it in a form the later wire
//! reads, and answers the 1999 form with a login failure (`0x100000`). So [`Probe`] sends the
//! later form first and the 1999 form after it, and classifies by what comes back
//! ([`Answer`]). Silence from both means the world is down or unreachable.
//!
//! Socket-free like the rest of the crate: the caller sends what [`Probe::poll`] returns, feeds
//! back what the world's address sends ([`Probe::receive`]), and supplies the time.

use pyreal_transport::conn::{build_login_request, ConnectionAuthenticator};
use pyreal_transport::{OutPacket, PacketFlags, ParsedPacket, ProtoHeader};

use crate::login::{login_datagram, Credential, CD_ITERATIONS};
use crate::packet::{flags, Datagram};

/// The account every server answers without a password.
pub const TRACKER_ACCOUNT: &str = "acservertracker";
/// The password trackers send with it; the server reads only the account.
const TRACKER_PASSWORD: &str = "jj9h26hcsggc";
/// How many times each form is sent.
pub const TRIES: u32 = 2;
/// Seconds between sends, and the wait after the last one.
pub const WAIT: f64 = 0.6;

/// What the world turned out to speak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// It answered the later form: the later wire.
    Later,
    /// It answered only the 1999 form: the 1999 wire.
    Wire1999,
    /// Neither form was answered: down, or not a server of either wire.
    Silent,
}

/// The later wire's tracker login: one unsequenced plaintext datagram.
#[must_use]
pub fn later_request() -> Vec<u8> {
    let auth =
        ConnectionAuthenticator::account_only(&format!("{TRACKER_ACCOUNT}:{TRACKER_PASSWORD}"));
    let mut packet = OutPacket::new(ProtoHeader::default());
    if packet
        .add_optional_header(PacketFlags::LOGIN_REQUEST, build_login_request(&auth))
        .is_err()
    {
        return Vec::new();
    }
    packet.serialize(None).unwrap_or_default()
}

/// The 1999 wire's tracker login: the account cut to the 20-character field, where that wire's
/// server reads it, with the CD's iterations.
#[must_use]
pub fn request_1999() -> Vec<u8> {
    let cut = CD_ITERATIONS;
    // `acservertracker:` leaves four of the 20 characters for the password.
    let password = &TRACKER_PASSWORD[..crate::CREDENTIAL_LEN - TRACKER_ACCOUNT.len() - 1];
    Credential::new(TRACKER_ACCOUNT, password)
        .map(|c| login_datagram(&c, cut.0, cut.1))
        .unwrap_or_default()
}

/// A packet of the later wire with an unkeyed checksum that holds.
#[must_use]
pub fn is_later_reply(datagram: &[u8]) -> bool {
    ParsedPacket::parse(datagram).is_ok_and(|p| p.checksum_ok(None))
}

/// A packet of the 1999 wire that answers a login: a refusal, a grant or a grant error, with the
/// generic checksum holding.
#[must_use]
pub fn is_1999_reply(datagram: &[u8]) -> bool {
    Datagram::parse(datagram).is_some_and(|d| {
        d.header.flags & (flags::NET_ERROR | flags::GRANT | flags::GRANT_ERROR) != 0
            && d.generic_ok()
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Later,
    Form1999,
}

/// One world's probe.
#[derive(Debug, Clone)]
pub struct Probe {
    step: Step,
    sent: u32,
    last: Option<f64>,
    answer: Option<Answer>,
}

impl Default for Probe {
    fn default() -> Self {
        Self::new()
    }
}

impl Probe {
    #[must_use]
    pub fn new() -> Self {
        Self {
            step: Step::Later,
            sent: 0,
            last: None,
            answer: None,
        }
    }

    /// The datagram to send at `now`, if one is due. Each form goes [`TRIES`] times, [`WAIT`]
    /// apart; after the 1999 form's last wait the answer is [`Answer::Silent`].
    pub fn poll(&mut self, now: f64) -> Option<Vec<u8>> {
        if self.answer.is_some() || self.last.is_some_and(|t| now - t < WAIT) {
            return None;
        }
        if self.sent == TRIES {
            match self.step {
                Step::Later => self.move_to_1999(),
                Step::Form1999 => {
                    self.answer = Some(Answer::Silent);
                    return None;
                }
            }
        }
        self.sent += 1;
        self.last = Some(now);
        Some(match self.step {
            Step::Later => later_request(),
            Step::Form1999 => request_1999(),
        })
    }

    /// A datagram from the world's address. A later reply settles it at any step; a 1999 reply
    /// while the later form is out moves straight on to the 1999 form, which alone settles it.
    pub fn receive(&mut self, datagram: &[u8]) {
        if self.answer.is_some() {
            return;
        }
        if is_later_reply(datagram) {
            self.answer = Some(Answer::Later);
        } else if is_1999_reply(datagram) {
            match self.step {
                Step::Later => self.move_to_1999(),
                Step::Form1999 => self.answer = Some(Answer::Wire1999),
            }
        }
    }

    fn move_to_1999(&mut self) {
        self.step = Step::Form1999;
        self.sent = 0;
        self.last = None;
    }

    /// The answer, once settled.
    #[must_use]
    pub fn answer(&self) -> Option<Answer> {
        self.answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::{unkeyed, HEADER_SIZE};
    use pyreal_transport::conn::LoginRequest;

    fn word(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
    }

    /// The 1999 login failure the local AC1999 server sent the tracker login: flags `0x100000`,
    /// no checksum flag, the generic checksum, the log-on-failed string id and eight bytes.
    fn refusal_1999() -> Vec<u8> {
        let mut body = 0x04DF_9C54_u32.to_le_bytes().to_vec();
        body.extend_from_slice(&[0; 8]);
        unkeyed(0, flags::NET_ERROR, 0, 0, 0, &body)
    }

    #[test]
    fn the_1999_form_is_a_96_byte_login_for_the_tracker_account() {
        let bytes = request_1999();
        assert_eq!(bytes.len(), HEADER_SIZE + 96);
        let d = Datagram::parse(&bytes).expect("a whole 1999 packet");
        assert_eq!(
            (d.header.seq, d.header.flags, d.header.recipient),
            (1, 0x2, 0)
        );
        assert_eq!((d.header.len, d.header.time, d.header.table), (96, 0, 0));
        assert!(d.generic_ok(), "seeded 0x03291975, header plus body");
        let body = d.body;
        assert_eq!(word(body, 0), 0x075B_CD15);
        assert_eq!(&body[4..14], b"\x08\x004.35752\0");
        assert_eq!(word(body, 16), 0xBEEF_BEEF);
        assert_eq!(&body[20..40], b"acservertracker:jj9h", "cut to the field");
        assert_eq!(word(body, 44), 0xBEEF_BEEF);
        assert!(body[48..72].iter().all(|b| *b == 0));
        let tail: Vec<u32> = (72..96).step_by(4).map(|at| word(body, at)).collect();
        assert_eq!(
            tail,
            [1, 0, 102, 67, 0, u32::MAX],
            "the CD's iterations, never 0"
        );
    }

    #[test]
    fn the_later_form_is_a_plain_login_for_the_tracker_account() {
        let bytes = later_request();
        let p = ParsedPacket::parse(&bytes).expect("a later packet");
        assert_eq!(p.header.header.0, PacketFlags::LOGIN_REQUEST);
        assert!(p.checksum_ok(None));
        let login = LoginRequest::parse(&bytes[HEADER_SIZE..]).expect("a login request");
        assert_eq!(login.auth.account, b"acservertracker:jj9h26hcsggc");
        assert!(login.auth.extra_data.is_empty(), "no password field");
        assert!(is_later_reply(&bytes));
        assert!(!is_1999_reply(&bytes));
    }

    #[test]
    fn each_wire_reads_only_its_own_replies() {
        let refusal = refusal_1999();
        assert!(is_1999_reply(&refusal));
        assert!(!is_later_reply(&refusal), "its checksum is the 1999 one");
        let mut bad = refusal.clone();
        bad[HEADER_SIZE] ^= 1;
        assert!(!is_1999_reply(&bad), "a checksum that does not hold");
        assert!(!is_1999_reply(&refusal[..HEADER_SIZE + 4]), "a cut packet");
        let keepalive = unkeyed(0, flags::CHECKSUM | flags::FLOW, 0, 0, 0, &[0; 4]);
        assert!(!is_1999_reply(&keepalive), "no login answer");
        assert!(!is_1999_reply(&request_1999()), "the request itself");
    }

    /// Drives `probe` from 0 s in 0.1 s steps, answering each send with what `world` returns.
    fn run(world: impl Fn(&[u8]) -> Option<Vec<u8>>) -> (Answer, Vec<Vec<u8>>, f64) {
        let mut probe = Probe::new();
        let mut sent = Vec::new();
        for tick in 0..100 {
            let now = f64::from(tick) * 0.1;
            if let Some(d) = probe.poll(now) {
                if let Some(reply) = world(&d) {
                    probe.receive(&reply);
                }
                sent.push(d);
            }
            if let Some(a) = probe.answer() {
                return (a, sent, now);
            }
        }
        panic!("the probe never settled");
    }

    #[test]
    fn a_world_that_answers_the_later_form_is_a_later_world() {
        let (answer, sent, _) = run(|d| is_later_reply(d).then(later_request));
        assert_eq!(answer, Answer::Later);
        assert_eq!(sent.len(), 1, "no 1999 login is sent to it");
    }

    #[test]
    fn a_world_that_ignores_the_later_form_and_refuses_the_1999_one_is_a_1999_world() {
        let (answer, sent, at) = run(|d| is_1999_reply_to(d).then(refusal_1999));
        assert_eq!(answer, Answer::Wire1999);
        assert_eq!(
            sent.len(),
            TRIES as usize + 1,
            "both later tries, then one 1999"
        );
        assert!(at >= f64::from(TRIES) * WAIT - 1e-9);
    }

    /// A 1999 server answers the later form with a grant it cannot use; the probe moves to the
    /// 1999 form at once.
    #[test]
    fn a_1999_answer_to_the_later_form_moves_straight_to_the_1999_form() {
        let grant = unkeyed(0, flags::GRANT | flags::CHECKSUM, 11, 0, 3, &[0; 24]);
        let (answer, sent, at) = run(|d| {
            if is_later_reply(d) {
                Some(grant.clone())
            } else {
                Some(refusal_1999())
            }
        });
        assert_eq!(answer, Answer::Wire1999);
        assert_eq!(sent.len(), 2);
        assert!(at < WAIT, "no wait for the later form's second try");
    }

    #[test]
    fn a_silent_world_is_silent_after_both_forms() {
        let (answer, sent, at) = run(|_| None);
        assert_eq!(answer, Answer::Silent);
        assert_eq!(sent.len(), 2 * TRIES as usize);
        assert!(sent[..2].iter().all(|d| is_later_reply(d)));
        assert!(sent[2..]
            .iter()
            .all(|d| Datagram::parse(d).is_some_and(|d| d.generic_ok())));
        assert!(at >= 2.0 * f64::from(TRIES) * WAIT - 1e-9);
    }

    #[test]
    fn a_later_reply_that_comes_late_still_counts() {
        let mut probe = Probe::new();
        for now in [0.0, 0.6, 1.2] {
            probe.poll(now);
        }
        probe.receive(&later_request());
        assert_eq!(probe.answer(), Some(Answer::Later));
        probe.receive(&refusal_1999());
        assert_eq!(probe.answer(), Some(Answer::Later), "settled once");
    }

    /// Whether `d` is the 1999 form of the login (what a 1999 server answers).
    fn is_1999_reply_to(d: &[u8]) -> bool {
        Datagram::parse(d).is_some_and(|d| d.header.seq == 1 && d.body.len() == 96)
    }
}
