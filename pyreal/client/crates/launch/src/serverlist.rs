//! The list of known worlds, where it is read from, and the copy of it kept for a day.
//!
//! The list is the Dereth Network hub's (`/v1/servers`, read a page at a time), which imports the
//! community's list (`Servers.xml` in the `acresources/serverslist` repository) daily and adds the
//! worlds registered at the hub. When the hub does not answer with its list, the community's own
//! `Servers.xml` is read instead ([`read_list`]). Dereth's launcher reads the same two, in the same
//! order. The copy also carries the old launcher's worlds over on a first run
//! ([`crate::state::import_launcher`]).
//!
//! Per world the list gives an id that never changes, a name, a description, the emulator (`ACE`
//! or `GDL`), the host and port, the rules (`PvE` or `PvP`), how settled the world is and its
//! website and Discord. A world is kept under [`slug_for`] its name, so what is remembered for it
//! (its accounts, passwords and choices) stays with it.
//!
//! Reading is tolerant: a row without a name is skipped, a malformed port is no endpoint, and
//! nothing in one row can cost the player another.

use std::collections::{BTreeMap, HashSet};
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::world::{Emulator, Endpoint, Game, Links, World};

/// How settled a world the hub listed for demonstration is said to be: a test world.
pub const TEST_WORLD: &str = "Test";

/// The day's copy, in the launcher's data folder.
pub const CACHE_FILE: &str = "world-list.json";

/// How long a copy is used before the list is fetched again: a day.
pub const MAX_AGE_SECS: u64 = 24 * 60 * 60;

/// Why the list could not be read at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MalformedList(pub String);

impl core::fmt::Display for MalformedList {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "the world list is unreadable: {}", self.0)
    }
}

impl std::error::Error for MalformedList {}

/// A world's identity from its name: lower case, every run of other characters one dash, none at
/// either end (`Asheron4Fun.com` is `asheron4fun-com`). Accounts, passwords and remembered choices
/// are kept under it. It is the spelling the launcher's earlier world source used, so what was kept
/// under a world before stays with it.
pub fn slug_for(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            if dash && !out.is_empty() {
                out.push('-');
            }
            dash = false;
            out.push(c.to_ascii_lowercase());
        } else {
            dash = true;
        }
    }
    out
}

/// Read `Servers.xml`: its worlds, in the list's order.
///
/// # Errors
/// [`MalformedList`] when the body is not text or holds no `ArrayOfServerItem`.
pub fn parse_servers_xml(body: &[u8]) -> Result<Vec<World>, MalformedList> {
    let text = std::str::from_utf8(body).map_err(|e| MalformedList(e.to_string()))?;
    let text = text.trim_start_matches('\u{feff}');
    let text = strip_comments(text);
    if !text.contains("<ArrayOfServerItem") {
        return Err(MalformedList("no ArrayOfServerItem".into()));
    }
    let mut taken = HashSet::new();
    Ok(elements(&text, "ServerItem")
        .into_iter()
        .filter_map(|item| world_of(&children(item), &mut taken))
        .collect())
}

fn world_of(row: &BTreeMap<String, String>, taken: &mut HashSet<String>) -> Option<World> {
    let field = |k: &str| row.get(k).map(|s| s.trim()).filter(|s| !s.is_empty());
    let name = field("name")?;
    let id = field("id").map(str::to_owned);
    let slug = take_slug(name, id.as_deref(), taken);

    let mut w = World::new(slug, name);
    w.list_id = id;
    w.description = field("description").map(str::to_owned);
    w.emulator = field("emu").map(Emulator::parse).unwrap_or_default();
    w.ruleset = field("type").map(str::to_owned);
    w.development_status = field("status").map(str::to_owned);
    let port = field("server_port")
        .and_then(|p| p.parse::<u16>().ok())
        .filter(|p| *p > 0);
    if let (Some(address), Some(port)) = (field("server_host"), port) {
        w.endpoint = Some(Endpoint {
            address: address.to_owned(),
            port,
            transport: None,
        });
    }
    w.links = Links {
        website: field("website_url").map(str::to_owned),
        discord: field("discord_url").map(str::to_owned),
        ..Links::default()
    };
    // What the list cannot say about the few worlds that run a client of their own.
    crate::known::apply(&mut w);
    Some(w)
}

/// The key a world named `name` is kept under before it is told apart from another of the same
/// spelling: [`slug_for`] its name, or `fallback` (its id) for a name with no letter or digit in
/// it.
fn key_for(name: &str, fallback: Option<&str>) -> String {
    let slug = slug_for(name);
    if slug.is_empty() {
        fallback.unwrap_or("world").to_owned()
    } else {
        slug
    }
}

/// The key a world named `name` is kept under: [`key_for`] it. Two names with one spelling: the
/// later one is told apart by a number.
fn take_slug(name: &str, fallback: Option<&str>, taken: &mut HashSet<String>) -> String {
    let mut slug = key_for(name, fallback);
    if taken.contains(&slug) {
        let base = slug.clone();
        slug = (2u32..)
            .map(|n| format!("{base}-{n}"))
            .find(|s| !taken.contains(s))
            .expect("some slug is free");
    }
    taken.insert(slug.clone());
    slug
}

// ----- the hub's list ------------------------------------------------------------------------------

/// One page of the hub's list: its rows, and where the next page starts if there is one.
fn hub_page(text: &str) -> Result<(Vec<Value>, Option<u64>), MalformedList> {
    let mut page: Value = serde_json::from_str(text).map_err(|e| MalformedList(e.to_string()))?;
    let next = page.get("nextOffset").and_then(Value::as_u64);
    match page.get_mut("data").map(Value::take) {
        Some(Value::Array(rows)) => Ok((rows, next)),
        _ => Err(MalformedList("no list of servers".into())),
    }
}

/// The rows of the hub's list, its pages in order, each world once: a world that moved from one
/// page to the next while they were read is read where it was first seen.
///
/// # Errors
/// [`MalformedList`] when a page is not a page of the list.
pub(crate) fn hub_rows(pages: &[String]) -> Result<Vec<Value>, MalformedList> {
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    for page in pages {
        for row in hub_page(page)?.0 {
            let hub_slug = row.get("slug").and_then(Value::as_str).map(str::to_owned);
            if hub_slug.is_some_and(|s| !seen.insert(s)) {
                continue;
            }
            rows.push(row);
        }
    }
    Ok(rows)
}

/// Read the hub's list, its pages in order: the worlds to show, in its order.
///
/// # Errors
/// [`MalformedList`] when a page is not a page of the list.
pub fn parse_hub_pages(pages: &[String]) -> Result<Vec<World>, MalformedList> {
    let mut taken = HashSet::new();
    Ok(hub_rows(pages)?
        .iter()
        .filter_map(|row| hub_world(row, &mut taken))
        .collect())
}

/// A text field of a hub row, at the JSON pointer `at`; `None` when it is missing or blank.
fn hub_text<'a>(row: &'a Value, at: &str) -> Option<&'a str> {
    row.pointer(at)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// The name of the world a hub row shows, with the id to key it by when the name has no letter or
/// digit in it, and the address and port to connect to. `None` for a row the launcher does not
/// show: one with no name, and one with no address and port.
fn hub_shown(row: &Value) -> Option<(&str, Option<&str>, &str, u16)> {
    let name = hub_text(row, "/name")?;
    let address = hub_text(row, "/endpoint/address")?;
    let port = row
        .pointer("/endpoint/port")
        .and_then(Value::as_u64)
        .and_then(|p| u16::try_from(p).ok())
        .filter(|p| *p > 0)?;
    // The community list's id, which the hub keeps for a world it imported from it; else the
    // hub's slug.
    let fallback = hub_text(row, "/provenance/externalId").or(hub_text(row, "/slug"));
    Some((name, fallback, address, port))
}

/// One world of the hub's list, read as the same world is from `Servers.xml`. `None` for a row
/// with no name, one with no address and port to connect to, and one of a game this launcher does
/// not know.
fn hub_world(row: &Value, taken: &mut HashSet<String>) -> Option<World> {
    let text = |at: &str| hub_text(row, at);
    let game = match row.get("game") {
        None | Some(Value::Null) => Game::Dereth,
        Some(g) => Game::parse(g.as_str()?)?,
    };
    let (name, fallback, address, port) = hub_shown(row)?;
    // The community list's id, which the hub keeps for a world it imported from it.
    let list_id = text("/provenance/externalId").map(str::to_owned);
    // Kept under its name, not the hub's slug, as the community list's world is.
    let slug = take_slug(name, fallback, taken);

    let mut w = World::new(slug, name);
    w.game = game;
    w.list_id = list_id;
    w.hub_slug = text("/slug").map(str::to_owned);
    w.description = text("/description").map(str::to_owned);
    w.emulator = text("/emulator/family")
        .map(Emulator::parse)
        .unwrap_or_default();
    w.emulator_version = text("/emulator/version").map(str::to_owned);
    // The community list's own word for the rules, which the hub keeps beside its own.
    w.ruleset = text("/provenance/legacyType")
        .or_else(|| text("/ruleset"))
        .map(str::to_owned);
    w.development_status = text("/maturity")
        .filter(|m| !m.eq_ignore_ascii_case("Unspecified"))
        .map(str::to_owned);
    // A world the hub lists for demonstration says so where its status is shown.
    if row.get("isDemo").and_then(Value::as_bool) == Some(true) {
        w.development_status = Some(TEST_WORLD.to_owned());
    }
    w.endpoint = Some(Endpoint {
        address: address.to_owned(),
        port,
        transport: None,
    });
    w.links = Links {
        website: text("/website").map(str::to_owned),
        discord: text("/discord").map(str::to_owned),
        ..Links::default()
    };
    w.status_url = hub_status_url(row);
    crate::known::apply(&mut w);
    Some(w)
}

/// The status document of the world a hub row shows, as the hub names it, only within the rule a
/// launcher connects by ([`crate::direct`]).
pub(crate) fn hub_status_url(row: &Value) -> Option<String> {
    hub_text(row, "/statusUrl")
        .filter(|u| crate::direct::is_status_url(u))
        .map(str::to_owned)
}

/// The day's copy: the list as it was fetched, when, and from which sources.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ListCache {
    /// Seconds since the epoch.
    pub fetched_at: u64,
    /// `Servers.xml`, as fetched, when the list was one; empty when it was the hub's.
    #[serde(default)]
    pub list: String,
    /// The hub's pages, as fetched, when the list was the hub's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hub: Vec<String>,
    /// The addresses of the sources the list was read from, in the order they were tried.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
}

impl ListCache {
    /// Whether the copy is the hub's list.
    pub fn is_hubs(&self) -> bool {
        !self.hub.is_empty()
    }

    /// Whether the copy is less than a day old. A copy from the future (a clock set back) is not.
    pub fn is_fresh(&self, now: u64) -> bool {
        now >= self.fetched_at && now - self.fetched_at < MAX_AGE_SECS
    }

    /// The copy in `dir`, if there is a readable one.
    pub fn load(dir: &Path) -> Option<Self> {
        let bytes = std::fs::read(dir.join(CACHE_FILE)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Write the copy into `dir`, atomically.
    ///
    /// # Errors
    /// Any failure to create the folder or write the file.
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let json = serde_json::to_vec(self).map_err(io::Error::other)?;
        let tmp = dir.join(format!("{CACHE_FILE}.tmp"));
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, dir.join(CACHE_FILE))
    }

    /// The worlds the copy holds.
    ///
    /// # Errors
    /// [`MalformedList`] when the list cannot be read.
    pub fn worlds(&self) -> Result<Vec<World>, MalformedList> {
        if self.hub.is_empty() {
            parse_servers_xml(self.list.as_bytes())
        } else {
            parse_hub_pages(&self.hub)
        }
    }
}

// ----- reading the list --------------------------------------------------------------------------

/// Where the list is read from first: the Dereth Network hub's list, as Dereth's launcher reads it.
pub const HUB_LIST_URL: &str = "https://hub.dereth.network/v1/servers";

/// Where the list is read from when the hub does not answer with its list: the community's own.
pub const COMMUNITY_LIST_URL: &str =
    "https://raw.githubusercontent.com/acresources/serverslist/master/Servers.xml";

/// The most a list's answer is read to.
pub const MAX_LIST_BYTES: usize = 8 << 20;

/// How many worlds each page of the hub's list asks for: the most the hub gives on one.
const HUB_PAGE_SIZE: u64 = 100;

/// The most pages of the hub's list read, so that a list that never ends still ends.
const HUB_MAX_PAGES: usize = 50;

/// The address of the page of the hub's list that starts at `offset`: this game's worlds.
fn hub_page_url(offset: u64) -> String {
    format!("{HUB_LIST_URL}?game=dereth&limit={HUB_PAGE_SIZE}&offset={offset}")
}

/// Read the list with `fetch` (an address to its body, or why there is none): the hub's, a page
/// at a time, else the community's `Servers.xml`. A source whose answer is not its list, or shows
/// no world of this game, is passed over as no answer is.
///
/// # Errors
/// Why each source did not answer with its list, when neither did.
pub fn read_list(
    mut fetch: impl FnMut(&str) -> Result<Vec<u8>, String>,
    now: u64,
) -> Result<ListCache, String> {
    let hub_failed = match read_hub(&mut fetch) {
        Ok(hub) => {
            return Ok(ListCache {
                fetched_at: now,
                hub,
                sources: vec![HUB_LIST_URL.to_owned()],
                ..ListCache::default()
            })
        }
        Err(e) => e,
    };
    let community = fetch(COMMUNITY_LIST_URL).and_then(|body| {
        let worlds = parse_servers_xml(&body).map_err(|e| e.to_string())?;
        if worlds.is_empty() {
            return Err("the world list shows no world".to_owned());
        }
        String::from_utf8(body).map_err(|e| e.to_string())
    });
    match community {
        Ok(list) => Ok(ListCache {
            fetched_at: now,
            list,
            sources: vec![HUB_LIST_URL.to_owned(), COMMUNITY_LIST_URL.to_owned()],
            ..ListCache::default()
        }),
        Err(c) => Err(format!("{hub_failed}; {COMMUNITY_LIST_URL}: {c}")),
    }
}

/// The hub's pages, in order, when they are its list and show a world of this game.
fn read_hub(
    fetch: &mut impl FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<Vec<String>, String> {
    let mut pages = Vec::new();
    let mut offset = 0;
    for _ in 0..HUB_MAX_PAGES {
        let url = hub_page_url(offset);
        let text = fetch(&url)
            .and_then(|b| String::from_utf8(b).map_err(|e| e.to_string()))
            .map_err(|e| format!("{url}: {e}"))?;
        let (_, next) = hub_page(&text).map_err(|e| format!("{url}: {e}"))?;
        pages.push(text);
        match next {
            Some(n) if n > offset => offset = n,
            _ => break,
        }
    }
    let worlds = parse_hub_pages(&pages).map_err(|e| format!("{HUB_LIST_URL}: {e}"))?;
    if !worlds.iter().any(|w| w.game == Game::Dereth) {
        return Err(format!("{HUB_LIST_URL}: the world list shows no world"));
    }
    Ok(pages)
}

// ----- the small XML reader ------------------------------------------------------------------------

/// The text with every `<!-- ... -->` taken out.
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("<!--") {
        out.push_str(&rest[..at]);
        rest = rest[at..]
            .find("-->")
            .map_or("", |end| &rest[at + end + 3..]);
    }
    out.push_str(rest);
    out
}

/// Where the tag `name` opens at `at` in `text`: `<name>`, `<name ...>` or `<name/>`, and not a
/// longer name that begins the same way.
fn opens(text: &str, at: usize, name: &str) -> bool {
    text[at..].starts_with('<')
        && text[at + 1..].starts_with(name)
        && text[at + 1 + name.len()..]
            .chars()
            .next()
            .is_some_and(|c| c == '>' || c == '/' || c.is_whitespace())
}

/// The inner text of every `<name>...</name>` in `text`, in order. A self-closed `<name/>` is empty.
fn elements<'a>(text: &'a str, name: &str) -> Vec<&'a str> {
    let close = format!("</{name}>");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = text[from..].find('<') {
        let at = from + rel;
        if !opens(text, at, name) {
            from = at + 1;
            continue;
        }
        let Some(gt) = text[at..].find('>').map(|g| at + g) else {
            break;
        };
        if text[..gt].ends_with('/') {
            out.push("");
            from = gt + 1;
            continue;
        }
        let Some(end) = text[gt + 1..].find(&close).map(|e| gt + 1 + e) else {
            break;
        };
        out.push(&text[gt + 1..end]);
        from = end + close.len();
    }
    out
}

/// Every child element of an item, by name, with its text decoded. Children are text only in this
/// list, so a child's inner markup is not looked into.
fn children(item: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut from = 0;
    while let Some(rel) = item[from..].find('<') {
        let at = from + rel;
        let tag: String = item[at + 1..]
            .chars()
            .take_while(|c| !(c.is_whitespace() || *c == '>' || *c == '/'))
            .collect();
        if tag.is_empty() || tag.starts_with(['!', '?']) {
            from = at + 1;
            continue;
        }
        let Some(gt) = item[at..].find('>').map(|g| at + g) else {
            break;
        };
        if item[..gt].ends_with('/') {
            out.entry(tag).or_insert_with(String::new);
            from = gt + 1;
            continue;
        }
        let close = format!("</{tag}>");
        let Some(end) = item[gt + 1..].find(&close).map(|e| gt + 1 + e) else {
            break;
        };
        out.entry(tag).or_insert_with(|| decode(&item[gt + 1..end]));
        from = end + close.len();
    }
    out
}

/// Character data: CDATA sections as they are, and the five named entities and numeric character
/// references decoded. An entity that is none of these is left as written.
fn decode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("<![CDATA[") {
            let end = after.find("]]>").unwrap_or(after.len());
            out.push_str(&after[..end]);
            rest = after.get(end + 3..).unwrap_or("");
            continue;
        }
        let c = rest.chars().next().expect("not empty");
        if c == '&' {
            if let Some(semi) = rest.bytes().take(12).position(|b| b == b';') {
                let entity = &rest[1..semi];
                let decoded = match entity {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    _ => entity
                        .strip_prefix("#x")
                        .or_else(|| entity.strip_prefix("#X"))
                        .and_then(|h| u32::from_str_radix(h, 16).ok())
                        .or_else(|| entity.strip_prefix('#').and_then(|d| d.parse().ok()))
                        .and_then(char::from_u32),
                };
                if let Some(d) = decoded {
                    out.push(d);
                    rest = &rest[semi + 1..];
                    continue;
                }
            }
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::WorldState;

    /// Two rows as the list publishes them, one with an entity, one with its links left out.
    const LIST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<ArrayOfServerItem>
  <!-- <ServerItem><name>Commented out</name></ServerItem> -->
  <ServerItem>
    <id>9d17ce44-7db5-40c1-b7bb-a01c9de21d00</id>
    <name>AChard</name>
    <description>ACE EOR PVP Server &amp; PK rankings</description>
    <emu>ACE</emu>
    <server_host>a-chard.ddns.net</server_host>
    <server_port>9000</server_port>
    <type>PvP</type>
    <status>Stable</status>
    <website_url>http://a-chard.ddns.net/</website_url>
    <discord_url></discord_url>
  </ServerItem>
  <ServerItem>
    <id>b5bde7a5-537c-4665-8714-b7b097e73964</id>
    <name>Asheron4Fun.com</name>
    <description><![CDATA[Five <worlds>]]></description>
    <emu>GDL</emu>
    <server_host>play.a4f.example</server_host>
    <server_port>not a port</server_port>
    <type>PvE</type>
    <status>Development</status>
    <website_url/>
  </ServerItem>
  <ServerItem><description>no name</description></ServerItem>
</ArrayOfServerItem>"#;

    #[test]
    fn the_community_list_reads_as_worlds_with_slugs_from_their_names() {
        let w = parse_servers_xml(LIST.as_bytes()).unwrap();
        assert_eq!(
            w.len(),
            2,
            "the commented row and the nameless row are not worlds"
        );
        let a = &w[0];
        assert_eq!((a.slug.as_str(), a.name.as_str()), ("achard", "AChard"));
        assert_eq!(
            a.list_id.as_deref(),
            Some("9d17ce44-7db5-40c1-b7bb-a01c9de21d00")
        );
        assert_eq!(
            a.description.as_deref(),
            Some("ACE EOR PVP Server & PK rankings")
        );
        assert_eq!(a.emulator, Emulator::Ace);
        assert_eq!(a.ruleset.as_deref(), Some("PvP"));
        assert_eq!(a.development_status.as_deref(), Some("Stable"));
        assert_eq!(
            a.endpoint.as_ref().map(|e| (e.address.as_str(), e.port)),
            Some(("a-chard.ddns.net", 9000))
        );
        assert_eq!(a.links.website.as_deref(), Some("http://a-chard.ddns.net/"));
        assert_eq!(a.links.discord, None, "an empty link is no link");
        assert_eq!(a.era, None, "the list never names an era");

        let b = &w[1];
        assert_eq!(b.slug, "asheron4fun-com");
        assert_eq!(b.emulator, Emulator::Gdle, "the list spells GDLE as GDL");
        assert_eq!(b.description.as_deref(), Some("Five <worlds>"));
        assert_eq!(b.endpoint, None, "a port that is no number is no endpoint");
        assert_eq!(b.links.website, None);
    }

    #[test]
    fn a_listed_world_the_launcher_knows_is_read_with_its_logon_files_and_rules() {
        let xml = "<ArrayOfServerItem><ServerItem>\
                   <id>3f1f41ec-c7fd-4ed9-b47d-25b0d94219c1</id><name>Unfamiliar Shores</name>\
                   <emu>ACE</emu><server_host>192.0.2.7</server_host><server_port>9000</server_port>\
                   </ServerItem><ServerItem><id>394C58D0-885D-466B-B17F-D7E0B96FE3E2</id>\
                   <name>Seedsow</name><emu>GDL</emu></ServerItem></ArrayOfServerItem>";
        let w = parse_servers_xml(xml.as_bytes()).unwrap();
        assert_eq!(
            w[0].emulator,
            Emulator::ClassicAce,
            "the list says plain ACE"
        );
        assert_eq!(w[0].logon_version.as_deref(), Some("c118"));
        assert_eq!(w[0].world_profile.as_deref(), Some("classicace-customdm"));
        assert_eq!(w[0].era.as_deref(), Some("infiltration"));
        assert!(w[0].dats.custom.is_some());
        assert_eq!(w[1].world_profile.as_deref(), Some("classicdereth"));
        assert_eq!(w[1].logon_version.as_deref(), Some("1802"));
    }

    #[test]
    fn slugs_follow_the_name_and_two_alike_are_told_apart() {
        assert_eq!(slug_for("Buadren AC"), "buadren-ac");
        assert_eq!(slug_for("  -Frost--Fell!  "), "frost-fell");
        let xml = "<ArrayOfServerItem><ServerItem><name>Twin</name></ServerItem>\
                   <ServerItem><name>twin</name></ServerItem>\
                   <ServerItem><id>abc</id><name>!!!</name></ServerItem></ArrayOfServerItem>";
        let w = parse_servers_xml(xml.as_bytes()).unwrap();
        let slugs: Vec<_> = w.iter().map(|w| w.slug.as_str()).collect();
        assert_eq!(slugs, ["twin", "twin-2", "abc"]);
    }

    #[test]
    fn something_that_is_not_the_list_is_an_error() {
        assert!(parse_servers_xml(b"<html></html>").is_err());
        assert!(parse_servers_xml(&[0xff, 0xfe, 0x00]).is_err());
        assert_eq!(
            parse_servers_xml(b"<ArrayOfServerItem/>").unwrap(),
            Vec::<World>::new()
        );
    }

    /// A `Servers.xml` of one world, named `name`.
    fn list_of(name: &str) -> Vec<u8> {
        format!(
            "<ArrayOfServerItem><ServerItem><name>{name}</name></ServerItem></ArrayOfServerItem>"
        )
        .into_bytes()
    }

    /// A row of the hub's list as `/v1/servers` gives it: the world `name`, which the hub lists as
    /// `slug`, imported from no list.
    fn hub_row(slug: &str, name: &str) -> Value {
        serde_json::json!({
            "slug": slug, "name": name, "description": "", "ruleset": "PvE",
            "maturity": "Unspecified", "region": "Unspecified", "discord": null,
            "emulator": {"family": "ACE"}, "software": "ACE", "isDemo": false, "federation": "unset",
            "endpoint": {"mode": "manual", "address": format!("{slug}.example"), "port": 9000,
                         "transport": "udp", "addressFamily": null, "source": "operator"},
            "ip": null, "port": 9000, "provenance": null,
            "status": {"state": "unknown", "playersOnline": null, "lastSeenAt": null}
        })
    }

    /// A page of the hub's list holding `rows`, with the next page starting at `next`.
    fn hub_page_of(rows: &[Value], next: Option<u64>) -> Vec<u8> {
        serde_json::json!({"data": rows, "limit": 100, "offset": 0, "nextOffset": next,
                           "total": rows.len()})
        .to_string()
        .into_bytes()
    }

    fn names(worlds: &[World]) -> Vec<&str> {
        worlds.iter().map(|w| w.name.as_str()).collect()
    }

    /// Each world of the hub's list carries the game its row names, a row that names none runs
    /// `dereth`, and a row of a game this launcher does not know is not shown; the community
    /// list's worlds are all `dereth`'s.
    #[test]
    fn a_hub_rows_game_is_the_worlds_and_one_of_an_unknown_game_is_not_shown() {
        let row = |slug: &str, game: Option<Value>| {
            let mut r = hub_row(slug, &slug.to_uppercase());
            if let Some(g) = game {
                r["game"] = g;
            }
            r
        };
        let page = String::from_utf8(hub_page_of(
            &[
                row("coldeve", None),
                row("frost", Some(Value::Null)),
                row("darktide", Some(serde_json::json!("dereth"))),
                row("shardfall", Some(serde_json::json!("auberean"))),
                row("later", Some(serde_json::json!("a-later-game"))),
                row("odd", Some(serde_json::json!(2))),
            ],
            None,
        ))
        .expect("text");
        let games: Vec<(String, Game)> = parse_hub_pages(&[page])
            .expect("read")
            .into_iter()
            .map(|w| (w.hub_slug.unwrap_or_default(), w.game))
            .collect();
        assert_eq!(
            games,
            [
                ("coldeve".to_owned(), Game::Dereth),
                ("frost".to_owned(), Game::Dereth),
                ("darktide".to_owned(), Game::Dereth),
                ("shardfall".to_owned(), Game::Auberean),
            ]
        );
        let community = parse_servers_xml(&list_of("Community World")).expect("read");
        assert!(community.iter().all(|w| w.game == Game::Dereth));
    }

    /// The same three worlds as the community list and the hub give them. The hub lists Unfamiliar
    /// Shores under a slug of its own, keeps Seedsow's id in lower case, and calls a type it does
    /// not know `Custom` beside the community list's own word.
    #[test]
    fn a_world_from_the_hub_is_kept_and_read_as_the_same_world_from_the_community_list() {
        let xml = r"<ArrayOfServerItem>
  <ServerItem><id>3f1f41ec-c7fd-4ed9-b47d-25b0d94219c1</id><name>Unfamiliar Shores</name>
    <description>Infiltration, as it was</description><emu>ACE</emu>
    <server_host>192.0.2.7</server_host><server_port>9000</server_port><type>PvE RP</type>
    <status>Stable</status><website_url>https://shores.example/</website_url>
    <discord_url>https://discord.gg/shores</discord_url></ServerItem>
  <ServerItem><id>394C58D0-885D-466B-B17F-D7E0B96FE3E2</id><name>Seedsow</name>
    <description>Classic Dereth</description><emu>GDL</emu>
    <server_host>seedsow.example</server_host><server_port>9060</server_port><type>PvE</type>
    <status>Stable</status></ServerItem>
  <ServerItem><id>9d17ce44-7db5-40c1-b7bb-a01c9de21d00</id><name>AChard</name>
    <description>PK rankings</description><emu>ACE</emu>
    <server_host>a-chard.example</server_host><server_port>9000</server_port><type>PvP</type>
    <status>Development</status></ServerItem>
</ArrayOfServerItem>";
        let imported = |slug: &str, name: &str, id: &str, ty: &str, ruleset: &str| {
            let mut row = hub_row(slug, name);
            row["ruleset"] = ruleset.into();
            row["provenance"] = serde_json::json!({"source": "community-list", "externalId": id,
                "legacyType": ty, "url": "https://github.com/acresources/serverslist"});
            row["endpoint"]["source"] = "community-list".into();
            row
        };
        let mut shores = imported(
            "ushores",
            "Unfamiliar Shores",
            "3f1f41ec-c7fd-4ed9-b47d-25b0d94219c1",
            "PvE RP",
            "Custom",
        );
        shores["description"] = "Infiltration, as it was".into();
        shores["maturity"] = "Stable".into();
        shores["website"] = "https://shores.example/".into();
        shores["discord"] = "https://discord.gg/shores".into();
        shores["endpoint"]["address"] = "192.0.2.7".into();
        let mut seedsow = imported(
            "seedsow",
            "Seedsow",
            "394c58d0-885d-466b-b17f-d7e0b96fe3e2",
            "PvE",
            "PvE",
        );
        seedsow["description"] = "Classic Dereth".into();
        seedsow["maturity"] = "Stable".into();
        seedsow["emulator"]["family"] = "GDL".into();
        seedsow["endpoint"]["port"] = 9060.into();
        let mut achard = imported(
            "achard",
            "AChard",
            "9d17ce44-7db5-40c1-b7bb-a01c9de21d00",
            "PvP",
            "PvP",
        );
        achard["description"] = "PK rankings".into();
        achard["maturity"] = "Development".into();
        achard["endpoint"]["address"] = "a-chard.example".into();
        let page = String::from_utf8(hub_page_of(&[shores, seedsow, achard], None)).unwrap();

        let from_list = parse_servers_xml(xml.as_bytes()).unwrap();
        let from_hub = parse_hub_pages(&[page]).unwrap();
        let slugs = |w: &[World]| w.iter().map(|w| w.slug.clone()).collect::<Vec<_>>();
        assert_eq!(slugs(&from_hub), ["unfamiliar-shores", "seedsow", "achard"]);
        assert_eq!(
            slugs(&from_hub),
            slugs(&from_list),
            "one key from either list"
        );
        // The table of worlds that run a client of their own knows both, by either spelling.
        assert_eq!(from_hub[0].logon_version.as_deref(), Some("c118"));
        assert_eq!(from_hub[1].world_profile.as_deref(), Some("classicdereth"));
        // The hub's own slug is kept, for its world alone.
        assert_eq!(
            from_hub
                .iter()
                .map(|w| w.hub_slug.as_deref())
                .collect::<Vec<_>>(),
            [Some("ushores"), Some("seedsow"), Some("achard")]
        );
        assert!(from_list.iter().all(|w| w.hub_slug.is_none()));
        // Ids compare without regard to case; everything else is the same world.
        let same = |w: &[World]| {
            w.iter()
                .cloned()
                .map(|mut w| {
                    w.list_id = w.list_id.map(|id| id.to_ascii_lowercase());
                    w.hub_slug = None;
                    w
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(same(&from_hub), same(&from_list));
    }

    #[test]
    fn a_hub_demonstration_world_is_shown_as_a_test_world_and_one_with_no_address_is_not() {
        let mut demo = hub_row("demo", "Demonstration");
        demo["isDemo"] = true.into();
        demo["maturity"] = "Stable".into();
        let mut unseen = hub_row("unseen", "Not Yet Seen");
        unseen["endpoint"] =
            serde_json::json!({"mode": "observed", "address": null, "port": 9000, "source": null});
        let mut observed = hub_row("seen", "Seen World");
        observed["endpoint"] = serde_json::json!({"mode": "observed", "address": "203.0.113.5",
            "port": 9001, "addressFamily": "ipv4", "source": "heartbeat-source"});
        let mut no_port = hub_row("noport", "No Port");
        no_port["endpoint"]["port"] = Value::Null;
        let nameless = serde_json::json!({"slug": "nameless",
            "endpoint": {"address": "n.example", "port": 9000}});
        let mut busy = hub_row("busy", "Busy World");
        busy["emulator"] = serde_json::json!({"family": "Empyrean", "version": "0.4.1"});
        busy["status"] = serde_json::json!({"state": "online", "playersOnline": 12});
        busy["website"] = "https://busy.example".into();
        let first = String::from_utf8(hub_page_of(
            &[demo, unseen, observed, no_port, nameless, busy.clone()],
            Some(100),
        ))
        .unwrap();
        // The world moved down a page while the pages were read.
        let second = String::from_utf8(hub_page_of(&[busy], None)).unwrap();

        let w = parse_hub_pages(&[first, second]).unwrap();
        assert_eq!(names(&w), ["Demonstration", "Seen World", "Busy World"]);
        assert_eq!(
            w[0].development_status.as_deref(),
            Some("Test"),
            "a test world says so where its status is shown, whatever its maturity"
        );
        assert_eq!(w[0].slug, "demonstration");
        let w = &w[1..];
        assert_eq!(
            w[0].endpoint.as_ref().map(|e| (e.address.as_str(), e.port)),
            Some(("203.0.113.5", 9001)),
            "the address the hub saw the world's heartbeats come from is its address"
        );
        let b = &w[1];
        assert_eq!(b.slug, "busy-world");
        assert_eq!(
            b.list_id, None,
            "a world registered at the hub has no list id"
        );
        assert_eq!(
            (b.emulator, b.emulator_version.as_deref()),
            (Emulator::Empyrean, Some("0.4.1"))
        );
        assert_eq!(b.development_status, None, "Unspecified says nothing");
        assert_eq!(b.ruleset.as_deref(), Some("PvE"));
        assert_eq!(b.links.website.as_deref(), Some("https://busy.example"));
        assert_eq!(b.description, None);
        assert_eq!(
            (b.state, b.players),
            (WorldState::Unknown, None),
            "whether it is up is asked of the world, not read from a copy up to a day old"
        );
    }

    #[test]
    fn decoding_handles_entities_references_and_leaves_strays_alone() {
        assert_eq!(
            decode("a &lt;b&gt; &quot;c&quot; &apos;d&apos;"),
            "a <b> \"c\" 'd'"
        );
        assert_eq!(decode("&#65;&#x42;"), "AB");
        assert_eq!(decode("fish & chips &bogus;"), "fish & chips &bogus;");
    }

    #[test]
    fn an_entity_before_accented_text_decodes_without_a_panic() {
        assert_eq!(decode("&amp; Bistrô"), "& Bistrô");
        assert_eq!(decode("&aaaaaaaaaaé"), "&aaaaaaaaaaé");
    }

    /// Read the list with stand-in answers, one per address; an address without one does not
    /// answer. Also the addresses asked, in order.
    fn read_with(answers: &[(&str, Vec<u8>)]) -> (Result<ListCache, String>, Vec<String>) {
        let mut asked = Vec::new();
        let read = read_list(
            |url| {
                asked.push(url.to_owned());
                answers
                    .iter()
                    .find(|(u, _)| *u == url)
                    .map(|(_, b)| b.clone())
                    .ok_or_else(|| "no answer".to_owned())
            },
            7,
        );
        (read, asked)
    }

    #[test]
    fn the_hubs_list_is_read_a_page_at_a_time_and_alone_when_it_answers() {
        let first = hub_page_url(0);
        let second = hub_page_url(100);
        let (read, asked) = read_with(&[
            (&first, hub_page_of(&[hub_row("a", "Alpha")], Some(100))),
            (&second, hub_page_of(&[hub_row("b", "Beta")], None)),
            (COMMUNITY_LIST_URL, list_of("Community World")),
        ]);
        let cache = read.unwrap();
        assert_eq!(
            asked,
            vec![first, second],
            "the community list is not asked"
        );
        assert_eq!(cache.fetched_at, 7);
        assert!(cache.is_hubs());
        assert_eq!(names(&cache.worlds().unwrap()), ["Alpha", "Beta"]);
    }

    #[test]
    fn a_hub_that_does_not_answer_with_its_list_falls_back_to_the_community_list() {
        let first = hub_page_url(0);
        for hub in [
            None,
            Some(b"<html>down for maintenance</html>".to_vec()),
            Some(hub_page_of(&[], None)),
        ] {
            let mut answers = vec![(COMMUNITY_LIST_URL, list_of("Community World"))];
            if let Some(body) = &hub {
                answers.push((first.as_str(), body.clone()));
            }
            let cache = read_with(&answers).0.unwrap();
            assert!(!cache.is_hubs());
            assert_eq!(names(&cache.worlds().unwrap()), ["Community World"]);
        }
    }

    #[test]
    fn when_neither_list_answers_the_failure_names_both() {
        let (read, _) = read_with(&[(COMMUNITY_LIST_URL, b"<ArrayOfServerItem/>".to_vec())]);
        let e = read.unwrap_err();
        assert!(
            e.contains(HUB_LIST_URL) && e.contains(COMMUNITY_LIST_URL),
            "{e}"
        );
    }

    #[test]
    fn the_copy_is_used_for_a_day_and_round_trips() {
        let dir = std::env::temp_dir().join(format!("pyreal-listcache-{}", std::process::id()));
        let cache = read_with(&[(COMMUNITY_LIST_URL, list_of("Kept"))])
            .0
            .unwrap();
        cache.save(&dir).unwrap();
        let back = ListCache::load(&dir).unwrap();
        assert_eq!(back, cache);
        assert!(back.is_fresh(7 + MAX_AGE_SECS - 1));
        assert!(!back.is_fresh(7 + MAX_AGE_SECS));
        assert!(!back.is_fresh(6), "a copy from the future is not fresh");
        assert!(ListCache::load(&dir.join("nowhere")).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
