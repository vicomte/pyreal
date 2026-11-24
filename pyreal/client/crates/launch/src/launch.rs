//! The launch plan: what the client starts a session with, in process.
//!
//! A [`LaunchPlan`] never holds the password: the client takes it from the vault at the moment it
//! connects. The plan can be logged, shown and tested freely; [`LaunchPlan::redacted`] is the form
//! for a log.
//!
//! When the world names the era it plays, the plan carries it and the systems the world has with
//! it, so the client's screens show that world's systems from the start. The systems go as the
//! shared bitfield (`pyreal_primitives::EraFeatureBits`): the world's whole set, the era's table
//! with what the world turned on or off over it. A world that names no era plays the end of
//! retail, and the plan says nothing of it. A world that runs a client of its own
//! ([`crate::known`]) also names the set its world is drawn from, the logon version its server
//! accepts and the rules its client played by.
//!
//! [`LaunchPlan::args`] is the same plan as the client's command line, for a run started by hand.

use std::path::PathBuf;

use pyreal_primitives::{EraFeatureBits, EraFeatureOverrides, EraId};

use crate::world::World;

/// The world's whole set of systems as the bitfield the client reads: its era's table (the end of
/// retail's when it names none) with [`World::era_features`] over it. `None` when nobody said its
/// systems, or what was said does not read.
fn era_feature_bits(world: &World) -> Option<EraFeatureBits> {
    let text = world
        .era_features
        .as_deref()
        .filter(|f| !f.trim().is_empty())?;
    let (overrides, _unknown) = EraFeatureOverrides::parse(text).ok()?;
    let era = world
        .era
        .as_deref()
        .and_then(EraId::parse)
        .unwrap_or_default();
    Some(EraFeatureBits::of(overrides.apply(era.features())))
}

/// Everything a session is started with, minus the password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    /// The world's id among the player's worlds, and its name.
    pub world_slug: String,
    pub world_name: String,
    pub host: String,
    pub port: u16,
    pub account: String,
    /// The Modern set, and the Classic one when it is not in the same folder.
    pub dat_dir: PathBuf,
    pub classic_dat_dir: Option<PathBuf>,
    /// What the client is told about the world.
    pub world: WorldLaunch,
    /// The world's plugin policy: its URL, or the file the caller wrote it to.
    pub plugin_policy: Option<String>,
    /// The plugin packages installed for the client, offered to it.
    pub plugins: Vec<PathBuf>,
    /// The world's status address, where the client finds what the world publishes.
    pub world_status: Option<String>,
}

/// What the plan is built from.
#[derive(Debug, Clone)]
pub struct LaunchRequest<'a> {
    pub world: &'a World,
    pub account: &'a str,
    /// The Modern set.
    pub dat_dir: Option<PathBuf>,
    /// The Classic set (`portal.dat`, `cell.dat`), when it is not in `dat_dir`.
    pub classic_dat_dir: Option<PathBuf>,
    /// Where the caller wrote the world's plugin policy document, when the entry carries it
    /// inline; a policy published at a URL is passed as the URL.
    pub plugin_policy_file: Option<PathBuf>,
    /// The plugin packages installed, offered to the client, which loads each the first time it
    /// sees it, until the player unticks it ([`crate::plugins::Shelf::offered_packages`]).
    pub plugins: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// The world lists no address to connect to.
    NoEndpoint,
    /// No dat set was chosen.
    NoDats,
    /// No account was given.
    NoAccount,
}

impl core::fmt::Display for PlanError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PlanError::NoEndpoint => write!(f, "This world has no address."),
            PlanError::NoDats => write!(f, "Choose the data files first."),
            PlanError::NoAccount => write!(f, "Enter an account name."),
        }
    }
}

impl std::error::Error for PlanError {}

/// What the client is told about the world it plays: one description, built from the world as the
/// list has it (its row and the built-in table of worlds that run a client of their own).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
pub struct WorldLaunch {
    /// The data set the world is drawn from, which the player's files must hold.
    pub set: crate::datset::SetKind,
    /// `--world-base`: the set the world names for its world, when it names one.
    pub world_base: Option<String>,
    /// `--era`.
    pub era: Option<String>,
    /// `--era-features`: the world's whole set of systems as the shared bitfield.
    pub era_features: Option<String>,
    /// `--logon-version`, for a world whose server wants another than the end of retail's.
    pub logon_version: Option<String>,
    /// `--world-profile`: the rules the world's own client played by.
    pub world_profile: Option<String>,
    /// `--wire`, for a world whose server speaks the 1999 wire.
    pub wire: Option<String>,
}

/// The description of `world` for the client: [`WorldLaunch`].
#[must_use]
pub fn describe(world: &World) -> WorldLaunch {
    let some = |v: Option<&str>| v.filter(|s| !s.is_empty()).map(str::to_owned);
    WorldLaunch {
        set: crate::choices::required_set_for(world),
        world_base: some(world.world_base.as_deref()),
        era: some(world.era.as_deref()),
        era_features: era_feature_bits(world).map(|b| b.to_string()),
        logon_version: some(world.logon_version.as_deref()),
        world_profile: some(world.world_profile.as_deref()),
        wire: world
            .speaks_1999()
            .then(|| crate::world::WIRE_1999.to_owned()),
    }
}

impl WorldLaunch {
    /// The description as `(switch, value)`, each only when it is said: the client's own names
    /// for them, without the dashes.
    #[must_use]
    pub fn switches(&self) -> Vec<(&'static str, String)> {
        [
            ("world-base", &self.world_base),
            ("era", &self.era),
            ("era-features", &self.era_features),
            ("logon-version", &self.logon_version),
            ("world-profile", &self.world_profile),
            ("wire", &self.wire),
        ]
        .into_iter()
        .filter_map(|(switch, v)| v.as_ref().map(|v| (switch, v.clone())))
        .collect()
    }

    /// The description as the client's command-line switches.
    #[must_use]
    pub fn args(&self) -> Vec<String> {
        self.switches()
            .into_iter()
            .flat_map(|(s, v)| [format!("--{s}"), v])
            .collect()
    }
}

/// Build the plan for a launch.
///
/// # Errors
/// [`PlanError`] for a world with no address, no account or no data files.
pub fn plan(req: &LaunchRequest<'_>) -> Result<LaunchPlan, PlanError> {
    let ep = req.world.endpoint.as_ref().ok_or(PlanError::NoEndpoint)?;
    let account = req.account.trim();
    if account.is_empty() {
        return Err(PlanError::NoAccount);
    }
    let dat_dir = req.dat_dir.clone().ok_or(PlanError::NoDats)?;
    let plugin_policy = req
        .world
        .plugin_policy_url()
        .map(str::to_owned)
        .or_else(|| {
            req.plugin_policy_file
                .as_ref()
                .map(|f| f.display().to_string())
        });
    Ok(LaunchPlan {
        world_slug: req.world.slug.clone(),
        world_name: req.world.name.clone(),
        host: ep.address.clone(),
        port: ep.port,
        account: account.to_owned(),
        dat_dir,
        classic_dat_dir: req.classic_dat_dir.clone(),
        world: describe(req.world),
        plugin_policy,
        plugins: req.plugins.clone(),
        world_status: req
            .world
            .status_url
            .clone()
            .filter(|u| crate::direct::is_status_url(u)),
    })
}

impl LaunchPlan {
    /// The plan as the client's command line, the password left out: the form a run by hand
    /// takes, after `-v <password>`.
    #[must_use]
    pub fn args(&self) -> Vec<String> {
        let mut args = vec![
            "-a".to_owned(),
            self.account.clone(),
            "-h".to_owned(),
            self.host.clone(),
            "-p".to_owned(),
            self.port.to_string(),
            "--dat-dir".to_owned(),
            self.dat_dir.display().to_string(),
        ];
        if let Some(classic) = &self.classic_dat_dir {
            args.extend([
                "--classic-dat-dir".to_owned(),
                classic.display().to_string(),
            ]);
        }
        args.extend(self.world.args());
        if let Some(p) = &self.plugin_policy {
            args.extend(["--plugin-policy".to_owned(), p.clone()]);
        }
        for p in &self.plugins {
            args.extend(["--offer-plugins".to_owned(), p.display().to_string()]);
        }
        if let Some(s) = &self.world_status {
            args.extend(["--world-status".to_owned(), s.clone()]);
        }
        args
    }

    /// The plan in one line for a log: never a password, since the plan holds none.
    #[must_use]
    pub fn redacted(&self) -> String {
        let mut out = format!("{} ({}:{})", self.world_name, self.host, self.port);
        for a in self.args() {
            out.push(' ');
            out.push_str(&quote(&a));
        }
        out
    }
}

fn quote(s: &str) -> String {
    if s.is_empty() || s.contains([' ', '\t', '"']) {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Emulator, Endpoint};

    fn world(emu: Emulator) -> World {
        let mut w = World::new("eulmore", "Eulmore");
        w.emulator = emu;
        w.endpoint = Some(Endpoint {
            address: "eulmore.example".into(),
            port: 19000,
            transport: None,
        });
        w
    }

    fn req(w: &World) -> LaunchRequest<'_> {
        LaunchRequest {
            world: w,
            account: "player",
            dat_dir: Some(PathBuf::from("/lib/eor")),
            classic_dat_dir: None,
            plugin_policy_file: None,
            plugins: Vec::new(),
        }
    }

    #[test]
    fn a_plan_names_the_world_the_account_and_the_dat_set() {
        let w = world(Emulator::Ace);
        let p = plan(&req(&w)).unwrap();
        assert_eq!((p.host.as_str(), p.port), ("eulmore.example", 19000));
        assert_eq!(p.account, "player");
        assert_eq!(
            p.args(),
            [
                "-a",
                "player",
                "-h",
                "eulmore.example",
                "-p",
                "19000",
                "--dat-dir",
                "/lib/eor"
            ]
        );
        let mut with_status = w.clone();
        with_status.status_url = Some("http://127.0.0.1:9100/status".into());
        let p = plan(&req(&with_status)).unwrap();
        assert_eq!(
            p.args()[p.args().len() - 2..],
            ["--world-status", "http://127.0.0.1:9100/status"],
            "the world's status address goes with it"
        );
        let mut r = req(&w);
        r.dat_dir = None;
        assert_eq!(plan(&r), Err(PlanError::NoDats));
        r = req(&w);
        r.account = "  ";
        assert_eq!(plan(&r), Err(PlanError::NoAccount));
        let mut w = world(Emulator::Ace);
        w.endpoint = None;
        assert_eq!(plan(&req(&w)), Err(PlanError::NoEndpoint));
    }

    #[test]
    fn a_worlds_plugin_policy_is_its_url_or_the_file_it_was_written_to() {
        let mut w = world(Emulator::Empyrean);
        w.plugin_policy = Some("https://eulmore.example/policy.toml".into());
        assert_eq!(
            plan(&req(&w)).unwrap().plugin_policy.as_deref(),
            Some("https://eulmore.example/policy.toml")
        );
        w.plugin_policy = Some("policy_version = 1".into());
        let mut r = req(&w);
        assert_eq!(plan(&r).unwrap().plugin_policy, None);
        r.plugin_policy_file = Some(PathBuf::from("/tmp/eulmore.toml"));
        assert_eq!(
            plan(&r).unwrap().plugin_policy.as_deref(),
            Some("/tmp/eulmore.toml")
        );
    }

    #[test]
    fn what_the_client_is_told_about_a_known_world() {
        let mut w = world(Emulator::Gdle);
        assert!(
            describe(&w).switches().is_empty(),
            "a world that says nothing"
        );
        assert_eq!(describe(&w).set, crate::datset::SetKind::Modern);
        w.list_id = Some("394c58d0-885d-466b-b17f-d7e0b96fe3e2".into());
        crate::known::apply(&mut w);
        let d = describe(&w);
        assert_eq!(d.set, crate::datset::SetKind::Modern);
        assert_eq!(d.era_features.as_deref(), Some("5:0000dff7fefff700"));
        let words = d.args();
        assert_eq!(
            words[..4],
            ["--world-base", "modern", "--era", "infiltration"]
        );
        assert_eq!(
            words[6..],
            [
                "--logon-version",
                "1802",
                "--world-profile",
                "classicdereth"
            ]
        );
        let args = plan(&req(&w)).unwrap().args();
        assert_eq!(args[args.len() - words.len()..], words[..]);
    }

    #[test]
    fn a_world_kept_by_hand_with_the_1999_wire_is_told_so() {
        let row: crate::state::CustomWorld = serde_json::from_str(
            r#"{"slug":"custom-1","name":"AC1999","host":"127.0.0.1","port":9030,"wire":"1999"}"#,
        )
        .expect("a hand-kept row reads");
        let d = describe(&row.to_world());
        assert_eq!(
            d.args(),
            ["--era", "launch", "--wire", "1999"],
            "a 1999 world plays Launch"
        );
        let plain: crate::state::CustomWorld =
            serde_json::from_str(r#"{"slug":"custom-2","name":"Other","host":"h","port":1}"#)
                .expect("reads");
        assert!(serde_json::to_string(&plain)
            .unwrap()
            .find("wire")
            .is_none());
        assert!(describe(&plain.to_world()).switches().is_empty());
        let later = crate::state::CustomWorld {
            wire: Some(crate::world::WIRE_LATER.into()),
            ..plain
        };
        assert!(
            describe(&later.to_world()).switches().is_empty(),
            "the later wire is the default"
        );
    }

    #[test]
    fn the_era_and_its_systems_go_as_the_whole_set() {
        let mut w = world(Emulator::Empyrean);
        w.era = Some("infiltration".into());
        w.era_features = Some("trade=false,aetheria=true".into());
        let mut r = req(&w);
        r.classic_dat_dir = Some(PathBuf::from("/lib/feb2005"));
        let args = plan(&r).unwrap().args();
        assert_eq!(
            args[7..],
            [
                "/lib/eor",
                "--classic-dat-dir",
                "/lib/feb2005",
                "--era",
                "infiltration",
                "--era-features",
                "5:0002def7fefff700"
            ]
        );
    }

    #[test]
    fn the_logged_form_quotes_paths_with_spaces() {
        let w = world(Emulator::Ace);
        let mut r = req(&w);
        r.dat_dir = Some(PathBuf::from("/Turbine/Asheron's Call"));
        let shown = plan(&r).unwrap().redacted();
        assert!(shown.contains("\"/Turbine/Asheron's Call\""), "{shown}");
        assert!(
            shown.starts_with("Eulmore (eulmore.example:19000)"),
            "{shown}"
        );
    }
}
