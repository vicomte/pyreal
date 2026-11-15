//! The rule for a world's status document address: `https://` to any host, `http://` only to this
//! machine. The rule is one for every address a launcher is given, so no list can point a launcher
//! anywhere else.
//!
//! Two addresses name one world when they differ only in what a URL leaves to its reader: the
//! case of the scheme and the host, the scheme's own port, and a path that is only a `/`. A status
//! address with no path names the world's `/status`.

/// Whether `url` may be a world's status document: `https://` to any host, `http://` only to this
/// machine.
#[must_use]
pub fn is_status_url(url: &str) -> bool {
    let url = url.trim();
    let Some((secure, rest)) = split_scheme(url, "https://", "http://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    !authority.is_empty() && !authority.contains('@') && (secure || is_loopback(host(authority)))
}

/// Whether `a` and `b` are one status document.
#[must_use]
pub fn same_status(a: &str, b: &str) -> bool {
    same(a, b, "/status")
}

/// Whether `a` and `b` are one address, a bare path read as `bare`.
fn same(a: &str, b: &str, bare: &str) -> bool {
    match (canonical(a, bare), canonical(b, bare)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// `url` as the one spelling it compares by: the scheme and the host in lower case, no port where
/// it is the scheme's own, `bare` for no path or `/`, no `/` at the path's end. `None` for no URL.
fn canonical(url: &str, bare: &str) -> Option<String> {
    let url = url.trim();
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    if authority.is_empty() {
        return None;
    }
    let mut authority = authority.to_ascii_lowercase();
    let own = match scheme.as_str() {
        "https" => ":443",
        "http" => ":80",
        _ => "",
    };
    if !own.is_empty() && authority.ends_with(own) {
        authority.truncate(authority.len() - own.len());
    }
    let tail = match tail.trim_end_matches('/') {
        "" => bare,
        t => t,
    };
    Some(format!("{scheme}://{authority}{tail}"))
}

/// Whether `url` starts with `secure` or `plain` (without regard to case), and what follows.
fn split_scheme<'a>(url: &'a str, secure: &str, plain: &str) -> Option<(bool, &'a str)> {
    let starts = |s: &str| {
        url.get(..s.len())
            .is_some_and(|p| p.eq_ignore_ascii_case(s))
    };
    if starts(secure) {
        Some((true, &url[secure.len()..]))
    } else if starts(plain) {
        Some((false, &url[plain.len()..]))
    } else {
        None
    }
}

/// The host of an authority, without its port: `[::1]` for `[::1]:9100`.
fn host(authority: &str) -> &str {
    if authority.starts_with('[') {
        return authority
            .split_once(']')
            .map_or(authority, |(h, _)| &authority[..=h.len()]);
    }
    authority.rsplit_once(':').map_or(authority, |(h, _)| h)
}

fn is_loopback(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") || host == "[::1]" {
        return true;
    }
    host.parse::<std::net::Ipv4Addr>()
        .is_ok_and(|ip| ip.is_loopback())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_document_is_https_or_plain_only_to_this_machine() {
        assert!(is_status_url("https://play.example.org/status"));
        assert!(is_status_url("http://127.0.0.1:9100/status"));
        assert!(!is_status_url("http://play.example.org/status"));
        assert!(!is_status_url("wss://play.example.org/status"));
        assert!(!is_status_url("javascript:alert(1)"));
        assert!(!is_status_url("https://user@play.example.org/status"));
    }

    #[test]
    fn two_spellings_of_one_address_are_one_world() {
        assert!(same_status(
            "https://Play.Example.org:443/",
            "https://play.example.org/status"
        ));
        assert!(same_status(
            "https://play.example.org/status/",
            "HTTPS://play.example.org/status"
        ));
        assert!(!same_status(
            "https://a.example.org/status",
            "https://b.example.org/status"
        ));
        assert!(!same_status(
            "http://127.0.0.1:9100/status",
            "http://127.0.0.1:9101/status"
        ));
        assert!(!same_status("", ""));
    }
}
