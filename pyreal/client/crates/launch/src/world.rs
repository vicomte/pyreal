//! A world: one registry entry, and everything the launcher needs to know to put a player in it.
//!
//! Every field past the address is optional, and an absent one means **"we have not been told"**,
//! never "no".

use serde::{Deserialize, Serialize};

use crate::datset::Iterations;

/// The game a world runs, as the hub's list names it: Asheron's Call (`dereth`) or Asheron's Call 2
/// (`auberean`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Game {
    /// Every world of the community list, and every hub row that names no game.
    #[default]
    Dereth,
    Auberean,
}

impl Game {
    /// The game's word, as the hub writes it.
    pub fn word(self) -> &'static str {
        match self {
            Game::Dereth => "dereth",
            Game::Auberean => "auberean",
        }
    }

    /// The game a word names, exactly as the hub writes it; `None` for one this launcher does not
    /// know.
    pub fn parse(word: &str) -> Option<Self> {
        [Game::Dereth, Game::Auberean]
            .into_iter()
            .find(|g| g.word() == word)
    }
}

/// The server software a world runs. It decides the command-line form a retail client needs and
/// which status source is worth asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Emulator {
    Empyrean,
    Ace,
    /// ClassicACE, a fork of ACE.
    ClassicAce,
    Gdle,
    /// Yalain, the Asheron's Call 2 server.
    Yalain,
    /// Not said, or software the launcher does not know by name, which it treats alike. A record
    /// saved as `other` reads as this.
    #[default]
    #[serde(alias = "other")]
    Unknown,
}

impl Emulator {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "empyrean" => Emulator::Empyrean,
            "ace" => Emulator::Ace,
            "classicace" | "classic ace" | "classic-ace" | "classic_ace" => Emulator::ClassicAce,
            // The community list spells it `GDL`.
            "gdle" | "gdl" => Emulator::Gdle,
            "yalain" => Emulator::Yalain,
            _ => Emulator::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Emulator::Empyrean => "Empyrean",
            Emulator::Ace => "ACE",
            Emulator::ClassicAce => "ClassicACE",
            Emulator::Gdle => "GDLE",
            Emulator::Yalain => "Yalain",
            Emulator::Unknown => "Unknown",
        }
    }
}

/// Whether a world is up, as last reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldState {
    Online,
    /// Up, but at or near its population ceiling.
    High,
    Offline,
    /// Starting up: reachable, not yet letting anyone in.
    Starting,
    /// Nobody has said. Most of the directory looks like this, and it is still worth trying.
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    pub address: String,
    pub port: u16,
    pub transport: Option<String>,
}

/// How a world creates accounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountModel {
    /// The first login under a new name creates the account. ACE's default, and the reason the
    /// launcher warns before a first sign-in: a typo becomes a new account, silently.
    AutoCreateOnFirstLogin,
    WebSignup,
    Invite,
    #[default]
    Unknown,
}

/// A world's own data files, for a world that ships them. Always a link: the launcher never hosts
/// or fetches-and-runs them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomDats {
    pub url: String,
    pub sha256: Option<String>,
    pub size: Option<u64>,
    pub iterations: Iterations,
    pub license_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WorldDats {
    /// What the world's own dats report. `None` means the world has not said.
    pub expected: Option<Iterations>,
    pub patches_over_wire: Option<bool>,
    pub custom: Option<CustomDats>,
    pub highres_required: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Links {
    pub website: Option<String>,
    pub discord: Option<String>,
    pub rules: Option<String>,
    pub guide: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Operator {
    pub name: Option<String>,
    pub contact: Option<String>,
    pub key_id: Option<String>,
}

/// Who said a world's era, or the systems it has. A world nobody said them for plays the end of
/// retail, with its systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Told {
    /// The world: its status ping or document, its list row, or the launcher's table of worlds
    /// that run a client of their own, which speaks for the world.
    World,
}

/// How sure a world's shown era is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EraState {
    /// Said: by its row, by the world (its status ping or document, or what it told the hub), or
    /// by its software, which plays only the end of retail.
    #[default]
    Known,
    /// An Empyrean world, which may play any era, not heard from yet while an answer may still
    /// come.
    Checking,
    /// An Empyrean world that nothing will answer for: it did not answer when asked, or the hub
    /// has no word from it.
    Unknown,
    /// A world whose software nobody names, shown as the end of retail, which it most likely plays.
    Assumed,
}

/// One world.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct World {
    /// Stable identity. Every per-world record the launcher keeps is keyed by this.
    pub slug: String,
    pub name: String,
    /// The game the world runs, as the hub's list names it. Every other world, and a hub row that
    /// names none, runs `dereth`.
    #[serde(default)]
    pub game: Game,
    pub description: Option<String>,
    pub ruleset: Option<String>,
    /// The era the world plays (`"eor"`, the end of retail, or an earlier one such as
    /// `"infiltration"`), from the registry row or, when it answers, the world's status document.
    /// `None` when neither says: the world plays the end of retail.
    #[serde(default)]
    pub era: Option<String>,
    /// The systems the world has (`ratings=false,trade=true,...`), from its status document when
    /// it answers; each one over the era's table. `None` when it does not say: it has the era's
    /// whole table.
    #[serde(default)]
    pub era_features: Option<String>,
    /// Who named [`World::era`]; `None` when nobody has.
    #[serde(default)]
    pub era_source: Option<Told>,
    /// Who named [`World::era_features`]; `None` when nobody has.
    #[serde(default)]
    pub features_source: Option<Told>,
    /// How sure the shown era is ([`EraState`]).
    #[serde(default)]
    pub era_state: EraState,
    /// Whether the player starred the world in the old launcher's world list.
    #[serde(default)]
    pub favorite: bool,
    pub emulator: Emulator,
    /// The server software's version, when the world says it (Empyrean's status document, or what
    /// a server reported to the directory).
    #[serde(default)]
    pub emulator_version: Option<String>,
    /// The community list's id for the world, which never changes.
    #[serde(default)]
    pub list_id: Option<String>,
    /// The world's slug on the Dereth Network's hub, for a world the hub's list names; `None` for
    /// one from any other list, or added by hand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hub_slug: Option<String>,
    /// How settled the world says it is: `Stable`, `Development` or `Experimental`; `Test` for a
    /// world the hub lists for demonstration.
    #[serde(default)]
    pub development_status: Option<String>,
    pub endpoint: Option<Endpoint>,
    /// Client ids the world accepts. Empty means "the end-of-retail wire protocol".
    pub accepted_clients: Vec<String>,
    pub preferred_client: Option<String>,
    pub dats: WorldDats,
    pub account_model: AccountModel,
    pub signup_url: Option<String>,
    pub reset_url: Option<String>,
    pub status_url: Option<String>,
    pub state: WorldState,
    pub players: Option<u32>,
    /// How old the count of players is, in seconds, when the hub's list gave it; `None` for a
    /// world's own answer just now, or no count.
    #[serde(default)]
    pub players_age: Option<u64>,
    /// Where the hub has the count from: `ping`, `heartbeat`, `client` or `treestats`.
    #[serde(default)]
    pub players_source: Option<String>,
    pub links: Links,
    pub operator: Option<Operator>,
    pub updated_at: Option<String>,
    /// The row's schema. Rows from before schema 2 say nothing and read as 1.
    pub schema: u32,
    /// The logon version string the world's server accepts, when it is not the end of retail's
    /// `"1802"` or is known to be it ([`crate::known`]). `None`: not said.
    #[serde(default)]
    pub logon_version: Option<String>,
    /// The client rules the world plays by, as `--world-profile` names them. `None`: the end of
    /// retail's.
    #[serde(default)]
    pub world_profile: Option<String>,
    /// The data set the world is drawn from (`modern` or `classic`), when it is not the one its
    /// era suggests. `None`: the era's.
    #[serde(default)]
    pub world_base: Option<String>,
    /// The world's plugin policy (`plugins.policy`, or `plugin_policy`): the policy document
    /// itself, or the `https://` URL it is published at. Absent: the world publishes none, and the
    /// Dereth client applies its defaults.
    pub plugin_policy: Option<String>,
    /// The wire its server speaks, as `--wire` names it: [`WIRE_1999`], or [`WIRE_LATER`] once a
    /// probe found the later one. `None`: not asked yet, played as the later wire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wire: Option<String>,
}

/// [`World::wire`] for a server of the November 1999 client's wire.
pub const WIRE_1999: &str = "1999";
/// [`World::wire`] for a server of the later wire.
pub const WIRE_LATER: &str = "later";

impl World {
    /// Whether the world's plugin policy is published at a URL (rather than written in the entry).
    #[must_use]
    pub fn plugin_policy_url(&self) -> Option<&str> {
        self.plugin_policy
            .as_deref()
            .map(str::trim)
            .filter(|p| p.starts_with("https://") && !p.contains(char::is_whitespace))
    }

    /// A bare world, for tests and for `servers.txt` rows.
    pub fn new(slug: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            slug: slug.into(),
            name: name.into(),
            schema: 1,
            ..Self::default()
        }
    }

    /// Whether the world may answer the status ping: Empyrean's, or software nobody named, and
    /// not a 1999 world.
    pub fn may_answer_status_ping(&self) -> bool {
        matches!(self.emulator, Emulator::Empyrean | Emulator::Unknown) && !self.speaks_1999()
    }

    /// Whether its server speaks the November 1999 client's wire.
    #[must_use]
    pub fn speaks_1999(&self) -> bool {
        self.wire.as_deref() == Some(WIRE_1999)
    }

    /// Take the era and systems the world's status ping names, where nothing has named them.
    pub fn take_status_ping(&mut self, era: &str, systems: &pyreal_primitives::EraFeatureBits) {
        if self.era.is_none() && !era.trim().is_empty() {
            self.era = Some(era.trim().to_owned());
            self.era_source = Some(Told::World);
        }
        let named = systems.overrides();
        if self.era_features.is_none() && !named.is_empty() {
            self.era_features = Some(named.to_string());
            self.features_source = Some(Told::World);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emulator_names_read_and_save() {
        assert_eq!(Emulator::parse("ClassicACE"), Emulator::ClassicAce);
        assert_eq!(Emulator::ClassicAce.label(), "ClassicACE");
        assert_eq!(Emulator::parse("GDL"), Emulator::Gdle);
        assert_eq!(Emulator::parse("Yalain"), Emulator::Yalain);
        assert_eq!(
            serde_json::to_string(&Emulator::Yalain).unwrap(),
            "\"yalain\""
        );
        assert_eq!(
            Emulator::parse("Thwarg"),
            Emulator::Unknown,
            "a name not known is unknown"
        );
        let saved: Emulator = serde_json::from_str("\"other\"").unwrap();
        assert_eq!(
            saved,
            Emulator::Unknown,
            "a record saved as other reads as unknown"
        );
    }

    #[test]
    fn a_status_ping_names_the_era_and_systems_only_where_nothing_has() {
        let bits =
            pyreal_primitives::EraFeatureBits::of(pyreal_primitives::EraFeatures::INFILTRATION);
        let mut w = World::new("custom-2", "Local 2005");
        assert!(w.may_answer_status_ping());
        w.take_status_ping("infiltration", &bits);
        assert_eq!(w.era.as_deref(), Some("infiltration"));
        assert_eq!(w.era_source, Some(Told::World));
        let systems = w.era_features.clone().expect("the systems");
        assert!(systems.contains("ratings=false"), "{systems}");
        let mut named = World::new("a", "a");
        named.era = Some("eor".into());
        named.take_status_ping("infiltration", &bits);
        assert_eq!(named.era.as_deref(), Some("eor"), "the row's era stands");
        named.emulator = Emulator::Ace;
        assert!(!named.may_answer_status_ping());
    }
}
