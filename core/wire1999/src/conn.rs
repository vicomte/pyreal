//! One 1999 connection, with no socket: the login and its grant, the key streams, packet-order
//! delivery, reassembly, resends, the flow word and reports, heartbeats, clock sync and log-off.
//!
//! The caller feeds it datagrams and the time, and sends what [`Connection::take_outgoing`]
//! returns to the server's address.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use pyreal_transport::isaac::CryptoSystem;
use pyreal_transport::wire::FragmentHeader;

use crate::login::{login_datagram, read_refusal, Credential, Grant, GrantError};
use crate::packet::{
    bare, client_data, control, flags, read_fragments, read_list, read_resend_request, unkeyed,
    write_list, Datagram, Fragment, Header, CLIENT_FRAGMENT_PAYLOAD, FRAGMENT_HEADER_SIZE,
    HEADER_SIZE,
};

/// Seconds between login sends.
pub const LOGIN_RESEND: f64 = 2.0;
/// Login sends before giving up.
pub const LOGIN_ATTEMPTS: u32 = 20;
/// Seconds between resend requests while anything is missing.
pub const RESEND_REQUEST_INTERVAL: f64 = 0.25;
/// Seconds between heartbeats while nothing is missing.
pub const HEARTBEAT_INTERVAL: f64 = 2.0;
/// Seconds after a clock sync before asking again.
pub const CLOCK_SYNC_REASK: f64 = 60.0;
/// Seconds of data after which a bare network error is ignored.
pub const NET_ERROR_GRACE: f64 = 120.0;
/// Seconds of silence, or of a message gaining no fragment, before the connection closes.
pub const SILENCE_LIMIT: f64 = 30.0;
/// Sequences accepted ahead of the next one delivered.
pub const REORDER_WINDOW: u32 = 256;
/// Sent packets kept for resends.
pub const RESEND_CACHE: usize = 256;
/// Messages being reassembled at once.
pub const OPEN_MESSAGES: usize = 16;
/// Parts in one received message: a dat record inflates to under 4 MB.
pub const MAX_PARTS_IN: u16 = 9600;
/// Parts in one sent message.
pub const MAX_PARTS_OUT: usize = 64;

/// The queues the server sends on: world objects, UI and the dat cache.
pub const SERVER_QUEUES: [u16; 3] = [3, 4, 7];

/// Why the connection ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Close {
    /// The server refused the login: its string and table ids.
    Refused { string_id: u32, table_id: u32 },
    /// The server's version is not this client's.
    VersionMismatch,
    /// No grant after every login send.
    LoginTimedOut,
    /// Silence, or a message that stopped arriving.
    TimedOut,
    /// The server ended the session.
    ServerDisconnected,
    /// A network error from the server.
    NetError,
    /// A second grant unlike the first.
    BadGrant,
    /// This client logged off.
    LoggedOff,
}

/// Where the connection is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Login sends going out.
    Authenticating,
    /// Granted and acknowledged; nothing on the lists or data yet.
    Granted,
    /// A server packet reached the list or data handling.
    Connected,
    Closed(Close),
}

/// A datagram changed nothing: malformed, a bad checksum, the wrong peer, or a duplicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rejected;

/// The inbound key stream. Sequence `n >= 2` takes the `(n - 1)`th draw.
#[derive(Debug, Clone)]
struct InKeys {
    isaac: CryptoSystem,
    /// The highest sequence whose key is committed.
    committed: u32,
    /// Keys drawn past `committed`, not yet committed: a cache, so a rejected packet commits
    /// nothing.
    ahead: VecDeque<u32>,
    /// Keys of committed sequences not yet received.
    held: BTreeMap<u32, u32>,
}

impl InKeys {
    fn new(seed: u32) -> Self {
        Self {
            isaac: CryptoSystem::new(seed),
            committed: 1,
            ahead: VecDeque::new(),
            held: BTreeMap::new(),
        }
    }

    /// The key `seq` would use, or `None` for a duplicate.
    fn peek(&mut self, seq: u32) -> Option<u32> {
        if seq <= self.committed {
            return self.held.get(&seq).copied();
        }
        let n = usize::try_from(seq - self.committed).ok()?;
        while self.ahead.len() < n {
            self.ahead.push_back(self.isaac.next());
        }
        self.ahead.get(n - 1).copied()
    }

    /// Draw through `seq`, holding every key but `seq`'s own when `consume`.
    fn commit(&mut self, seq: u32, consume: bool) {
        if seq <= self.committed {
            if consume {
                self.held.remove(&seq);
            }
            return;
        }
        let _ = self.peek(seq);
        while self.committed < seq {
            self.committed += 1;
            let key = self.ahead.pop_front().unwrap_or(0);
            if !(consume && self.committed == seq) {
                self.held.insert(self.committed, key);
            }
        }
    }

    fn forget(&mut self, seq: u32) {
        self.held.remove(&seq);
    }
}

/// A sent data packet, kept for resends.
#[derive(Debug, Clone)]
struct Sent {
    seq: u32,
    keyed: u32,
    fragment: Vec<u8>,
    last_sent: f64,
}

/// A message being reassembled.
#[derive(Debug)]
struct Partial {
    queue: u16,
    parts: Vec<Option<Vec<u8>>>,
    have: usize,
    touched: f64,
}

/// What an accepted datagram does, decided before anything changes.
enum Effect {
    Keepalive,
    BadGrant,
    ClockSync,
    Disconnect,
    NetError,
    Ignore,
    ServerSwitch,
    GiveUp(Vec<u32>),
    Resend(Vec<u32>, Option<u32>),
    Announce(u32),
    Sequenced(u32, Vec<Fragment>),
}

/// One 1999 connection.
#[derive(Debug)]
pub struct Connection {
    credential: Credential,
    iterations: (u32, u32),
    state: State,
    login_sends: u32,
    last_login: Option<f64>,
    grant: Option<Grant>,
    in_keys: Option<InKeys>,
    out_keys: Option<CryptoSystem>,
    /// The next sequence to deliver.
    next_deliver: u32,
    /// Accepted and waiting for the sequences before them.
    parked: BTreeMap<u32, Vec<Fragment>>,
    given_up: BTreeSet<u32>,
    highest_received: u32,
    partial: BTreeMap<u64, Partial>,
    messages: VecDeque<(u16, Vec<u8>)>,
    next_seq: u32,
    blob_counter: u64,
    sent: VecDeque<Sent>,
    /// The server's period byte, as adopted.
    time: u8,
    flow_count: u32,
    clock_synced: Option<f64>,
    now: f64,
    last_heard: f64,
    last_data: Option<f64>,
    last_resend_request: Option<f64>,
    last_heartbeat: Option<f64>,
    outgoing: Vec<Vec<u8>>,
}

impl Connection {
    /// A connection that logs in with `credential`, reporting the newest portal and cell
    /// iterations the client holds.
    #[must_use]
    pub fn new(credential: Credential, iterations: (u32, u32)) -> Self {
        Self {
            credential,
            iterations,
            state: State::Authenticating,
            login_sends: 0,
            last_login: None,
            grant: None,
            in_keys: None,
            out_keys: None,
            next_deliver: 2,
            parked: BTreeMap::new(),
            given_up: BTreeSet::new(),
            highest_received: 1,
            partial: BTreeMap::new(),
            messages: VecDeque::new(),
            next_seq: 2,
            blob_counter: 0,
            sent: VecDeque::new(),
            time: 1,
            flow_count: 0,
            clock_synced: None,
            now: 0.0,
            last_heard: 0.0,
            last_data: None,
            last_resend_request: None,
            last_heartbeat: None,
            outgoing: Vec::new(),
        }
    }

    /// The portal and cell iterations the login reports.
    #[must_use]
    pub fn iterations(&self) -> (u32, u32) {
        self.iterations
    }

    #[must_use]
    pub fn state(&self) -> State {
        self.state
    }

    #[must_use]
    pub fn grant(&self) -> Option<&Grant> {
        self.grant.as_ref()
    }

    #[must_use]
    pub fn credential(&self) -> &Credential {
        &self.credential
    }

    /// Datagrams to send to the server, oldest first.
    pub fn take_outgoing(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.outgoing)
    }

    /// The next whole message, in packet order, with the queue it came on.
    pub fn next_message(&mut self) -> Option<(u16, Vec<u8>)> {
        self.messages.pop_front()
    }

    fn granted(&self) -> bool {
        matches!(self.state, State::Granted | State::Connected)
    }

    fn close(&mut self, why: Close) {
        self.state = State::Closed(why);
        self.parked.clear();
        self.partial.clear();
        self.sent.clear();
        self.given_up.clear();
    }

    /// The login sends, then the resend requests, heartbeats and the silence limit.
    pub fn tick(&mut self, now: f64) {
        self.now = now;
        match self.state {
            State::Closed(_) => {}
            State::Authenticating => {
                if self.last_login.is_some_and(|t| now - t < LOGIN_RESEND) {
                    return;
                }
                if self.login_sends >= LOGIN_ATTEMPTS {
                    self.close(Close::LoginTimedOut);
                    return;
                }
                self.login_sends += 1;
                self.last_login = Some(now);
                self.outgoing.push(login_datagram(
                    &self.credential,
                    self.iterations.0,
                    self.iterations.1,
                ));
            }
            State::Granted | State::Connected => {
                if now - self.last_heard > SILENCE_LIMIT
                    || self
                        .partial
                        .values()
                        .any(|p| now - p.touched > SILENCE_LIMIT)
                {
                    self.close(Close::TimedOut);
                    return;
                }
                let missing = self.missing();
                if !missing.is_empty() {
                    if self
                        .last_resend_request
                        .is_none_or(|t| now - t >= RESEND_REQUEST_INTERVAL)
                    {
                        self.last_resend_request = Some(now);
                        let f = flags::RESEND_REQUEST | flags::CHECKSUM | self.sync_bit();
                        self.send_control(f, self.next_seq - 1, &write_list(&missing));
                    }
                } else if self
                    .last_heartbeat
                    .is_none_or(|t| now - t >= HEARTBEAT_INTERVAL)
                {
                    self.last_heartbeat = Some(now);
                    let f = flags::FLOW | flags::CHECKSUM | self.sync_bit();
                    self.send_control(f, self.next_seq - 1, &self.highest_received.to_le_bytes());
                }
            }
        }
    }

    /// `0x10` while the clock is unsynced, and again a minute after each sync.
    fn sync_bit(&self) -> u32 {
        if self
            .clock_synced
            .is_none_or(|t| self.now - t >= CLOCK_SYNC_REASK)
        {
            flags::CLOCK_SYNC
        } else {
            0
        }
    }

    /// Sequences known and neither received nor given up, at most the list's worth.
    fn missing(&self) -> Vec<u32> {
        let Some(keys) = &self.in_keys else {
            return Vec::new();
        };
        (self.next_deliver..=keys.committed)
            .filter(|s| !self.parked.contains_key(s) && !self.given_up.contains(s))
            .take(crate::packet::MAX_LIST_WRITE)
            .collect()
    }

    fn header(&self, seq: u32, flags: u32, time: u8) -> Header {
        Header {
            seq,
            flags,
            recipient: self.grant.map_or(0, |g| g.client_id),
            checksum: 0,
            len: 0,
            time,
            table: self.grant.map_or(0, |g| g.table),
        }
    }

    fn send_control(&mut self, flags: u32, seq: u32, body: &[u8]) {
        let h = self.header(seq, flags, self.time);
        self.outgoing.push(control(h, self.flow_count, body));
    }

    fn send_bare(&mut self, flags: u32) {
        let h = self.header(0, flags, 0);
        self.outgoing.push(bare(0, flags, h.recipient, 0, h.table));
    }

    /// Send one message on `queue` as data packets, one fragment each. `false` before the grant,
    /// after close, or for a message too long to send.
    pub fn send(&mut self, queue: u16, message: &[u8]) -> bool {
        if !self.granted() || message.is_empty() {
            return false;
        }
        let parts: Vec<&[u8]> = message.chunks(CLIENT_FRAGMENT_PAYLOAD).collect();
        let Ok(count) = u16::try_from(parts.len()) else {
            return false;
        };
        if parts.len() > MAX_PARTS_OUT {
            return false;
        }
        self.blob_counter += 1;
        let blob = 0x8000_0000_0000_0000 | self.blob_counter;
        for (index, payload) in parts.into_iter().enumerate() {
            let fragment = Fragment {
                header: FragmentHeader {
                    blob_id_low: (blob & 0xFFFF_FFFF) as u32,
                    blob_id_high: (blob >> 32) as u32,
                    num_frags: count,
                    blob_frag_size: u16::try_from(FRAGMENT_HEADER_SIZE + payload.len())
                        .unwrap_or(u16::MAX),
                    blob_num: u16::try_from(index).unwrap_or(0),
                    queue_id: queue,
                },
                payload: payload.to_vec(),
            };
            let key = self.out_keys.as_mut().map_or(0, CryptoSystem::next);
            let keyed = fragment.hash() ^ key;
            let seq = self.next_seq;
            self.next_seq += 1;
            let bytes = fragment.to_bytes();
            let h = self.header(seq, flags::DATA | flags::CHECKSUM, self.time);
            self.outgoing
                .push(client_data(h, self.flow_count, keyed, &bytes));
            self.sent.push_back(Sent {
                seq,
                keyed,
                fragment: bytes,
                last_sent: self.now,
            });
            if self.sent.len() > RESEND_CACHE {
                self.sent.pop_front();
            }
        }
        true
    }

    /// Log off: a bare `0x42`, then a bare `0x8002`, and close.
    pub fn log_off(&mut self) {
        if self.granted() {
            self.send_bare(flags::LOG_OFF | flags::CHECKSUM);
            self.send_bare(flags::RESET | flags::CHECKSUM);
        }
        self.close(Close::LoggedOff);
    }

    /// One received datagram.
    ///
    /// # Errors
    /// [`Rejected`] when it changed nothing.
    pub fn feed(&mut self, raw: &[u8], now: f64) -> Result<(), Rejected> {
        self.now = now;
        let d = Datagram::parse(raw).ok_or(Rejected)?;
        // AC1999's login refusal is the one packet that comes without the checksum bit.
        let refusal = self.state == State::Authenticating && d.header.has(flags::NET_ERROR);
        // The server's later-wire acknowledgement and time update: alone, or before the data
        // starts, they hold no sequence and are passed over. A time update with the checksum bit
        // takes a sequence of its own, and is checked and counted below.
        let server = self.grant.map(|g| g.server_id);
        let later = d.header.flags & (flags::LATER_ACK | flags::LATER_TIME) != 0;
        if server == Some(d.header.recipient)
            && later
            && (!d.header.has(flags::CHECKSUM) || d.header.seq < 2)
        {
            return Ok(());
        }
        if !d.header.has(flags::CHECKSUM) && !refusal {
            return Err(Rejected);
        }
        match self.state {
            State::Closed(_) => Err(Rejected),
            State::Authenticating => self.feed_login(&d),
            State::Granted | State::Connected => {
                let effect = self.check(&d)?;
                self.apply(d.header, effect);
                Ok(())
            }
        }
    }

    fn feed_login(&mut self, d: &Datagram<'_>) -> Result<(), Rejected> {
        if !d.generic_ok() {
            return Err(Rejected);
        }
        if d.header.has(flags::GRANT) {
            return match Grant::read(d) {
                Ok(g) => {
                    self.grant = Some(g);
                    self.in_keys = Some(InKeys::new(g.server_seed));
                    self.out_keys = Some(CryptoSystem::new(g.client_seed));
                    self.state = State::Granted;
                    self.last_heard = self.now;
                    self.last_heartbeat = Some(self.now);
                    self.send_bare(flags::GRANT | flags::CHECKSUM);
                    Ok(())
                }
                Err(GrantError::VersionMismatch) => {
                    self.close(Close::VersionMismatch);
                    Ok(())
                }
                Err(GrantError::Malformed) => Err(Rejected),
            };
        }
        if let Some((string_id, table_id)) = read_refusal(d) {
            self.close(Close::Refused {
                string_id,
                table_id,
            });
            return Ok(());
        }
        Err(Rejected)
    }

    /// Every check a datagram must pass, with nothing changed.
    fn check(&mut self, d: &Datagram<'_>) -> Result<Effect, Rejected> {
        let h = d.header;
        let grant = self.grant.ok_or(Rejected)?;
        if h.recipient != grant.server_id {
            return Err(Rejected);
        }
        let table_ok = h.table == grant.table
            || (h.has(flags::CLOCK_SYNC) && h.table < grant.table)
            || (h.has(flags::DENY) && h.table > grant.table);
        if !table_ok {
            return Err(Rejected);
        }
        let generic = |ok: bool| if ok { Ok(()) } else { Err(Rejected) };

        if h.has(flags::GRANT) {
            generic(d.generic_ok())?;
            if d.body.is_empty() {
                return Ok(Effect::Keepalive);
            }
            return Ok(match Grant::read(d) {
                Ok(again) if again == grant => Effect::Keepalive,
                _ => Effect::BadGrant,
            });
        }
        if h.has(flags::CLOCK_SYNC) {
            generic(d.generic_ok() && d.body.len() >= 8)?;
            return Ok(Effect::ClockSync);
        }
        if h.has(flags::DISCONNECT) {
            generic(d.header_only_ok())?;
            return Ok(Effect::Disconnect);
        }
        if h.has(flags::NET_ERROR) {
            if d.body.is_empty() {
                generic(d.header_only_ok())?;
                let recent = self
                    .last_data
                    .is_some_and(|t| self.now - t <= NET_ERROR_GRACE);
                return Ok(if recent {
                    Effect::Ignore
                } else {
                    Effect::NetError
                });
            }
            generic(d.generic_ok() && d.body.len() >= 8)?;
            return Ok(Effect::NetError);
        }
        if h.has(flags::SERVER_SWITCH) {
            generic(d.generic_ok())?;
            return Ok(Effect::ServerSwitch);
        }
        if h.has(flags::DENY) {
            generic(d.generic_ok())?;
            return Ok(Effect::GiveUp(read_list(d.body).ok_or(Rejected)?));
        }
        if h.has(flags::SKIP) || h.has(flags::RESEND_REQUEST) || h.has(flags::FLOW) {
            generic(d.generic_ok())?;
            if h.has(flags::SKIP) {
                return Ok(Effect::GiveUp(read_list(d.body).ok_or(Rejected)?));
            }
            if h.has(flags::RESEND_REQUEST) {
                let list = read_resend_request(d.body).ok_or(Rejected)?;
                return Ok(Effect::Resend(list, self.announced(h.seq)));
            }
            return Ok(self
                .announced(h.seq)
                .map_or(Effect::Ignore, Effect::Announce));
        }
        if h.seq < 2 {
            return Err(Rejected);
        }
        let keys = self.in_keys.as_mut().ok_or(Rejected)?;
        if h.seq > self.next_deliver.saturating_add(REORDER_WINDOW) {
            return Err(Rejected);
        }
        let key = keys.peek(h.seq).ok_or(Rejected)?;
        if h.has(flags::LATER_TIME) {
            // Its body (the acknowledged sequence, then the time) is hashed whole, length term
            // included, and keyed like data.
            let sum = pyreal_transport::crc::hash32(d.body) ^ key;
            generic(h.hash().wrapping_add(sum) == h.checksum)?;
            return Ok(Effect::Sequenced(h.seq, Vec::new()));
        }
        if !h.has(flags::DATA) {
            generic(d.generic_ok())?;
            return Ok(Effect::Sequenced(h.seq, Vec::new()));
        }
        let fragments = read_fragments(d.body).ok_or(Rejected)?;
        if fragments.iter().any(|f| {
            !SERVER_QUEUES.contains(&f.header.queue_id) || f.header.num_frags > MAX_PARTS_IN
        }) {
            return Err(Rejected);
        }
        let sum = fragments
            .iter()
            .fold(0u32, |acc, f| acc.wrapping_add(f.hash()));
        generic(h.hash().wrapping_add(sum ^ key) == h.checksum)?;
        Ok(Effect::Sequenced(h.seq, fragments))
    }

    /// The server's latest sequence, when it is one to draw up to.
    fn announced(&self, seq: u32) -> Option<u32> {
        (seq >= 2 && seq <= self.next_deliver.saturating_add(REORDER_WINDOW)).then_some(seq)
    }

    fn apply(&mut self, h: Header, effect: Effect) {
        self.last_heard = self.now;
        match effect {
            Effect::Ignore => return,
            Effect::Keepalive => {
                self.send_bare(flags::GRANT | flags::CHECKSUM);
                return;
            }
            Effect::BadGrant => {
                self.close(Close::BadGrant);
                return;
            }
            Effect::ClockSync => {
                self.clock_synced = Some(self.now);
                return;
            }
            Effect::Disconnect => {
                self.send_bare(flags::DISCONNECT | flags::CHECKSUM);
                self.close(Close::ServerDisconnected);
                return;
            }
            Effect::NetError => {
                self.close(Close::NetError);
                return;
            }
            _ => {}
        }
        self.account_flow(h);
        match effect {
            Effect::ServerSwitch => {
                self.clock_synced = Some(self.now);
                self.send_bare(flags::SERVER_SWITCH | flags::CHECKSUM);
                return;
            }
            Effect::GiveUp(seqs) => {
                for s in seqs {
                    self.give_up(s);
                }
            }
            Effect::Resend(seqs, announce) => {
                if let Some(s) = announce {
                    self.announce(s);
                }
                self.resend(&seqs);
            }
            Effect::Announce(s) => self.announce(s),
            Effect::Sequenced(seq, fragments) => {
                if let Some(keys) = self.in_keys.as_mut() {
                    keys.commit(seq, true);
                }
                self.highest_received = self.highest_received.max(seq);
                if !fragments.is_empty() {
                    self.last_data = Some(self.now);
                }
                self.parked.insert(seq, fragments);
            }
            _ => {}
        }
        if self.state == State::Granted {
            self.state = State::Connected;
        }
        self.deliver();
    }

    /// Count the packet toward the period, reporting the finished period when the server's time
    /// byte moves on.
    fn account_flow(&mut self, h: Header) {
        let ahead = h.time.wrapping_sub(self.time);
        if ahead != 0 && ahead < 0x80 {
            if h.time != 1 {
                let header = self.header(0, flags::FLOW_REPORT | flags::CHECKSUM, h.time - 1);
                self.outgoing.push(unkeyed(
                    0,
                    header.flags,
                    header.recipient,
                    header.time,
                    header.table,
                    &self.flow_count.to_le_bytes(),
                ));
            }
            self.flow_count = 0;
            self.time = h.time;
        }
        self.flow_count = self
            .flow_count
            .wrapping_add(u32::try_from(HEADER_SIZE).unwrap_or(0) + u32::from(h.len));
    }

    fn announce(&mut self, seq: u32) {
        if let Some(keys) = self.in_keys.as_mut() {
            keys.commit(seq, false);
        }
    }

    fn give_up(&mut self, seq: u32) {
        if let Some(keys) = self.in_keys.as_mut() {
            keys.forget(seq);
        }
        if seq >= self.next_deliver && !self.parked.contains_key(&seq) {
            self.given_up.insert(seq);
        }
    }

    /// Resend what is still held; deny the rest, in both forms.
    fn resend(&mut self, seqs: &[u32]) {
        let mut denied = Vec::new();
        // Sequence 1 is the login's, never data: AC1999's server asks for it while it has had no
        // data, as its acknowledgement.
        for &seq in seqs.iter().filter(|&&s| s >= 2) {
            let Some(i) = self.sent.iter().position(|s| s.seq == seq) else {
                denied.push(seq);
                continue;
            };
            if self.now - self.sent[i].last_sent < RESEND_REQUEST_INTERVAL {
                continue;
            }
            self.sent[i].last_sent = self.now;
            let s = self.sent[i].clone();
            let h = self.header(
                s.seq,
                flags::DATA | flags::CHECKSUM | flags::RESEND,
                self.time,
            );
            self.outgoing
                .push(client_data(h, self.flow_count, s.keyed, &s.fragment));
        }
        if !denied.is_empty() {
            self.send_control(flags::SKIP | flags::CHECKSUM, 0, &write_list(&denied));
            let ac1999: Vec<u32> = denied.into_iter().filter(|&s| s >= 2).collect();
            if !ac1999.is_empty() {
                let h = self.header(0, flags::DENY | flags::CHECKSUM, self.time);
                self.outgoing.push(unkeyed(
                    0,
                    h.flags,
                    h.recipient,
                    h.time,
                    h.table,
                    &write_list(&ac1999),
                ));
            }
        }
    }

    /// Release packets in sequence order, passing over the ones given up.
    fn deliver(&mut self) {
        loop {
            let seq = self.next_deliver;
            if let Some(fragments) = self.parked.remove(&seq) {
                for f in fragments {
                    self.add_fragment(f);
                }
            } else if !self.given_up.remove(&seq) {
                break;
            }
            self.next_deliver += 1;
        }
        let next = self.next_deliver;
        self.given_up.retain(|&s| s >= next);
    }

    fn add_fragment(&mut self, f: Fragment) {
        let h = f.header;
        if h.num_frags == 1 {
            self.finish(h.queue_id, f.payload);
            return;
        }
        let id = h.blob_id();
        let now = self.now;
        let p = self.partial.entry(id).or_insert_with(|| Partial {
            queue: h.queue_id,
            parts: vec![None; usize::from(h.num_frags)],
            have: 0,
            touched: now,
        });
        let index = usize::from(h.blob_num);
        if p.queue != h.queue_id || p.parts.len() != usize::from(h.num_frags) {
            self.partial.remove(&id);
            return;
        }
        p.touched = now;
        if p.parts[index].is_none() {
            p.parts[index] = Some(f.payload);
            p.have += 1;
        }
        if p.have == p.parts.len() {
            if let Some(p) = self.partial.remove(&id) {
                let message = p.parts.into_iter().flatten().flatten().collect();
                self.finish(p.queue, message);
            }
        } else if self.partial.len() > OPEN_MESSAGES {
            let oldest = self
                .partial
                .iter()
                .min_by(|a, b| a.1.touched.total_cmp(&b.1.touched))
                .map(|(&k, _)| k);
            if let Some(k) = oldest {
                self.partial.remove(&k);
            }
        }
    }

    fn finish(&mut self, queue: u16, message: Vec<u8>) {
        if message.len() >= 4 {
            self.messages.push_back((queue, message));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::login::tests::grant_datagram;
    use crate::packet::{body_hash, Header};

    const SERVER: u32 = 11;
    const CLIENT: u32 = 0x1234_5678;
    const TABLE: u8 = 3;

    /// A connection past its grant, seeds 0 both ways, with the ack taken.
    fn granted() -> Connection {
        let mut c = Connection::new(Credential::new("demo", "secret").unwrap(), (102, 67));
        c.tick(0.0);
        assert_eq!(c.take_outgoing().len(), 1, "the login");
        c.feed(&grant_datagram(b"4.35752", 0, 0), 0.1)
            .expect("granted");
        let ack = c.take_outgoing();
        assert_eq!(ack.len(), 1);
        let h = Header::read(&ack[0]).unwrap();
        assert_eq!(
            (h.seq, h.flags, h.recipient, h.table, h.time),
            (0, 0x202, CLIENT, TABLE, 0)
        );
        assert_eq!(h.checksum, h.hash(), "generic, no body");
        assert_eq!(c.state(), State::Granted);
        c
    }

    /// A server data packet: one fragment per message, keyed with the server stream.
    struct Server {
        keys: CryptoSystem,
        seq: u32,
        blob: u32,
        time: u8,
    }

    impl Server {
        fn new() -> Self {
            Self {
                keys: CryptoSystem::new(0),
                seq: 1,
                blob: 0,
                time: 1,
            }
        }

        fn packet(&mut self, frags: &[Fragment]) -> Vec<u8> {
            self.seq += 1;
            let key = self.keys.next();
            let body: Vec<u8> = frags.iter().flat_map(Fragment::to_bytes).collect();
            let sum = frags.iter().fold(0u32, |a, f| a.wrapping_add(f.hash()));
            let mut h = Header {
                seq: self.seq,
                flags: 0x802,
                recipient: SERVER,
                len: u16::try_from(body.len()).unwrap(),
                time: self.time,
                table: TABLE,
                ..Header::default()
            };
            h.checksum = h.hash().wrapping_add(sum ^ key);
            let mut out = h.to_bytes().to_vec();
            out.extend(body);
            out
        }

        fn message(&mut self, queue: u16, message: &[u8], per: usize) -> Vec<Fragment> {
            self.blob += 1;
            let parts: Vec<&[u8]> = message.chunks(per).collect();
            parts
                .iter()
                .enumerate()
                .map(|(i, p)| Fragment {
                    header: FragmentHeader {
                        blob_id_low: self.blob,
                        blob_id_high: 0,
                        num_frags: u16::try_from(parts.len()).unwrap(),
                        blob_frag_size: u16::try_from(16 + p.len()).unwrap(),
                        blob_num: u16::try_from(i).unwrap(),
                        queue_id: queue,
                    },
                    payload: p.to_vec(),
                })
                .collect()
        }

        fn control(&self, seq: u32, flags: u32, body: &[u8]) -> Vec<u8> {
            unkeyed(seq, flags, SERVER, self.time, TABLE, body)
        }
    }

    #[test]
    fn the_login_resends_every_two_seconds_and_gives_up_after_twenty() {
        let mut c = Connection::new(Credential::new("demo", "secret").unwrap(), (102, 67));
        let mut sent = 0;
        for i in 0..=20 {
            c.tick(f64::from(i) * 2.0);
            c.tick(f64::from(i) * 2.0 + 1.0);
            sent += c.take_outgoing().len();
        }
        assert_eq!(sent, 20);
        assert_eq!(c.state(), State::Closed(Close::LoginTimedOut));
    }

    #[test]
    fn a_refusal_or_another_version_ends_the_login() {
        let mut c = Connection::new(Credential::new("demo", "secret").unwrap(), (102, 67));
        let mut body = 5u32.to_le_bytes().to_vec();
        body.extend(8u32.to_le_bytes());
        c.feed(&unkeyed(0, 0x10_0000, 11, 0, 0, &body), 0.0)
            .expect("AC1999's refusal has no checksum bit");
        assert_eq!(
            c.state(),
            State::Closed(Close::Refused {
                string_id: 5,
                table_id: 8
            })
        );
        let mut c = Connection::new(Credential::new("demo", "secret").unwrap(), (102, 67));
        c.feed(&grant_datagram(b"5.1", 0, 0), 0.0).unwrap();
        assert_eq!(c.state(), State::Closed(Close::VersionMismatch));
    }

    #[test]
    fn a_bad_checksum_or_the_wrong_peer_changes_nothing() {
        let mut c = granted();
        let mut s = Server::new();
        let f = s.message(4, &[0x5A, 0xF6, 0, 0, 0, 0, 0, 0], 100);
        let mut good = s.packet(&f);
        good[14] ^= 1;
        assert_eq!(c.feed(&good, 1.0), Err(Rejected));
        good[14] ^= 1;
        let mut other = good.clone();
        other[8] = 12;
        assert_eq!(c.feed(&other, 1.0), Err(Rejected), "another server id");
        assert_eq!(c.state(), State::Granted);
        c.feed(&good, 1.0)
            .expect("the same packet, intact, is taken");
        assert_eq!(c.state(), State::Connected);
        assert_eq!(c.next_message().map(|m| m.0), Some(4));
        assert_eq!(c.feed(&good, 1.1), Err(Rejected), "a duplicate");
        let mut ack = Header {
            seq: 2,
            flags: 0x4000,
            recipient: SERVER,
            len: 4,
            ..Header::default()
        }
        .to_bytes()
        .to_vec();
        ack.extend([1, 0, 0, 0]);
        assert_eq!(
            c.feed(&ack, 1.2),
            Ok(()),
            "the server's later-wire ack is passed over"
        );
    }

    #[test]
    fn messages_are_released_in_packet_order_and_gaps_are_asked_for() {
        let mut c = granted();
        let mut s = Server::new();
        let a = s.message(4, b"\x01\x00\x00\x00first", 100);
        let b = s.message(3, b"\x02\x00\x00\x00second", 100);
        let c3 = s.message(7, b"\x03\x00\x00\x00third", 100);
        let p2 = s.packet(&a);
        let p3 = s.packet(&b);
        let p4 = s.packet(&c3);
        c.feed(&p4, 1.0).expect("early");
        c.feed(&p3, 1.0).expect("early");
        assert!(c.next_message().is_none(), "held for sequence 2");
        c.tick(1.0);
        let out = c.take_outgoing();
        let req = out
            .iter()
            .map(|d| Header::read(d).unwrap())
            .find(|h| h.flags & flags::RESEND_REQUEST != 0)
            .expect("a resend request");
        assert_eq!(req.flags, 0x1A, "unsynced clock asks too");
        assert_eq!(req.seq, 1, "no data sent yet");
        let body = &out[0][24..];
        assert_eq!(read_list(body), Some(vec![2]));
        c.feed(&p2, 1.1).expect("the gap");
        let got: Vec<u16> = std::iter::from_fn(|| c.next_message())
            .map(|m| m.0)
            .collect();
        assert_eq!(got, [4, 3, 7]);
    }

    #[test]
    fn a_message_in_many_fragments_and_a_skip_reassemble() {
        let mut c = granted();
        let mut s = Server::new();
        let long: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
        let frags = s.message(4, &long, 200);
        assert_eq!(frags.len(), 5);
        let p2 = s.packet(&frags[..2]);
        let p3 = s.packet(&frags[2..4]);
        let p4 = s.packet(&frags[4..]);
        let lost_frags = s.message(4, b"\x09\x00\x00\x00", 100);
        let lost = s.packet(&lost_frags);
        let p5_frags = s.message(4, b"\x0A\x00\x00\x00", 100);
        let p5 = s.packet(&p5_frags);
        drop(lost);
        c.feed(&p2, 1.0).unwrap();
        c.feed(&p4, 1.0).unwrap();
        c.feed(&p3, 1.0).unwrap();
        assert_eq!(c.next_message(), Some((4, long)));
        c.feed(&p5, 1.0).unwrap();
        assert!(c.next_message().is_none(), "5 is missing");
        c.feed(&s.control(0, 0x82, &write_list(&[5])), 1.1)
            .expect("a skip");
        assert_eq!(
            c.next_message().map(|m| m.1),
            Some(b"\x0A\x00\x00\x00".to_vec())
        );
    }

    #[test]
    fn a_time_update_with_the_checksum_bit_fills_its_sequence() {
        let mut c = granted();
        let mut s = Server::new();
        let early = s.control(1, 0x100_0002, &[0u8; 8]);
        assert_eq!(c.feed(&early, 0.2), Ok(()), "before the data, passed over");
        let first = s.message(4, b"\x01\x00\x00\x00", 100);
        let p2 = s.packet(&first);
        // Sequence 3: AC1999's time update, the acknowledged sequence then the time, keyed.
        s.seq += 1;
        let key = s.keys.next();
        let mut body = 2u32.to_le_bytes().to_vec();
        body.extend(12.5f64.to_le_bytes());
        let mut h = Header {
            seq: s.seq,
            flags: 0x100_4002,
            recipient: SERVER,
            len: 12,
            time: s.time,
            table: TABLE,
            ..Header::default()
        };
        h.checksum = h
            .hash()
            .wrapping_add(pyreal_transport::crc::hash32(&body) ^ key);
        let mut update = h.to_bytes().to_vec();
        update.extend(&body);
        let second = s.message(4, b"\x02\x00\x00\x00", 100);
        let p4 = s.packet(&second);
        c.feed(&p2, 1.0).unwrap();
        c.feed(&p4, 1.0).unwrap();
        assert_eq!(c.next_message().map(|m| m.1[0]), Some(1));
        assert!(c.next_message().is_none(), "held for sequence 3");
        c.feed(&update, 1.1).expect("the time update is taken");
        assert_eq!(
            c.next_message().map(|m| m.1[0]),
            Some(2),
            "and fills its slot"
        );
        assert_eq!(
            c.feed(&update, 1.2),
            Err(Rejected),
            "its resend is a duplicate"
        );
        let mut bare_ack = Header {
            seq: 3,
            flags: 0x4000,
            recipient: SERVER,
            len: 4,
            ..Header::default()
        }
        .to_bytes()
        .to_vec();
        bare_ack.extend([3, 0, 0, 0]);
        assert_eq!(
            c.feed(&bare_ack, 1.3),
            Ok(()),
            "a bare ack holds no sequence"
        );
        c.tick(1.3);
        assert!(
            c.take_outgoing()
                .iter()
                .all(|d| Header::read(d).unwrap().flags & flags::RESEND_REQUEST == 0),
            "nothing missing"
        );
    }

    #[test]
    fn sent_data_is_keyed_numbered_and_resent_on_request() {
        let mut c = granted();
        assert!(c.send(6, &[1, 2, 3, 4]));
        assert!(c.send(6, &[7u8; 500]));
        let out = c.take_outgoing();
        assert_eq!(out.len(), 3);
        let mut keys = CryptoSystem::new(0);
        for (i, d) in out.iter().enumerate() {
            let h = Header::read(d).unwrap();
            assert_eq!(h.seq, 2 + u32::try_from(i).unwrap());
            assert_eq!(h.flags, 0x802);
            assert_eq!(h.recipient, CLIENT);
            assert!(d.len() <= 484);
            let flow = &d[20..24];
            let frag = &d[24..];
            assert_eq!(usize::from(h.len), frag.len());
            let f = &read_fragments(frag).unwrap()[0];
            assert_eq!(f.header.queue_id, 6);
            let want = h
                .hash()
                .wrapping_add(body_hash(flow))
                .wrapping_add(f.hash() ^ keys.next());
            assert_eq!(h.checksum, want);
        }
        let blob = |d: &Vec<u8>| read_fragments(&d[24..]).unwrap()[0].header.blob_id();
        assert_eq!(blob(&out[0]), 0x8000_0000_0000_0001);
        assert_eq!(blob(&out[1]), 0x8000_0000_0000_0002);
        assert_eq!(blob(&out[2]), 0x8000_0000_0000_0002);
        assert_eq!(read_fragments(&out[1][24..]).unwrap()[0].payload.len(), 444);

        let s = Server::new();
        let framed: Vec<u8> = [12u32, 2, 3, 99]
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect();
        c.feed(&s.control(4, 0xA, &framed), 1.0).unwrap();
        let again = c.take_outgoing();
        let h = Header::read(&again[0]).unwrap();
        assert_eq!((h.seq, h.flags), (3, 0x803));
        assert_eq!(&again[0][24..], &out[1][24..], "the same fragment");
        let flags: Vec<u32> = again
            .iter()
            .map(|d| Header::read(d).unwrap().flags)
            .collect();
        assert_eq!(flags, [0x803, 0x82, 0x2002], "99 denied in both forms");
        assert_eq!(read_list(&again[2][20..]), Some(vec![99]));
    }

    #[test]
    fn heartbeats_name_the_highest_sequence_and_carry_the_flow_word() {
        let mut c = granted();
        let mut s = Server::new();
        let p_frags = s.message(4, b"\x01\x00\x00\x00abcdefgh", 100);
        let p = s.packet(&p_frags);
        c.feed(&p, 0.5).unwrap();
        c.tick(2.1);
        let out = c.take_outgoing();
        assert_eq!(out.len(), 1);
        let h = Header::read(&out[0]).unwrap();
        assert_eq!(h.flags, 0x32, "first heartbeat asks for the clock");
        assert_eq!(&out[0][24..28], &2u32.to_le_bytes());
        let flow = u32::from_le_bytes([out[0][20], out[0][21], out[0][22], out[0][23]]);
        assert_eq!(flow, u32::try_from(p.len()).unwrap());

        let mut sync = 0.0f64.to_le_bytes().to_vec();
        sync.extend([0u8; 0]);
        c.feed(&s.control(0, 0x12, &sync), 2.2).unwrap();
        c.tick(4.2);
        let h = Header::read(&c.take_outgoing()[0]).unwrap();
        assert_eq!(h.flags, 0x22, "synced");
    }

    #[test]
    fn a_newer_time_byte_reports_the_finished_period() {
        let mut c = granted();
        let mut s = Server::new();
        let p_frags = s.message(4, b"\x01\x00\x00\x00", 100);
        let p = s.packet(&p_frags);
        c.feed(&p, 0.5).unwrap();
        s.time = 5;
        c.feed(&s.control(0, 0x102, &[0u8; 8]), 0.6).unwrap();
        let out = c.take_outgoing();
        let report = Header::read(&out[0]).unwrap();
        assert_eq!((report.flags, report.time, report.len), (0x1002, 4, 4));
        assert_eq!(
            &out[0][20..24],
            &u32::try_from(p.len()).unwrap().to_le_bytes()
        );
        assert_eq!(
            Header::read(&out[1]).unwrap().flags,
            0x102,
            "the switch is answered"
        );
    }

    #[test]
    fn the_server_disconnect_is_answered_and_log_off_sends_both_notices() {
        let mut c = granted();
        let mut h = Header {
            flags: 0x2_0002,
            recipient: SERVER,
            table: TABLE,
            time: 1,
            ..Header::default()
        };
        h.checksum = h.hash();
        c.feed(&h.to_bytes(), 1.0).unwrap();
        assert_eq!(c.state(), State::Closed(Close::ServerDisconnected));
        assert_eq!(Header::read(&c.take_outgoing()[0]).unwrap().flags, 0x2_0002);

        let mut c = granted();
        c.log_off();
        let flags: Vec<u32> = c
            .take_outgoing()
            .iter()
            .map(|d| Header::read(d).unwrap().flags)
            .collect();
        assert_eq!(flags, [0x42, 0x8002]);
        assert_eq!(c.state(), State::Closed(Close::LoggedOff));
    }

    #[test]
    fn silence_closes_the_connection() {
        let mut c = granted();
        c.tick(20.0);
        assert_eq!(c.state(), State::Granted);
        c.tick(30.2);
        assert_eq!(c.state(), State::Closed(Close::TimedOut));
    }

    #[test]
    fn keys_are_held_for_skipped_and_announced_sequences() {
        let mut k = InKeys::new(0);
        let mut reference = CryptoSystem::new(0);
        let draws: Vec<u32> = (0..5).map(|_| reference.next()).collect();
        assert_eq!(k.peek(4), Some(draws[2]), "sequence n takes draw n - 1");
        assert_eq!(k.committed, 1, "a peek commits nothing");
        k.commit(4, true);
        assert_eq!(k.peek(2), Some(draws[0]));
        assert_eq!(k.peek(3), Some(draws[1]));
        assert_eq!(k.peek(4), None, "used");
        k.commit(6, false);
        assert_eq!(k.peek(6), Some(draws[4]), "announced and held");
        k.forget(5);
        assert_eq!(k.peek(5), None, "given up");
        assert_eq!(k.peek(1), None, "the login's");
    }
}
