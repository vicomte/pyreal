//! The 1999 driver: [`ClientNetwork`] on a world that speaks the November 1999 client's wire.
//!
//! The wire is `pyreal_wire1999`'s; this drives it from the frame. One port, no later handshake:
//! the login goes to the server, the grant comes back, the acknowledgement goes out, and from then
//! on the same frame pump runs. The session reads the link through [`super::LinkTransport`].

use std::net::SocketAddr;

use pyreal_client_net::client_session::Session;
use pyreal_client_net::socket::{parse_host_spec, resolve_sin_addr};
use pyreal_primitives::LocalTime;
use pyreal_transport::conn::{LoginState, NetErrorCode};
use pyreal_wire1999::{Close, Connection, Credential, CredentialError, Link1999, State};

use super::{link_status_holder, ClientNetwork, ClientNetworkError, LinkStatus, LinkTransport};

/// Seconds between the link lamp's heartbeats while connected.
const LAMP_INTERVAL: f64 = 2.0;

/// What the 1999 driver keeps beside the link.
#[derive(Debug, Default)]
pub struct Driver1999 {
    last_lamp: Option<LocalTime>,
    lamp_pending: bool,
}

impl Driver1999 {
    /// The lamp's heartbeat edge: no loss is measured on this wire.
    pub fn take_heartbeat(&mut self) -> Option<f64> {
        std::mem::take(&mut self.lamp_pending).then_some(0.0)
    }
}

/// The newest portal and cell iterations the store holds, overlay included: what a 1999 login
/// reports.
#[must_use]
pub fn login_iterations(store: &pyreal_dat::RetailDatStore) -> (u32, u32) {
    let newest = |f: &pyreal_dat::DatFile| {
        f.header_iteration()
            .or_else(|| f.iteration_list().ok().and_then(|l| l.into_iter().max()))
            .or_else(|| f.base_header_iteration())
            .unwrap_or(0)
    };
    (newest(store.portal()), newest(store.cell()))
}

/// The world's name on the character screen, which the 1999 wire never sends: its address.
#[must_use]
pub fn world_name_1999(cfg: &crate::config::Config) -> String {
    if cfg.host.contains(':') {
        cfg.host.clone()
    } else {
        format!("{}:{}", cfg.host, cfg.port)
    }
}

impl ClientNetwork {
    /// A network for a 1999 world: the login carries `account:password` in one 20-character field
    /// and the `iterations` the files hold. `world_name` names the world on the character screen.
    ///
    /// # Errors
    /// [`ClientNetworkError::BadServerAddress`] for a host that does not resolve, and
    /// [`ClientNetworkError::CredentialTooLong`] or [`ClientNetworkError::CredentialNotPlain`] for a
    /// credential the login cannot carry, before anything is sent.
    pub fn new_1999(
        host: &str,
        default_port: u16,
        account: &str,
        password: &str,
        iterations: (u32, u32),
        world_name: &str,
    ) -> Result<Self, ClientNetworkError> {
        let credential = Credential::new(account, password).map_err(|e| match e {
            CredentialError::TooLong => ClientNetworkError::CredentialTooLong,
            CredentialError::Empty => ClientNetworkError::BadCredential,
            CredentialError::ColonInAccount | CredentialError::NotPrintable => {
                ClientNetworkError::CredentialNotPlain
            }
        })?;
        let spec = parse_host_spec(host, default_port);
        let logon_addr = resolve_sin_addr(&spec.host, spec.port)
            .ok_or_else(|| ClientNetworkError::BadServerAddress(host.to_string()))?;
        let link = Link1999::new(Connection::new(credential, iterations), world_name);
        Ok(Self {
            session: Session::new(LinkTransport::ac1999(link)),
            auth: pyreal_transport::conn::ConnectionAuthenticator::account_only(""),
            login: LoginState::new(),
            status: LinkStatus::Idle,
            logon_addr,
            peer: Some(logon_addr),
            handshake_addr: None,
            cookie: 0,
            rec_id: 0,
            net_id: 0,
            iteration: 0,
            last_connect_ack: None,
            pending: Vec::new(),
            error: None,
            rejected: 0,
            trace: crate::trace::net(),
            connected_pending: false,
            login_completed: false,
            ac1999: Some(Driver1999::default()),
        })
    }

    /// Whether this network speaks the 1999 wire.
    #[must_use]
    pub fn is_1999(&self) -> bool {
        self.ac1999.is_some()
    }

    fn link(&mut self) -> Option<&mut Link1999> {
        self.session.transport.link1999_mut()
    }

    pub(super) fn feed_1999(&mut self, raw: &[u8], from: SocketAddr, now: LocalTime) {
        // The server answers from its port or the one after it; only the address is held to.
        if from.ip() != self.logon_addr.ip() {
            self.rejected = self.rejected.saturating_add(1);
            return;
        }
        if self.trace {
            if let Some(h) = pyreal_wire1999::packet::Header::read(raw) {
                tracing::debug!(
                    target: "pyreal::trace::net",
                    "recv 1999 seq={} flags={:#010X} len={} from={from}",
                    h.seq,
                    h.flags,
                    raw.len()
                );
            }
        }
        let fed = self
            .link()
            .map(|l| l.connection_mut().feed(raw, now.0).is_ok());
        if fed == Some(false) {
            self.rejected = self.rejected.saturating_add(1);
        }
        self.follow_1999(now);
    }

    /// Bring the link status into line with the connection.
    fn follow_1999(&mut self, now: LocalTime) {
        let Some(state) = self.link().map(|l| l.connection().state()) else {
            return;
        };
        match state {
            State::Authenticating => {
                if self.status == LinkStatus::Idle {
                    self.status = LinkStatus::LoginAuthenticating;
                }
            }
            State::Granted => self.status = LinkStatus::LoginConnecting,
            State::Connected => {
                if self.status != LinkStatus::Connected {
                    self.status = LinkStatus::Connected;
                    self.connected_pending = true;
                    self.login_completed = true;
                }
                if let Some(d) = self.ac1999.as_mut() {
                    if d.last_lamp
                        .is_none_or(|t| now.seconds_since(t) >= LAMP_INTERVAL)
                    {
                        d.last_lamp = Some(now);
                        d.lamp_pending = true;
                    }
                }
            }
            State::Closed(close) => {
                if matches!(
                    self.status,
                    LinkStatus::Disconnected | LinkStatus::Failed(_)
                ) {
                    return;
                }
                if close == Close::LoggedOff {
                    self.status = LinkStatus::Disconnected;
                    link_status_holder::disconnected();
                } else if self.login_completed {
                    tracing::info!("the 1999 connection ended: {close:?}");
                    self.raise_server_died();
                } else {
                    tracing::info!("the 1999 login ended: {close:?}");
                    self.fail_login(net_error(close));
                }
            }
        }
    }

    pub(super) fn tick_1999(&mut self, now: LocalTime) {
        self.receive_1999(now);
        self.session.tick(now);
        self.send_1999(now);
    }

    pub(super) fn receive_1999(&mut self, now: LocalTime) {
        if let Some(l) = self.link() {
            l.connection_mut().tick(now.0);
        }
        self.follow_1999(now);
        self.session.receive(now);
        self.session.network_use_time(now);
    }

    pub(super) fn send_1999(&mut self, now: LocalTime) {
        // World entry is a later milestone: the session's transport calls have nothing to do here.
        let _ = self.session.take_transport_calls();
        if let Some(l) = self.link() {
            l.connection_mut().tick(now.0);
        }
        self.follow_1999(now);
        let to = self.logon_addr;
        let out = self
            .link()
            .map(|l| l.connection_mut().take_outgoing())
            .unwrap_or_default();
        self.pending.extend(out.into_iter().map(|d| (d, to)));
    }

    pub(super) fn goodbye_1999(&self) -> Vec<(Vec<u8>, SocketAddr)> {
        let Some(grant) = self
            .session
            .transport
            .link1999()
            .and_then(|l| l.connection().grant().copied())
        else {
            return Vec::new();
        };
        use pyreal_wire1999::packet::{bare, flags};
        [flags::LOG_OFF, flags::RESET]
            .into_iter()
            .map(|f| {
                (
                    bare(0, f | flags::CHECKSUM, grant.client_id, 0, grant.table),
                    self.logon_addr,
                )
            })
            .collect()
    }

    pub(super) fn log_off_1999(&mut self) -> usize {
        let to = self.logon_addr;
        let out = self
            .link()
            .map(|l| {
                let c = l.connection_mut();
                c.log_off();
                c.take_outgoing()
            })
            .unwrap_or_default();
        let n = out.len();
        self.pending.extend(out.into_iter().map(|d| (d, to)));
        n
    }
}

/// The connection error the player is shown for a 1999 connection that ended.
fn net_error(close: Close) -> NetErrorCode {
    match close {
        // AC1999 refuses with the later client's own string ids.
        Close::Refused { string_id, .. } => NetErrorCode::unpack(&string_id.to_le_bytes())
            .unwrap_or(NetErrorCode::AccountAuthenticationFailed),
        Close::VersionMismatch => NetErrorCode::NetVersionMismatch,
        Close::LoginTimedOut | Close::TimedOut => NetErrorCode::ClientTimedOutServer,
        Close::ServerDisconnected => NetErrorCode::ServerClosedConnection,
        Close::BadGrant => NetErrorCode::AbortedHandshake,
        Close::NetError | Close::LoggedOff => NetErrorCode::Generic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyreal_wire1999::packet::{unkeyed, Datagram, Header};

    fn grant(seed: u32) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&1.0f64.to_le_bytes());
        body.extend_from_slice(&2.0f64.to_le_bytes());
        body.extend_from_slice(&0x77u32.to_le_bytes());
        body.extend_from_slice(&7u32.to_le_bytes());
        body.extend_from_slice(b"4.35752");
        body.extend_from_slice(&seed.to_le_bytes());
        body.extend_from_slice(&seed.to_le_bytes());
        unkeyed(0, 0x202, 11, 1, 0, &body)
    }

    fn server() -> SocketAddr {
        "127.0.0.1:9030".parse().unwrap()
    }

    #[test]
    fn a_credential_over_twenty_characters_is_refused_before_connecting() {
        let long = ClientNetwork::new_1999(
            "127.0.0.1:9030",
            9030,
            "abcdefghij",
            "1234567890",
            (102, 67),
            "AC1999",
        );
        assert!(matches!(long, Err(ClientNetworkError::CredentialTooLong)));
        let text = ClientNetworkError::CredentialTooLong.to_string();
        assert!(!text.contains('\u{2014}') && text.ends_with('.'));
        assert!(matches!(
            ClientNetwork::new_1999("127.0.0.1:9030", 9030, "a:b", "c", (102, 67), "AC1999"),
            Err(ClientNetworkError::CredentialNotPlain)
        ));
        assert!(ClientNetwork::new_1999(
            "127.0.0.1:9030",
            9030,
            "abcdefghij",
            "123456789",
            (102, 67),
            "AC1999"
        )
        .is_ok());
    }

    #[test]
    fn the_login_goes_to_the_server_and_the_grant_is_acknowledged() {
        let mut c = ClientNetwork::new_1999(
            "127.0.0.1:9030",
            9030,
            "Demo",
            "Secret",
            (102, 67),
            "AC1999",
        )
        .expect("fits");
        assert!(c.is_1999());
        c.tick(LocalTime(0.0));
        let out = c.take_outgoing();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].1, server());
        let login = Datagram::parse(&out[0].0).expect("a 1999 login");
        assert_eq!((login.header.seq, login.header.flags), (1, 2));
        assert_eq!(&login.body[20..31], b"demo:secret");
        assert_eq!(c.status(), LinkStatus::LoginAuthenticating);

        c.feed(&grant(0), "127.0.0.1:9031".parse().unwrap(), LocalTime(0.1));
        assert_eq!(c.status(), LinkStatus::LoginConnecting);
        c.packet_controller_use_time(LocalTime(0.1));
        let out = c.take_outgoing();
        let ack = Header::read(&out[0].0).unwrap();
        assert_eq!((ack.seq, ack.flags, ack.recipient), (0, 0x202, 0x77));
        assert_eq!(
            out[0].1,
            server(),
            "everything goes to the port the login went to"
        );
    }

    #[test]
    fn a_refused_login_is_a_login_refusal() {
        let mut c = ClientNetwork::new_1999(
            "127.0.0.1:9030",
            9030,
            "demo",
            "secret",
            (102, 67),
            "AC1999",
        )
        .expect("fits");
        c.tick(LocalTime(0.0));
        // AC1999's wrong password: its login-failed id, table 8, no checksum bit.
        let mut body = 0x04DF_9C54u32.to_le_bytes().to_vec();
        body.extend(8u32.to_le_bytes());
        c.feed(
            &unkeyed(0, 0x10_0000, 11, 0, 0, &body),
            server(),
            LocalTime(0.1),
        );
        assert_eq!(c.login_refusal(), Some(NetErrorCode::ClientLogOnFailed));
        // Its other two: a full server, and a login it could not read.
        let refused = |id: u32| {
            net_error(Close::Refused {
                string_id: id,
                table_id: 8,
            })
        };
        assert_eq!(refused(0x00F9_982C), NetErrorCode::ServerFull);
        assert_eq!(refused(0x082E_3779), NetErrorCode::BadCryptoKey);
    }
}
