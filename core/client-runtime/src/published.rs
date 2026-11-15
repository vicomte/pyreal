//! What a world's server publishes beside its status, as Empyrean names it: the world's status
//! document says where its portal table (`"portals"`) and its places (`"places"`) are, each a path
//! on the status's own address, or `null` when it does not publish them. The places are asked for
//! as version 2 (`?version=2`), which adds the world's map marks; a server that only knows version
//! 1 answers that. A document the server is still gathering answers `503` for a while after it
//! starts, so it is asked again.

use crate::weenie_list::FetchText;

/// How many times a document answering `503` is asked for, and how many seconds apart.
pub const TRIES: u32 = 10;
pub const AGAIN: f64 = 3.0;

/// The documents a world publishes, as fetched.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Published {
    /// Its places (a place store, `dereth-places-store`).
    pub places: Option<String>,
    /// Its portal table (`dereth-navigation-store`).
    pub portals: Option<String>,
}

/// The addresses of the portal table and the places that `status`, the world's status document
/// read from `status_url`, names: each a path on the status's address, the places asked for as
/// version 2. A value that is no such path is passed over, so a status can never send the client
/// to another host.
#[must_use]
pub fn documents(status_url: &str, status: &str) -> (Option<String>, Option<String>) {
    let Some(origin) = origin(status_url) else {
        return (None, None);
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(status) else {
        return (None, None);
    };
    let path = |key: &str| {
        v.get(key)?
            .as_str()
            .filter(|p| {
                p.starts_with('/')
                    && !p.starts_with("//")
                    && !p.contains(|c: char| c == '\\' || c.is_whitespace() || c.is_control())
            })
            .map(|p| format!("{origin}{p}"))
    };
    let places = path("places").map(|url| {
        let join = if url.contains('?') { '&' } else { '?' };
        format!("{url}{join}version=2")
    });
    (path("portals"), places)
}

/// `http://host[:port]` or `https://host[:port]` of a URL of either scheme; `None` for any other,
/// or one naming a user.
fn origin(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))?;
    let scheme = url.len() - rest.len();
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    if end == 0 || rest[..end].contains('@') {
        return None;
    }
    Some(&url[..scheme + end])
}

/// Reads what the world whose status is at `status_url` publishes: the status, then its places,
/// and its portal table when `portals` asks for it. Each is fetched by `fetch`, and one answering
/// `503` is asked again up to [`TRIES`] times, `wait` given the seconds between.
pub fn read(
    status_url: &str,
    portals: bool,
    fetch: &FetchText,
    wait: &mut dyn FnMut(f64),
) -> Published {
    let mut get = |url: &str| {
        let mut tries = 1;
        loop {
            match fetch(url) {
                Err(e) if e.ends_with("HTTP 503") && tries < TRIES => {
                    tries += 1;
                    wait(AGAIN);
                }
                Err(e) => {
                    tracing::info!("what the world publishes, from {url}: {e}");
                    return None;
                }
                Ok(text) => return Some(text),
            }
        }
    };
    let Some(status) = get(status_url) else {
        return Published::default();
    };
    let (portals_url, places_url) = documents(status_url, &status);
    Published {
        places: places_url.and_then(|u| get(&u)),
        portals: portals_url.filter(|_| portals).and_then(|u| get(&u)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    #[test]
    fn the_status_names_its_documents_on_its_own_address_and_the_places_as_version_two() {
        let status =
            r#"{"world_name":"W","portals":"/portals","places":"/places","not_ported":{}}"#;
        assert_eq!(
            documents("http://127.0.0.1:9100/status", status),
            (
                Some("http://127.0.0.1:9100/portals".to_owned()),
                Some("http://127.0.0.1:9100/places?version=2".to_owned())
            )
        );
        assert_eq!(
            documents(
                "https://play.example.org/game/status",
                r#"{"places":"/places?w=1"}"#
            ),
            (
                None,
                Some("https://play.example.org/places?w=1&version=2".to_owned())
            )
        );
        assert_eq!(
            documents(
                "http://127.0.0.1:9100/status",
                r#"{"portals":null,"places":null}"#
            ),
            (None, None),
            "a world that publishes nothing"
        );
        for elsewhere in [
            "http://evil.example/places",
            "//evil.example/places",
            "places",
        ] {
            let status = format!(r#"{{"places":"{elsewhere}"}}"#);
            assert_eq!(
                documents("http://127.0.0.1:9100/status", &status),
                (None, None)
            );
        }
        assert_eq!(
            documents("file:///status", r#"{"places":"/places"}"#),
            (None, None)
        );
        assert_eq!(
            documents("http://user@host/status", r#"{"places":"/places"}"#),
            (None, None)
        );
        assert_eq!(documents("http://127.0.0.1:9100/status", "{"), (None, None));
    }

    /// Reads through `server` (an address to what each fetch of it brings, in turn; refused once
    /// it has nothing more): what came, the addresses asked and the waits.
    fn read_from(
        portals: bool,
        server: BTreeMap<&'static str, Vec<Result<&'static str, &'static str>>>,
    ) -> (Published, Vec<String>, Vec<f64>) {
        let server = Arc::new(Mutex::new(server));
        let asked = Arc::new(Mutex::new(Vec::new()));
        let (s, a) = (Arc::clone(&server), Arc::clone(&asked));
        let fetch: FetchText = Arc::new(move |url: &str| {
            a.lock().unwrap().push(url.to_owned());
            let mut server = s.lock().unwrap();
            match server.get_mut(url).filter(|v| !v.is_empty()) {
                Some(answers) => answers.remove(0).map(str::to_owned).map_err(str::to_owned),
                None => Err(format!("{url}: refused")),
            }
        });
        let mut waits = Vec::new();
        let got = read("http://127.0.0.1:9100/status", portals, &fetch, &mut |s| {
            waits.push(s)
        });
        let asked = asked.lock().unwrap().clone();
        (got, asked, waits)
    }

    const STATUS: &str = r#"{"world_name":"Server","portals":"/portals","places":"/places"}"#;
    const PLACES: &str = "dereth-places-store\t2\nworld\tServer\n";
    const PORTALS: &str = "dereth-navigation-store\t1\n";

    #[test]
    fn the_status_then_the_places_and_the_portals_are_read_and_a_503_is_asked_again() {
        let (got, asked, waits) = read_from(
            true,
            BTreeMap::from([
                ("http://127.0.0.1:9100/status", vec![Ok(STATUS)]),
                (
                    "http://127.0.0.1:9100/places?version=2",
                    vec![Err("x answered HTTP 503"), Ok(PLACES)],
                ),
                ("http://127.0.0.1:9100/portals", vec![Ok(PORTALS)]),
            ]),
        );
        assert_eq!(
            got,
            Published {
                places: Some(PLACES.to_owned()),
                portals: Some(PORTALS.to_owned())
            }
        );
        assert_eq!(
            asked,
            [
                "http://127.0.0.1:9100/status",
                "http://127.0.0.1:9100/places?version=2",
                "http://127.0.0.1:9100/places?version=2",
                "http://127.0.0.1:9100/portals",
            ]
        );
        assert_eq!(waits, [AGAIN]);
    }

    #[test]
    fn a_document_that_stays_503_is_given_up_after_ten_tries_and_portals_are_read_only_when_wanted()
    {
        let (got, asked, waits) = read_from(
            false,
            BTreeMap::from([
                ("http://127.0.0.1:9100/status", vec![Ok(STATUS)]),
                (
                    "http://127.0.0.1:9100/places?version=2",
                    vec![Err("x answered HTTP 503"); 50],
                ),
            ]),
        );
        assert_eq!(got, Published::default());
        assert_eq!(asked.iter().filter(|u| u.contains("/places")).count(), 10);
        assert!(!asked.iter().any(|u| u.ends_with("/portals")));
        assert_eq!(waits, [AGAIN; 9]);
        let (got, asked, _) = read_from(false, BTreeMap::new());
        assert_eq!(got, Published::default(), "no status, nothing");
        assert_eq!(asked, ["http://127.0.0.1:9100/status"]);
    }
}
