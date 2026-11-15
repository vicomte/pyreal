//! A join link: a world's host, port and name in one link a host sends a friend, which opens
//! Pyreal's world list with the world filled in, for the friend to confirm.
//!
//! **`pyreal://join?host=<host>&port=<port>&name=<name>`**, Pyreal's own scheme, which the system
//! hands the app (registered in the macOS bundle's `Info.plist`). The old launcher's
//! `dereth://join?...` links read the same; one that says it is for Asheron's Call 2
//! (`game=auberean`) is refused.
//!
//! A link only fills the form: it never connects, logs in or carries an account or a password,
//! and anything else it holds is ignored. Its host and port are checked as a world added by hand
//! is ([`crate::state::custom_host`], [`crate::state::custom_port`]); its name is cut to
//! [`MAX_NAME`] characters, with no control characters.

use serde::{Deserialize, Serialize};

use crate::state::{custom_host, custom_port, CustomWorldError};

/// Pyreal's URL scheme.
pub const SCHEME: &str = "pyreal";

/// The old launcher's scheme, whose links read the same.
pub const OLD_SCHEME: &str = "dereth";

/// The most characters of a world's name a link carries.
pub const MAX_NAME: usize = 80;

/// The longest link read, in bytes.
pub const MAX_LINK: usize = 2048;

/// What a join link says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinLink {
    pub host: String,
    pub port: u16,
    /// The world's name; its host when the link names none.
    pub name: String,
}

/// Why a link is not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinError {
    /// Not a join link at all.
    NotALink,
    /// A join link whose host or port is not one a world can be added with.
    World(CustomWorldError),
    /// A join link for another game.
    OtherGame,
}

impl core::fmt::Display for JoinError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotALink => f.write_str("That is not a link to a world."),
            Self::World(e) => write!(f, "The link's world: {e}"),
            Self::OtherGame => f.write_str("That link is for another game."),
        }
    }
}

impl std::error::Error for JoinError {}

/// `s` with `%XX` decoded and `+` read as a space; an escape that does not read is kept as it is.
fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let hex = |i: usize| b.get(i).and_then(|c| char::from(*c).to_digit(16));
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' => match (hex(i + 1), hex(i + 2)) {
                (Some(h), Some(l)) => {
                    out.push(u8::try_from(h * 16 + l).unwrap_or(b'?'));
                    i += 2;
                }
                _ => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `s` percent-encoded for a query or fragment value: letters, digits and `-._~` as they are,
/// every other byte as `%XX`.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The `key=value` pairs of a query or fragment, decoded.
fn pairs(s: &str) -> Vec<(String, String)> {
    s.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (decode(k), decode(v))
        })
        .collect()
}

/// The part of `link` after the scheme's `join`: its query.
fn details(link: &str) -> Option<&str> {
    let lower = link.to_ascii_lowercase();
    for scheme in [SCHEME, OLD_SCHEME] {
        for prefix in [format!("{scheme}://join"), format!("{scheme}:join")] {
            if lower.starts_with(&prefix) {
                let rest = &link[prefix.len()..];
                let rest = rest.strip_prefix('/').unwrap_or(rest);
                return rest.strip_prefix('?').or_else(|| rest.strip_prefix('#'));
            }
        }
    }
    None
}

/// A world's name from a link: its control characters taken out, trimmed, and cut to
/// [`MAX_NAME`] characters; `host` when nothing is left.
fn clean_name(name: &str, host: &str) -> String {
    let kept: String = name.chars().filter(|c| !c.is_control()).collect();
    let cut: String = kept.trim().chars().take(MAX_NAME).collect();
    let cut = cut.trim();
    if cut.is_empty() {
        host.to_owned()
    } else {
        cut.to_owned()
    }
}

/// Reads a join link.
///
/// # Errors
/// [`JoinError::NotALink`] for anything that is not one, or one over [`MAX_LINK`] bytes;
/// [`JoinError::World`] for a link whose host or port a world cannot be added with;
/// [`JoinError::OtherGame`] for one naming another game.
pub fn parse(link: &str) -> Result<JoinLink, JoinError> {
    let link = link.trim();
    if link.len() > MAX_LINK {
        return Err(JoinError::NotALink);
    }
    let details = details(link).ok_or(JoinError::NotALink)?;
    let pairs = pairs(details);
    let get = |key: &str| {
        pairs
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    };
    if get("game").is_some_and(|g| !g.trim().is_empty() && !g.trim().eq_ignore_ascii_case("dereth"))
    {
        return Err(JoinError::OtherGame);
    }
    let host = custom_host(get("host").unwrap_or("")).map_err(JoinError::World)?;
    let port = custom_port(get("port").unwrap_or("")).map_err(JoinError::World)?;
    Ok(JoinLink {
        host: host.to_owned(),
        port,
        name: clean_name(get("name").unwrap_or(""), host),
    })
}

impl JoinLink {
    /// Pyreal's link: `pyreal://join?host=...&port=...&name=...`.
    #[must_use]
    pub fn app_link(&self) -> String {
        format!(
            "{SCHEME}://join?host={}&port={}&name={}",
            encode(&self.host),
            self.port,
            encode(&self.name)
        )
    }
}

/// The link among a program's arguments, as the system passes it: the first that is a join link
/// of either scheme.
#[must_use]
pub fn from_args(args: &[String]) -> Option<&str> {
    args.iter().map(String::as_str).find(|a| {
        let a = a.to_ascii_lowercase();
        a.starts_with("pyreal:") || a.starts_with("dereth:")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(host: &str, port: u16, name: &str) -> JoinLink {
        JoinLink {
            host: host.into(),
            port,
            name: name.into(),
        }
    }

    #[test]
    fn a_join_link_reads_its_host_port_and_name_in_either_scheme() {
        let want = link("play.example.org", 9000, "Coldeve & Friends");
        for text in [
            "pyreal://join?host=play.example.org&port=9000&name=Coldeve%20%26%20Friends",
            "pyreal://join/?host=play.example.org&port=9000&name=Coldeve+%26+Friends",
            "dereth://join?host=play.example.org&port=9000&name=Coldeve%20%26%20Friends",
            "  PYREAL://join?name=Coldeve%20%26%20Friends&port=9000&host=play.example.org  ",
        ] {
            assert_eq!(parse(text), Ok(want.clone()), "{text}");
        }
    }

    #[test]
    fn a_link_round_trips() {
        let l = link("192.0.2.10", 9050, "Coldeve, \"quotes\" & #hash");
        assert_eq!(parse(&l.app_link()), Ok(l.clone()));
        assert!(l
            .app_link()
            .starts_with("pyreal://join?host=192.0.2.10&port=9050&name="));
    }

    #[test]
    fn a_link_for_another_game_or_with_a_bad_world_is_refused() {
        assert_eq!(
            parse("dereth://join?host=a.example&port=9200&name=A&game=auberean"),
            Err(JoinError::OtherGame)
        );
        assert!(parse("pyreal://join?host=a.example&port=9000&game=dereth").is_ok());
        assert_eq!(
            parse("pyreal://join?port=9000"),
            Err(JoinError::World(CustomWorldError::NoHost))
        );
        assert_eq!(
            parse("pyreal://join?host=a&port=0"),
            Err(JoinError::World(CustomWorldError::BadPort))
        );
        for text in [
            "",
            "pyreal://play?host=a&port=9000",
            "https://example.org/join#host=a&port=1",
        ] {
            assert_eq!(parse(text), Err(JoinError::NotALink), "{text}");
        }
    }

    #[test]
    fn a_link_never_carries_an_account_and_its_name_is_cleaned() {
        let l = parse(
            "pyreal://join?host=a.example&port=9000&name=%0D%0AHi%07%20there&account=bob&password=hunter2",
        )
        .unwrap();
        assert_eq!(l, link("a.example", 9000, "Hi there"));
        let long = "x".repeat(200);
        let l = parse(&format!(
            "pyreal://join?host=a.example&port=9000&name={long}"
        ))
        .unwrap();
        assert_eq!(l.name.chars().count(), MAX_NAME);
        assert_eq!(
            parse("pyreal://join?host=a.example&port=9000")
                .unwrap()
                .name,
            "a.example"
        );
    }

    #[test]
    fn the_link_is_found_among_the_programs_arguments() {
        let args = [
            "--x".to_owned(),
            "pyreal://join?host=a&port=9000".to_owned(),
        ];
        assert_eq!(from_args(&args), Some("pyreal://join?host=a&port=9000"));
        assert_eq!(from_args(&["-a".to_owned()]), None);
    }
}
