//! The worlds whose servers want more than the end-of-retail client, which the launcher knows by
//! their community-list id, and the worlds Pyreal lists itself ([`BUILT_IN_WORLDS`]), known by
//! address.
//!
//! A few worlds run their own client: its own logon version, data files of their own over the
//! end-of-retail ones, and a few rules of their own. Their servers say none of it, and the
//! community list has no place for it, so the launcher keeps it here, keyed by the list's id
//! ([`World::list_id`]), which never changes. The player points the launcher at the world's files
//! (from the world's own download page, which [`KnownWorld::files_note`] names); the launcher
//! downloads nothing.

use pyreal_primitives::{EraFeatureOverrides, EraFeatures, EraId};

use crate::datset::Iterations;
use crate::world::{CustomDats, Emulator, Endpoint, Told, World, WIRE_1999};

/// One world the launcher knows more about than its list row says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownWorld {
    /// The community list's id, compared without regard to case.
    pub list_id: &'static str,
    /// The name the list gives it, for reading this table.
    pub name: &'static str,
    /// The server software, which the list may name as plain ACE.
    pub emulator: Emulator,
    /// The logon version string its server accepts.
    pub logon_version: &'static str,
    /// Whether that string is confirmed (the server's own source, or its client) or assumed.
    pub logon_confirmed: bool,
    /// The era its rules are.
    pub era: EraId,
    /// The systems it has. Its server does not announce them.
    pub features: EraFeatures,
    /// The data set its world is drawn from: `modern`, the later files, for every world here,
    /// whose own files are the later files with the world's changes over them.
    pub world_base: &'static str,
    /// What its own files report, file by file.
    pub expected: Iterations,
    /// The client rules it plays by (`--world-profile`).
    pub profile: &'static str,
    /// Where its own files come from, for the player.
    pub files_url: &'static str,
    pub files_note: &'static str,
}

/// The two ClassicACE CustomDM worlds' files.
const CUSTOMDM_FILES: Iterations = Iterations {
    portal: Some(20044),
    cell: Some(20011),
    local: Some(20011),
    highres: Some(497),
};

/// ClassicDereth's files: the end-of-retail portal count, and its own cell. Its servers compare
/// portal and cell only.
const CLASSICDERETH_FILES: Iterations = Iterations {
    portal: Some(2072),
    cell: Some(4),
    local: None,
    highres: None,
};

/// The systems a CustomDM world has: Infiltration's, with cloaks and trinkets, which its server
/// equips.
const CUSTOMDM_FEATURES: EraFeatures = EraFeatures {
    cloaks: true,
    trinkets: true,
    ..EraFeatures::INFILTRATION
};

/// Every world the launcher knows this way.
pub const KNOWN_WORLDS: &[KnownWorld] = &[
    KnownWorld {
        list_id: "4db379b0-587a-4775-9b7f-879e0f15ba4b",
        name: "Dekarutide",
        emulator: Emulator::ClassicAce,
        logon_version: "c118",
        logon_confirmed: true,
        era: EraId::Infiltration,
        features: CUSTOMDM_FEATURES,
        world_base: "modern",
        expected: CUSTOMDM_FILES,
        profile: "classicace-customdm",
        files_url: "https://www.dekarutide.com/",
        files_note: "Dekarutide plays with its own data files, the ClassicACE CustomDM set. Get \
                     them from Dekarutide's own download page, unpack them into a folder of their \
                     own, and choose that folder here.",
    },
    KnownWorld {
        list_id: "3f1f41ec-c7fd-4ed9-b47d-25b0d94219c1",
        name: "Unfamiliar Shores",
        emulator: Emulator::ClassicAce,
        logon_version: "c118",
        logon_confirmed: true,
        era: EraId::Infiltration,
        features: CUSTOMDM_FEATURES,
        world_base: "modern",
        expected: CUSTOMDM_FILES,
        profile: "classicace-customdm",
        files_url: "https://github.com/bDekaru/ClassicACE#required-data-files",
        files_note: "Unfamiliar Shores plays with the ClassicACE CustomDM data files. Get them \
                     from the CustomDM download the ClassicACE project's page links, unpack them \
                     into a folder of their own, and choose that folder here.",
    },
    KnownWorld {
        list_id: "394c58d0-885d-466b-b17f-d7e0b96fe3e2",
        name: "Seedsow",
        emulator: Emulator::Gdle,
        logon_version: "1802",
        logon_confirmed: true,
        era: EraId::Infiltration,
        features: EraFeatures::INFILTRATION,
        world_base: "modern",
        expected: CLASSICDERETH_FILES,
        profile: "classicdereth",
        files_url: "https://github.com/bDekaru/ClassicDereth",
        files_note: "Seedsow plays with the ClassicDereth portal and cell files. Get them from \
                     the download the ClassicDereth project's page links (or Seedsow's own \
                     setup post), put them beside a copy of the end-of-retail files in a folder \
                     of their own, and choose that folder here.",
    },
    KnownWorld {
        list_id: "eea962c0-cf9f-481e-9736-0eb058acc1d8",
        name: "Snowreap",
        emulator: Emulator::Gdle,
        logon_version: "1802",
        logon_confirmed: true,
        era: EraId::Infiltration,
        features: EraFeatures::INFILTRATION,
        world_base: "modern",
        expected: CLASSICDERETH_FILES,
        profile: "classicdereth",
        files_url: "https://github.com/bDekaru/ClassicDereth",
        files_note: "Snowreap plays with the ClassicDereth portal and cell files. Get them from \
                     the download the ClassicDereth project's page links (or Seedsow's own \
                     setup post), put them beside a copy of the end-of-retail files in a folder \
                     of their own, and choose that folder here.",
    },
];

/// The known world a list id names.
#[must_use]
pub fn known(list_id: &str) -> Option<&'static KnownWorld> {
    let id = list_id.trim();
    KNOWN_WORLDS
        .iter()
        .find(|k| k.list_id.eq_ignore_ascii_case(id))
}

/// A world no list Pyreal reads carries, which Pyreal lists itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltInWorld {
    pub slug: &'static str,
    pub name: &'static str,
    pub host: &'static str,
    pub port: u16,
    pub emulator: Emulator,
    pub era: EraId,
    /// [`World::wire`].
    pub wire: &'static str,
    pub description: &'static str,
}

/// Every world Pyreal lists itself.
pub const BUILT_IN_WORLDS: &[BuiltInWorld] = &[BuiltInWorld {
    slug: "ac1999",
    name: "AC1999",
    host: "ac1999.cobradragon.com",
    port: 9110,
    // A GDLE fork.
    emulator: Emulator::Gdle,
    era: EraId::Launch,
    wire: WIRE_1999,
    description: "The AC1999 project's world, which plays the game as it was in November 1999 \
                  and speaks that client's protocol.",
}];

impl BuiltInWorld {
    /// The world record, as a list row would give it.
    #[must_use]
    pub fn world(&self) -> World {
        let mut w = World::new(self.slug, self.name);
        w.description = Some(self.description.to_owned());
        w.endpoint = Some(Endpoint {
            address: self.host.to_owned(),
            port: self.port,
            transport: None,
        });
        apply(&mut w);
        w
    }
}

/// The built-in world at `host` and `port`, the host compared without regard to case.
#[must_use]
pub fn built_in(host: &str, port: u16) -> Option<&'static BuiltInWorld> {
    let host = host.trim();
    BUILT_IN_WORLDS
        .iter()
        .find(|b| b.port == port && b.host.eq_ignore_ascii_case(host))
}

/// Fill in what the tables know about `w`, if they know `w`, and what its wire implies. The tables
/// speak for the world: its era and systems are the world's, not a choice the player is asked to
/// make. A 1999 world plays Launch, with the 1999 files.
pub fn apply(w: &mut World) {
    if let Some(b) = w
        .endpoint
        .as_ref()
        .and_then(|e| built_in(&e.address, e.port))
    {
        w.emulator = b.emulator;
        w.wire = Some(b.wire.to_owned());
        w.era = Some(b.era.name().to_owned());
        w.era_source = Some(Told::World);
    }
    if w.speaks_1999() && w.era.is_none() {
        w.era = Some(EraId::Launch.name().to_owned());
        w.era_source = Some(Told::World);
    }
    let Some(k) = w.list_id.as_deref().and_then(known) else {
        return;
    };
    w.emulator = k.emulator;
    w.era = Some(k.era.name().to_owned());
    w.era_source = Some(Told::World);
    w.era_features = Some(EraFeatureOverrides::all_of(k.features).to_string());
    w.features_source = Some(Told::World);
    w.world_base = Some(k.world_base.to_owned());
    w.logon_version = Some(k.logon_version.to_owned());
    w.world_profile = Some(k.profile.to_owned());
    w.dats.expected = Some(k.expected);
    w.dats.custom = Some(CustomDats {
        url: k.files_url.to_owned(),
        sha256: None,
        size: None,
        iterations: k.expected,
        license_note: Some(k.files_note.to_owned()),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_known_world_has_a_profile_the_client_compiles_and_a_logon_it_can_send() {
        for k in KNOWN_WORLDS {
            assert!(
                pyreal_rules::world::profile(k.profile).is_some(),
                "{}: {}",
                k.name,
                k.profile
            );
            assert!(!k.logon_version.is_empty() && k.logon_version.is_ascii());
            assert!(matches!(k.world_base, "modern" | "classic"), "{}", k.name);
            assert_ne!(
                k.expected,
                Iterations::END_OF_RETAIL,
                "{} has its own files",
                k.name
            );
            assert!(k.files_note.contains(k.name), "{}", k.name);
            assert_eq!(known(&k.list_id.to_ascii_uppercase()), Some(k), "any case");
        }
        let ids: std::collections::HashSet<_> = KNOWN_WORLDS
            .iter()
            .map(|k| k.list_id.to_ascii_lowercase())
            .collect();
        assert_eq!(ids.len(), KNOWN_WORLDS.len(), "one entry per world");
    }

    #[test]
    fn the_table_holds_the_four_worlds_with_their_servers_logon_files_and_rules() {
        let row = |name: &str| *KNOWN_WORLDS.iter().find(|k| k.name == name).unwrap();
        let d = row("Dekarutide");
        assert_eq!(
            (d.logon_version, d.profile, d.emulator),
            ("c118", "classicace-customdm", Emulator::ClassicAce)
        );
        assert_eq!(d.expected.label(), "20044/20011/20011/497");
        assert_eq!(
            row("Unfamiliar Shores").list_id,
            "3f1f41ec-c7fd-4ed9-b47d-25b0d94219c1"
        );
        assert_eq!(row("Unfamiliar Shores").expected, d.expected);
        for name in ["Seedsow", "Snowreap"] {
            let g = row(name);
            assert_eq!(
                (g.logon_version, g.profile, g.emulator),
                ("1802", "classicdereth", Emulator::Gdle)
            );
            assert!(
                g.logon_confirmed,
                "the server answered a 1802 logon with an account refusal, not a version one"
            );
            assert_eq!(g.expected.label(), "2072/4/-/-");
        }
        for k in KNOWN_WORLDS {
            assert_eq!((k.era, k.world_base), (EraId::Infiltration, "modern"));
        }
        assert!(d.features.trinkets && d.features.cloaks && !d.features.ratings);
    }

    #[test]
    fn ac1999_is_built_in_as_a_launch_world_of_the_1999_wire() {
        let w = built_in("AC1999.cobradragon.com", 9110)
            .expect("by address")
            .world();
        assert_eq!((w.slug.as_str(), w.name.as_str()), ("ac1999", "AC1999"));
        let ep = w.endpoint.as_ref().unwrap();
        assert_eq!(
            (ep.address.as_str(), ep.port),
            ("ac1999.cobradragon.com", 9110)
        );
        assert!(w.speaks_1999());
        assert_eq!(w.era.as_deref(), Some("launch"));
        assert_eq!(
            crate::choices::required_set_for(&w),
            crate::SetKind::Original,
            "the 1999 files"
        );
        assert!(!w.may_answer_status_ping());
        assert!(
            built_in("ac1999.cobradragon.com", 9000).is_none(),
            "another port"
        );
        let slugs: std::collections::HashSet<_> = BUILT_IN_WORLDS.iter().map(|b| b.slug).collect();
        assert_eq!(slugs.len(), BUILT_IN_WORLDS.len());
    }

    /// The same address typed by hand is the built-in world; any world found to speak the 1999
    /// wire plays Launch.
    #[test]
    fn a_world_at_a_built_in_address_or_of_the_1999_wire_plays_launch() {
        let mut typed = World::new("custom-3", "My AC1999");
        typed.endpoint = Some(Endpoint {
            address: "ac1999.cobradragon.com".into(),
            port: 9110,
            transport: None,
        });
        apply(&mut typed);
        assert!(typed.speaks_1999());
        assert_eq!(typed.name, "My AC1999", "the player's name stands");
        let mut found = World::new("custom-4", "Local AC1999");
        found.wire = Some(WIRE_1999.into());
        apply(&mut found);
        assert_eq!(found.era.as_deref(), Some("launch"));
        let mut later = World::new("custom-5", "Local");
        later.wire = Some(crate::world::WIRE_LATER.into());
        apply(&mut later);
        assert_eq!(later.era, None);
    }

    #[test]
    fn a_known_world_is_filled_in_and_any_other_is_left_as_its_row_says() {
        let mut w = World::new("dekarutide", "Dekarutide");
        w.list_id = Some("4DB379B0-587A-4775-9B7F-879E0F15BA4B".into());
        w.emulator = Emulator::Ace;
        apply(&mut w);
        assert_eq!(w.emulator, Emulator::ClassicAce);
        assert_eq!(w.era.as_deref(), Some("infiltration"));
        assert_eq!(w.era_source, Some(Told::World));
        assert_eq!(w.logon_version.as_deref(), Some("c118"));
        assert_eq!(w.world_profile.as_deref(), Some("classicace-customdm"));
        assert_eq!(w.world_base.as_deref(), Some("modern"));
        assert_eq!(w.dats.custom.as_ref().unwrap().iterations, CUSTOMDM_FILES);
        let (o, unknown) = EraFeatureOverrides::parse(w.era_features.as_deref().unwrap()).unwrap();
        assert!(unknown.is_empty());
        assert_eq!(o.apply(EraId::Eor.features()), CUSTOMDM_FEATURES);

        let mut other = World::new("coldeve", "Coldeve");
        other.list_id = Some("9d17ce44-7db5-40c1-b7bb-a01c9de21d00".into());
        let before = other.clone();
        apply(&mut other);
        assert_eq!(other, before);
    }
}
