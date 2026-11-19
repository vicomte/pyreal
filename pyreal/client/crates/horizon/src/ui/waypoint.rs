//! The waypoint: a place the player follows. A double click on the map sets it, as do `@way` in
//! chat and a Journal page's Set Waypoint. While one is set the radar's rim carries an arrow the way
//! to it, the ground round the character a ring with an arrow on it (as the Waypoints setting
//! chooses), and a Clear Waypoint button stands under the radar. It clears itself once the
//! player stands within [`ARRIVED_METRES`] of it.
//!
//! The way is worked out from the game's coordinates, in a building's rooms too. A dungeon's
//! coordinates are not where it lies, so in a dungeon neither the radar's arrow nor the ring is
//! shown. Each
//! character keeps its own waypoint for each world, in a small file in the settings folder;
//! explore mode keeps one for each era.

use std::path::{Path, PathBuf};

use pyreal_client_model::cmd::waypoint::{place_text, WaypointCommand};

use crate::art::Image;

/// A route's step and its count in a title that ends `(2/5)`, and the title before it.
fn route_step(title: &str) -> Option<(&str, u32, u32)> {
    let (stem, tail) = title.trim().rsplit_once(" (")?;
    let (step, of) = tail.strip_suffix(')')?.split_once('/')?;
    Some((stem, step.trim().parse().ok()?, of.trim().parse().ok()?))
}

/// The page with the next step of a route, with a place, after arriving at the step called
/// `arrived`: an Advocate's route pages are titled `To Fort Tethana (1/2)` and so on.
#[must_use]
pub fn next_route_page<'a>(
    pages: &'a [pyreal_client_contract::journal::JournalPage],
    arrived: &str,
) -> Option<&'a pyreal_client_contract::journal::JournalPage> {
    let (stem, step, of) = route_step(arrived)?;
    (step < of).then_some(())?;
    pages.iter().find(|p| {
        p.location_set
            && route_step(&p.title).is_some_and(|(s, n, o)| s == stem && n == step + 1 && o == of)
    })
}
/// A unit of the game's coordinates, in metres: ten landcells of 24.
pub const METRES_PER_UNIT: f64 = 240.0;

/// How near the waypoint, in metres, the player has arrived and it clears. A place typed to a
/// tenth is only good to about 12 metres.
pub const ARRIVED_METRES: f64 = 10.0;

/// The ring's radius round the character, metres.
pub const RING_RADIUS: f32 = 1.6;

/// The arrow's colours: a warm gold in a dark edge, which reads on every ground.
pub const FILL: [u8; 3] = [255, 184, 40];
pub const EDGE: [u8; 3] = [20, 14, 6];

/// What the arrow's picture is kept under.
pub const ARROW_KEY: &str = "waypoint.arrow";

/// The arrow's picture's side, in its pixels.
const ARROW_SIDE: u32 = 64;

/// Where the arrows the Waypoints setting shows are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaypointStyle {
    #[default]
    Radar,
    OnCharacter,
    Both,
}

impl WaypointStyle {
    /// Every style, in the order Settings lists them.
    pub const ALL: [Self; 3] = [Self::Radar, Self::OnCharacter, Self::Both];

    /// Its name in Settings.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Radar => "Radar",
            Self::OnCharacter => "On Character",
            Self::Both => "Both",
        }
    }

    /// Its word in the settings file.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Radar => "radar",
            Self::OnCharacter => "character",
            Self::Both => "both",
        }
    }

    /// The style a settings file's word names.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.word() == word.trim())
    }

    /// Whether the radar carries the arrow.
    #[must_use]
    pub const fn on_radar(self) -> bool {
        matches!(self, Self::Radar | Self::Both)
    }

    /// Whether the ground round the character carries the ring.
    #[must_use]
    pub const fn on_character(self) -> bool {
        matches!(self, Self::OnCharacter | Self::Both)
    }
}

/// A place to follow: north and east in the game's coordinates (south and west negative), and
/// its name when it has one.
#[derive(Debug, Clone, PartialEq)]
pub struct Waypoint {
    pub ns: f64,
    pub ew: f64,
    pub name: Option<String>,
}

impl Waypoint {
    /// Its name, or its place when it has none.
    #[must_use]
    pub fn label(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| place_text(self.ns, self.ew))
    }

    /// The way to it from `here` (`ns`, `ew`), in radians clockwise from north, and how far, in
    /// metres.
    #[must_use]
    pub fn way_from(&self, here: (f64, f64)) -> (f32, f64) {
        let north = (self.ns - here.0) * METRES_PER_UNIT;
        let east = (self.ew - here.1) * METRES_PER_UNIT;
        #[allow(clippy::cast_possible_truncation)] // LINT-OK: an angle.
        let bearing = pyreal_primitives::num::math::atan2(east, north) as f32;
        (bearing, pyreal_primitives::num::math::hypot(north, east))
    }
}

/// `metres` as a distance is read: metres to a kilometre, then kilometres to a tenth.
#[must_use]
pub fn distance_text(metres: f64) -> String {
    if metres < 1000.0 {
        format!("{metres:.0} m")
    } else {
        format!("{:.1} km", metres / 1000.0)
    }
}

/// The compass point nearest `bearing` (radians clockwise from north): "north", "north-east".
#[must_use]
pub fn compass_word(bearing: f32) -> &'static str {
    const WORDS: [&str; 8] = [
        "north",
        "north-east",
        "east",
        "south-east",
        "south",
        "south-west",
        "west",
        "north-west",
    ];
    let eighth = (bearing / std::f32::consts::FRAC_PI_4).round();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // LINT-OK: 0 to 7.
    let at = (eighth.rem_euclid(8.0) as usize) % 8;
    WORDS[at]
}

/// The waypoint followed, and the file it is kept in.
#[derive(Debug, Default)]
pub struct Waypoints {
    /// The waypoint followed, if any.
    pub active: Option<Waypoint>,
    /// Who it was read for, and its file.
    kept_for: Option<String>,
    path: Option<PathBuf>,
}

impl Waypoints {
    /// The waypoint kept for `who` (a character in a world, or an explore era), read from its
    /// file in `dir` once, when `who` changes. Nothing is read for nobody.
    pub fn load_for(&mut self, dir: Option<&Path>, who: &str) {
        if who.is_empty() || self.kept_for.as_deref() == Some(who) {
            return;
        }
        self.path = dir.map(|d| d.join(file_name(who)));
        self.active = self
            .path
            .as_ref()
            .and_then(|p| pyreal_client_runtime::platform::files::read_to_string(p).ok())
            .and_then(|t| parse(&t));
        self.kept_for = Some(who.to_owned());
    }

    /// Follow `waypoint` from now on, and keep it.
    pub fn set(&mut self, waypoint: Waypoint) {
        self.active = Some(waypoint);
        self.save();
    }

    /// Follow no waypoint, and keep none.
    pub fn clear(&mut self) {
        self.active = None;
        self.save();
    }

    /// Carry out a typed `@way`, answering with the line to print.
    pub fn command(&mut self, command: WaypointCommand) -> String {
        match command {
            WaypointCommand::Show => match &self.active {
                Some(w) => match &w.name {
                    Some(name) => format!("The waypoint is {name} at {}.", place_text(w.ns, w.ew)),
                    None => format!("The waypoint is at {}.", place_text(w.ns, w.ew)),
                },
                None => "No waypoint is set.".to_owned(),
            },
            WaypointCommand::Set { ns, ew, name } => {
                let w = Waypoint { ns, ew, name };
                let answer = format!("Waypoint set to {}.", set_text(&w));
                self.set(w);
                answer
            }
            WaypointCommand::Clear => {
                let was = self.active.is_some();
                self.clear();
                if was {
                    "The waypoint is cleared.".to_owned()
                } else {
                    "No waypoint is set.".to_owned()
                }
            }
        }
    }

    /// Whether the player at `here` has arrived at the waypoint, which clears it then.
    pub fn arrive(&mut self, here: (f64, f64)) -> Option<Waypoint> {
        let near = self
            .active
            .as_ref()
            .is_some_and(|w| w.way_from(here).1 <= ARRIVED_METRES);
        if !near {
            return None;
        }
        let w = self.active.take();
        self.save();
        w
    }

    fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let done = match &self.active {
            Some(w) => std::fs::write(path, to_text(w)),
            None if path.exists() => std::fs::remove_file(path),
            None => Ok(()),
        };
        if let Err(e) = done {
            tracing::warn!("the waypoint was not kept: {e}");
        }
    }
}

/// How a set waypoint is named in a line: its name and place, or its place.
#[must_use]
pub fn set_text(w: &Waypoint) -> String {
    match &w.name {
        Some(name) => format!("{name} at {}", place_text(w.ns, w.ew)),
        None => place_text(w.ns, w.ew),
    }
}

/// The file, in the settings folder, the waypoint kept for `who` is in.
#[must_use]
pub fn file_name(who: &str) -> String {
    let safe: String = who
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("horizon-waypoint-{safe}.txt")
}

/// A waypoint's file: `ns=`, `ew=` and `name=` lines.
#[must_use]
pub fn to_text(w: &Waypoint) -> String {
    let mut text = format!("ns={}\new={}\n", w.ns, w.ew);
    if let Some(name) = &w.name {
        text.push_str(&format!("name={}\n", name.replace('\n', " ")));
    }
    text
}

/// The waypoint a file holds; `None` for one that names no place.
#[must_use]
pub fn parse(text: &str) -> Option<Waypoint> {
    let (mut ns, mut ew, mut name) = (None, None, None);
    for line in text.lines() {
        match line.split_once('=') {
            Some(("ns", v)) => ns = v.trim().parse::<f64>().ok().filter(|v| v.is_finite()),
            Some(("ew", v)) => ew = v.trim().parse::<f64>().ok().filter(|v| v.is_finite()),
            Some(("name", v)) if !v.trim().is_empty() => name = Some(v.trim().to_owned()),
            _ => {}
        }
    }
    Some(Waypoint {
        ns: ns?,
        ew: ew?,
        name,
    })
}

/// The arrow's picture: a gold arrowhead in a dark edge, pointing straight up, its tip at the
/// middle of the top edge.
#[must_use]
pub fn arrow_image() -> Image {
    const SHAPE: [(f32, f32); 4] = [(32.0, 3.0), (58.0, 60.0), (32.0, 46.0), (6.0, 60.0)];
    let side = ARROW_SIDE as usize;
    let mut bgra = vec![0; side * side * 4];
    for (i, px) in bgra.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        #[allow(clippy::cast_precision_loss)] // LINT-OK: within the picture's few pixels.
        let d = edge_distance(&SHAPE, (i % side) as f32 + 0.5, (i / side) as f32 + 0.5);
        let fill = ((-d - 3.5) + 0.5).clamp(0.0, 1.0);
        let cover = (0.5 - d).clamp(0.0, 1.0);
        let mix = |e: u8, f: u8| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // LINT-OK: a colour channel between two in range.
            {
                (f32::from(e) * (1.0 - fill) + f32::from(f) * fill) as u8
            }
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: a coverage in 0..1 scaled to a byte.
        px.copy_from_slice(&[
            mix(EDGE[2], FILL[2]),
            mix(EDGE[1], FILL[1]),
            mix(EDGE[0], FILL[0]),
            (cover * 255.0) as u8,
        ]);
    }
    Image {
        width: ARROW_SIDE,
        height: ARROW_SIDE,
        bgra,
    }
}

/// The arrow's tip in its picture, as a share of its side from the top.
pub const ARROW_TIP: f32 = 3.0 / 64.0;

/// How far `(x, y)` stands from the edge of the polygon `shape`, negative inside.
fn edge_distance(shape: &[(f32, f32)], x: f32, y: f32) -> f32 {
    let mut nearest = f32::MAX;
    let mut inside = false;
    for (i, &(ax, ay)) in shape.iter().enumerate() {
        let (bx, by) = shape[(i + 1) % shape.len()];
        let (dx, dy) = (bx - ax, by - ay);
        let t = (((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        let (ox, oy) = (x - ax - t * dx, y - ay - t * dy);
        nearest = nearest.min(ox * ox + oy * oy);
        if (ay > y) != (by > y) && x < ax + (y - ay) / dy * dx {
            inside = !inside;
        }
    }
    let d = nearest.sqrt();
    if inside {
        -d
    } else {
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_waypoint_s_way_is_clockwise_from_north_and_its_distance_in_metres() {
        let w = Waypoint {
            ns: 42.0,
            ew: 34.0,
            name: None,
        };
        let (north, d) = w.way_from((41.0, 34.0));
        assert!(north.abs() < 1e-6 && (d - 240.0).abs() < 1e-9);
        let (east, _) = w.way_from((42.0, 33.0));
        assert!((east - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        let (south_west, _) = w.way_from((43.0, 35.0));
        assert!((south_west + 3.0 * std::f32::consts::FRAC_PI_4).abs() < 1e-6);
        assert_eq!(distance_text(437.4), "437 m");
        assert_eq!(distance_text(1260.0), "1.3 km");
        assert_eq!(compass_word(south_west), "south-west");
        assert_eq!(compass_word(3.1), "south");
        assert_eq!(compass_word(-0.2), "north");
    }

    #[test]
    fn a_waypoint_is_kept_per_character_and_cleared_on_arrival() {
        let dir = std::env::temp_dir().join(format!("pyreal-waypoint-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut w = Waypoints::default();
        w.load_for(Some(&dir), "127.0.0.1:9000|Ardent");
        assert_eq!(w.active, None);
        let answer = w.command(WaypointCommand::Set {
            ns: 41.6,
            ew: 33.9,
            name: Some("The well".into()),
        });
        assert_eq!(answer, "Waypoint set to The well at 41.6N 33.9E.");
        // Another character keeps its own, and coming back reads the first's from its file.
        let mut other = Waypoints::default();
        other.load_for(Some(&dir), "127.0.0.1:9000|Bram");
        assert_eq!(other.active, None);
        let mut again = Waypoints::default();
        again.load_for(Some(&dir), "127.0.0.1:9000|Ardent");
        assert_eq!(again.active, w.active);
        // Ten metres off it has not arrived; within them it has, and the file goes.
        assert!(again
            .arrive((41.6 - 11.0 / METRES_PER_UNIT, 33.9))
            .is_none());
        let arrived = again.arrive((41.6 - 9.0 / METRES_PER_UNIT, 33.9));
        assert_eq!(arrived.map(|a| a.label()), Some("The well".to_owned()));
        assert!(!dir.join(file_name("127.0.0.1:9000|Ardent")).exists());
        assert_eq!(again.command(WaypointCommand::Show), "No waypoint is set.");
        assert_eq!(parse("ns=1\nname=x\n"), None, "no east");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_waypoints_setting_reads_its_words_and_says_where_each_arrow_is() {
        for s in WaypointStyle::ALL {
            assert_eq!(WaypointStyle::from_word(s.word()), Some(s));
        }
        assert_eq!(WaypointStyle::default(), WaypointStyle::Radar);
        assert!(WaypointStyle::Radar.on_radar() && !WaypointStyle::Radar.on_character());
        assert!(!WaypointStyle::OnCharacter.on_radar());
        assert!(WaypointStyle::Both.on_radar() && WaypointStyle::Both.on_character());
    }

    #[test]
    fn arriving_at_a_route_s_step_finds_the_next_step_s_page() {
        use pyreal_client_contract::journal::JournalPage;
        let step = |title: &str, placed: bool| JournalPage {
            title: title.into(),
            location_set: placed,
            ns: 42.3,
            ew: -33.3,
            ..JournalPage::default()
        };
        let pages = [
            step("To Fort Tethana (1/3)", true),
            step("To Fort Tethana (2/3)", true),
            step("To Fort Tethana (3/3)", false),
            step("To Yaraq (2/3)", true),
        ];
        let next = |arrived| next_route_page(&pages, arrived).map(|p| p.title.as_str());
        assert_eq!(next("To Fort Tethana (1/3)"), Some("To Fort Tethana (2/3)"));
        assert_eq!(
            next("To Fort Tethana (2/3)"),
            None,
            "the last step has no place"
        );
        assert_eq!(next("To Fort Tethana (3/3)"), None);
        assert_eq!(next("The well"), None);
    }
}
