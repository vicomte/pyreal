//! The corpse marker: where the player's corpse lies. A death waits for the game to say what it
//! cost, and one that kept every item (the game says so, or a PK Lite death to another player)
//! marks nothing. The place is the game's: the corpse that comes into the world just after the
//! death, else the last outside death the game keeps, else where the player stood. The map and
//! the radar's rim show it, and Set Waypoint to Corpse leads back to it. It clears once the corpse
//! is opened and emptied, once it vanishes close by or the player stands by the place and no
//! corpse of theirs is there (it decayed), or on a right click on its stone.
//!
//! A corpse in a dungeon is shown on the radar only in that dungeon: a dungeon's coordinates are
//! not where it lies, so it has no place on the map and no waypoint. Each character keeps its own
//! marker for each world, with the last outside death it knew, in a small file in the settings
//! folder: a different one at login is a death this client did not see, and is marked.

use std::path::{Path, PathBuf};

use pyreal_primitives::ObjectId;

use crate::art::Image;
use crate::ui::death_recap::{Recap, LOSS_SECONDS};
use crate::ui::game::{BlipKind, GameState};
use crate::ui::waypoint::{edge_distance, Waypoint};

/// How near the place, in metres, the player stands for the corpse to be looked for there.
pub const NEAR_METRES: f64 = 20.0;

/// How far from the player, in metres, a corpse of theirs still counts as the one marked.
pub const SEEN_METRES: f32 = 60.0;

/// How long, in seconds, no corpse is seen by the place before the marker clears.
pub const MISSING_SECONDS: f64 = 5.0;

/// How long after a death, in seconds, the corpse is not looked for: the game lays it a moment
/// after the death, as the player leaves.
pub const GRACE_SECONDS: f64 = 20.0;

/// How far a dungeon's corpse is shown on the radar, in metres, when its landblock is not known.
pub const DUNGEON_METRES: f64 = 300.0;

/// How long after a death, in seconds, the place is marked though the game has not said what the
/// death cost.
pub const SETTLE_SECONDS: f64 = 20.0;

/// How long after a death, in seconds, a corpse of the player's coming into the world is its own.
pub const CAPTURE_SECONDS: f64 = 30.0;

/// How near the player, in metres, the marked corpse vanishes for it to have decayed.
pub const DECAY_METRES: f64 = 10.0;

/// How soon after the death, in seconds, and how near, in metres, the corpse vanishing means it
/// was empty: the game rots an empty corpse in fifteen seconds.
pub const EMPTY_SECONDS: f64 = 25.0;
pub const EMPTY_METRES: f64 = 100.0;

/// How far from the place, in metres, a corpse of the player's is taken as the one marked.
pub const SNAP_METRES: f64 = 20.0;

/// How far, in metres, the marked corpse's id moves before it is another object's.
pub const MOVED_METRES: f64 = 2.0;

/// How long before the death count rises, in seconds, the last outside death may change for it.
pub const EARLY_SECONDS: f64 = 5.0;

/// The position quality the game keeps the last outside death in.
pub const LAST_OUTSIDE_DEATH: u32 = 14;

/// The last line of the marker's tooltip, on the map and the radar.
pub const FORGET_TIP: &str = "Right click to forget it.";

/// What the marker's picture is kept under.
pub const MARK_KEY: &str = "corpse.mark";

/// The marker's colours: a pale stone in the waypoint arrow's dark edge, and its cross.
pub const STONE: [u8; 3] = [226, 220, 204];
pub const CROSS: [u8; 3] = [96, 88, 78];

/// The marker's picture's side, in its pixels.
const MARK_SIDE: u32 = 64;

/// A place in the world: north and east in the game's coordinates, and its landcell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Place {
    pub ns: f64,
    pub ew: f64,
    pub cell: u32,
}

impl Place {
    /// The place `x`, `y` metres into the landblock of `cell`.
    #[must_use]
    pub fn at(cell: u32, x: f32, y: f32) -> Self {
        let (ns, ew) = crate::ui::panels::map::spot_in(cell, x, y);
        Self { ns, ew, cell }
    }

    /// Whether it is out on the land.
    #[must_use]
    pub const fn outdoors(&self) -> bool {
        self.cell & 0xFFFF < 0x100
    }
}

/// An object in the world named as the player's corpse.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lying {
    pub id: ObjectId,
    pub place: Place,
}

/// Where the player's corpse lies: north and east in the game's coordinates, whether in a
/// dungeon, and its landcell when known.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Corpse {
    pub ns: f64,
    pub ew: f64,
    pub in_dungeon: bool,
    pub cell: Option<u32>,
}

impl Corpse {
    /// A corpse out on the land at `p`.
    #[must_use]
    pub const fn outside(p: Place) -> Self {
        Self {
            ns: p.ns,
            ew: p.ew,
            in_dungeon: false,
            cell: Some(p.cell),
        }
    }

    /// This corpse, found lying at `p`: indoors on another landblock is a dungeon.
    #[must_use]
    pub fn lying_at(self, p: Place) -> Self {
        let other_block = self.cell.is_some_and(|c| c >> 16 != p.cell >> 16);
        Self {
            ns: p.ns,
            ew: p.ew,
            in_dungeon: !p.outdoors() && (other_block || self.in_dungeon),
            cell: Some(p.cell),
        }
    }

    /// How far it lies from `at`, `(ns, ew)`, in metres.
    #[must_use]
    pub fn metres_from(&self, at: (f64, f64)) -> f64 {
        place(self).way_from(at).1
    }
}

/// What changed this frame, for a line in chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorpseNews {
    /// The player died: the place is marked.
    Marked,
    /// The corpse was opened and emptied.
    Recovered,
    /// The corpse is not there any more.
    Gone,
}

impl CorpseNews {
    /// The line said in chat, if any.
    #[must_use]
    pub const fn line(self) -> Option<&'static str> {
        match self {
            Self::Marked => Some("The place you died is marked on your map and radar."),
            Self::Recovered => None,
            Self::Gone => Some("Your corpse is no longer here, so its marker is cleared."),
        }
    }
}

/// What the marker looks at in a frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Seen {
    /// The character's death count, once the game has sent it.
    pub deaths: Option<i32>,
    /// Where the player stands, `(ns, ew)`, and the landcell.
    pub here: Option<(f64, f64)>,
    pub cell: Option<u32>,
    pub in_dungeon: bool,
    pub teleporting: bool,
    /// How far off, in metres, the nearest object named as the player's corpse stands.
    pub corpse_metres: Option<f32>,
    /// The player's own corpse is open: how many items are in it.
    pub opened: Option<usize>,
    /// The player's corpses in the world.
    pub lying: Vec<Lying>,
    /// The last outside death the game keeps for the character.
    pub last_outside: Option<Place>,
    /// What the last death cost, once known: `Some(true)` when it took no item.
    pub kept: Option<bool>,
}

impl Seen {
    /// What `state` shows of the player's death and corpse, with the death recap `recap` saying
    /// what the last death cost, at `now`.
    #[must_use]
    pub fn of(state: &GameState, recap: Option<&Recap>, now: f64) -> Self {
        let name = corpse_name(&state.name);
        let corpse_metres = state
            .targets
            .iter()
            .filter(|b| b.name == name)
            .map(|b| pyreal_primitives::num::math::hypotf(b.dx, b.dy))
            .reduce(f32::min);
        let recap = recap.filter(|r| now - r.at <= LOSS_SECONDS);
        // A PK Lite character killed by another player loses nothing.
        let pk_lite_death = state.pk_lite
            && recap.and_then(Recap::killer).is_some_and(|k| {
                state
                    .targets
                    .iter()
                    .any(|b| b.kind == BlipKind::Player && b.name == k)
            });
        let kept = if pk_lite_death {
            Some(true)
        } else {
            recap
                .and_then(|r| r.loss.as_ref())
                .and_then(|l| match (l.kept, l.items.is_empty()) {
                    (true, _) => Some(true),
                    (false, false) => Some(false),
                    (false, true) => None,
                })
        };
        Self {
            deaths: state.deaths,
            here: state.here,
            cell: state.cell,
            in_dungeon: state.in_dungeon,
            teleporting: state.teleporting,
            corpse_metres,
            opened: state
                .loot
                .as_ref()
                .filter(|(_, n, _)| *n == name)
                .map(|(_, _, items)| items.len()),
            lying: state.own_corpses.clone(),
            last_outside: state.last_outside_death,
            kept,
        }
    }
}

/// The name the game gives the corpse of the character called `name`.
#[must_use]
pub fn corpse_name(name: &str) -> String {
    format!("Corpse of {name}")
}

/// A death waiting for the game to say what it cost.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pending {
    at: f64,
    corpse: Corpse,
    /// Its corpse object, once it came.
    id: Option<ObjectId>,
}

/// The marked corpse, and the file it is kept in.
#[derive(Debug, Default)]
pub struct CorpseMarker {
    /// Where the player's corpse lies, while it is marked.
    pub active: Option<Corpse>,
    /// The last death, until the game says what it cost.
    pending: Option<Pending>,
    /// The death count last seen, after the first.
    deaths: Option<i32>,
    /// When the marked death came, this session.
    died_at: Option<f64>,
    /// The marked corpse's object, once seen, and an id seen again elsewhere: another corpse.
    tracked: Option<ObjectId>,
    disowned: Option<ObjectId>,
    /// The player's corpses in the world last frame.
    lying: Vec<Lying>,
    /// The last outside death as seen this session, and when it changed.
    outside: Option<Place>,
    outside_changed: Option<f64>,
    /// The last outside death as the file keeps it.
    recorded: Option<Place>,
    /// The open corpse held something: emptying it recovers it.
    held: bool,
    /// Since when the player has stood by the place with no corpse seen.
    missing_since: Option<f64>,
    path: Option<PathBuf>,
}

impl CorpseMarker {
    /// The marker kept for `who`, read from its file in `dir`; the death count is learnt anew.
    pub fn load_for(&mut self, dir: Option<&Path>, who: &str) {
        *self = Self {
            path: dir.map(|d| d.join(file_name(who))),
            ..Self::default()
        };
        let text = self
            .path
            .as_ref()
            .and_then(|p| pyreal_client_runtime::platform::files::read_to_string(p).ok());
        if let Some(t) = text {
            self.active = parse(&t);
            self.recorded = parse_outside(&t);
        }
    }

    /// Follow a frame at `now` seconds: a death marks the place once the game says it cost
    /// something, and an emptied, vanished or missing corpse clears it.
    pub fn follow(&mut self, seen: &Seen, now: f64) -> Option<CorpseNews> {
        let last = std::mem::replace(&mut self.lying, seen.lying.clone());
        if let Some(n) = seen.deaths {
            let died = self.deaths.is_some_and(|m| n > m);
            self.deaths = Some(n);
            if let (true, Some((ns, ew))) = (died, seen.here) {
                let corpse = Corpse {
                    ns,
                    ew,
                    in_dungeon: seen.in_dungeon,
                    cell: seen.cell,
                };
                self.pending = Some(Pending {
                    at: now,
                    corpse,
                    id: None,
                });
            }
        }
        let news = self
            .learn_outside(seen, now)
            .or_else(|| self.follow_pending(seen, &last, now))
            .or_else(|| self.follow_tracked(seen, &last, now));
        if news.is_some() {
            return news;
        }
        let here = seen.here.filter(|_| !seen.teleporting)?;
        let metres = self.way(here, seen.in_dungeon, seen.cell)?.1;
        // Opened by the place, it held something and now holds nothing.
        match seen.opened.filter(|_| metres <= NEAR_METRES * 1.5) {
            Some(0) if self.held => {
                self.forget();
                return Some(CorpseNews::Recovered);
            }
            Some(n) => self.held |= n > 0,
            None => self.held = false,
        }
        let settled = self.died_at.is_none_or(|t| now - t >= GRACE_SECONDS);
        let missing = seen.corpse_metres.is_none_or(|m| m > SEEN_METRES);
        if !(settled && missing && metres <= NEAR_METRES) {
            self.missing_since = None;
            return None;
        }
        let since = *self.missing_since.get_or_insert(now);
        (now - since >= MISSING_SECONDS).then(|| {
            self.forget();
            CorpseNews::Gone
        })
    }

    /// The last outside death the game keeps: a change is a death's place, and at login one the
    /// file does not know is a death this client did not see.
    fn learn_outside(&mut self, seen: &Seen, now: f64) -> Option<CorpseNews> {
        let p = seen.last_outside?;
        match self.outside {
            Some(o) if same_place(o, p) => None,
            Some(_) => {
                self.outside = Some(p);
                self.outside_changed = Some(now);
                None
            }
            None => {
                self.outside = Some(p);
                if self.pending.is_none() && self.recorded.is_some_and(|r| !same_place(r, p)) {
                    self.mark(Corpse::outside(p), now);
                    return Some(CorpseNews::Marked);
                }
                if self.recorded.is_none() {
                    self.save();
                }
                None
            }
        }
    }

    /// The death waiting for what it cost: its corpse's place as the game gives it, then marked
    /// once something was lost, or let go if nothing was or its corpse rotted at once.
    fn follow_pending(&mut self, seen: &Seen, last: &[Lying], now: f64) -> Option<CorpseNews> {
        let mut p = self.pending?;
        let outside = self.outside.filter(|_| {
            self.outside_changed
                .is_some_and(|t| t >= p.at - EARLY_SECONDS)
        });
        if let (Some(o), None) = (outside, p.id) {
            p.corpse = Corpse::outside(o);
        }
        if p.id.is_none() && now - p.at <= CAPTURE_SECONDS {
            let come = seen.lying.iter().find(|l| {
                is_new(l, last)
                    && outside
                        .is_none_or(|o| Corpse::outside(o).metres_from(at(&l.place)) <= SNAP_METRES)
            });
            if let Some(l) = come {
                p.id = Some(l.id);
                p.corpse = p.corpse.lying_at(l.place);
            }
        }
        let rotted = p.id.is_some_and(|id| vanished(id, seen, last))
            && seen
                .here
                .is_some_and(|h| p.corpse.metres_from(h) <= EMPTY_METRES);
        if seen.kept == Some(true) || rotted {
            self.pending = None;
            self.save();
            return None;
        }
        if seen.kept.is_none() && now - p.at < SETTLE_SECONDS {
            self.pending = Some(p);
            return None;
        }
        self.pending = None;
        self.mark(p.corpse, p.at);
        self.tracked = p.id;
        Some(CorpseNews::Marked)
    }

    /// The marked corpse's object: its exact place while seen, and gone when it vanishes close by
    /// or soon after the death. Its id seen at another place is another object's.
    fn follow_tracked(&mut self, seen: &Seen, last: &[Lying], now: f64) -> Option<CorpseNews> {
        let c = self.active?;
        if self.tracked.is_none() {
            self.tracked = seen
                .lying
                .iter()
                .filter(|l| Some(l.id) != self.disowned)
                .map(|l| (l, c.metres_from(at(&l.place))))
                .filter(|(_, m)| *m <= SNAP_METRES)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(l, _)| l.id);
        }
        let id = self.tracked?;
        if let Some(l) = seen.lying.iter().find(|l| l.id == id) {
            let moved = c.metres_from(at(&l.place));
            if moved > SNAP_METRES || (moved > MOVED_METRES && last.iter().any(|o| o.id == id)) {
                self.tracked = None;
                self.disowned = Some(id);
            } else if moved > 0.1 {
                self.active = Some(c.lying_at(l.place));
                self.save();
            }
            return None;
        }
        if !vanished(id, seen, last) {
            return None;
        }
        let metres = seen.here.map(|h| c.metres_from(h))?;
        let soon = self.died_at.is_some_and(|t| now - t <= EMPTY_SECONDS);
        (metres <= DECAY_METRES || (soon && metres <= EMPTY_METRES)).then(|| {
            self.forget();
            CorpseNews::Gone
        })
    }

    /// Out of the world: the death count, the corpses about and a death not yet marked are let
    /// go, and the last outside death is learnt anew on the way back in.
    pub fn out_of_world(&mut self) {
        self.deaths = None;
        self.pending = None;
        self.tracked = None;
        self.disowned = None;
        self.lying.clear();
        self.outside = None;
        self.outside_changed = None;
    }

    /// Mark `corpse`, a death at `now`.
    pub fn mark(&mut self, corpse: Corpse, now: f64) {
        self.active = Some(corpse);
        self.died_at = Some(now);
        self.tracked = None;
        self.disowned = None;
        self.held = false;
        self.missing_since = None;
        self.save();
    }

    /// Mark no corpse, and keep none.
    pub fn forget(&mut self) {
        self.active = None;
        self.tracked = None;
        self.disowned = None;
        self.held = false;
        self.missing_since = None;
        self.save();
    }

    /// The way to the corpse from `here` in landcell `cell`, as [`Waypoint::way_from`] gives it,
    /// while it can be shown: outside a dungeon for a corpse outside one, and in its dungeon's
    /// landblock (near it, when either landblock is not known) for one in a dungeon.
    #[must_use]
    pub fn way(&self, here: (f64, f64), in_dungeon: bool, cell: Option<u32>) -> Option<(f32, f64)> {
        let c = self.active.filter(|c| c.in_dungeon == in_dungeon)?;
        let (bearing, metres) = place(&c).way_from(here);
        let shown = !in_dungeon
            || match (c.cell, cell) {
                (Some(a), Some(b)) => a >> 16 == b >> 16,
                _ => metres <= DUNGEON_METRES,
            };
        shown.then_some((bearing, metres))
    }

    /// The waypoint to the corpse of the character called `name`; none for one in a dungeon.
    #[must_use]
    pub fn waypoint(&self, name: &str) -> Option<Waypoint> {
        let c = self.active.filter(|c| !c.in_dungeon)?;
        Some(Waypoint {
            name: Some(corpse_name(name)),
            ..place(&c)
        })
    }

    /// Whether `waypoint` leads to the corpse already.
    #[must_use]
    pub fn is_followed(&self, waypoint: Option<&Waypoint>) -> bool {
        match (self.active.filter(|c| !c.in_dungeon), waypoint) {
            (Some(c), Some(w)) => w.way_from((c.ns, c.ew)).1 <= crate::ui::waypoint::ARRIVED_METRES,
            _ => false,
        }
    }

    fn save(&mut self) {
        // A death not yet settled leaves the file's last outside death as it was.
        if self.pending.is_none() && self.outside.is_some() {
            self.recorded = self.outside;
        }
        let Some(path) = &self.path else {
            return;
        };
        let mut text = self.active.as_ref().map(to_text).unwrap_or_default();
        if let Some(o) = &self.recorded {
            text.push_str(&outside_text(o));
        }
        let done = if !text.is_empty() {
            std::fs::write(path, text)
        } else if path.exists() {
            std::fs::remove_file(path)
        } else {
            Ok(())
        };
        if let Err(e) = done {
            tracing::warn!("the corpse marker was not kept: {e}");
        }
    }
}

/// Whether `l` came into the world this frame: an id not about last frame, or about elsewhere.
fn is_new(l: &Lying, last: &[Lying]) -> bool {
    last.iter()
        .find(|o| o.id == l.id)
        .is_none_or(|o| Corpse::outside(o.place).metres_from(at(&l.place)) > MOVED_METRES)
}

/// Whether the object `id`, about last frame, left the world this frame, outside a teleport.
fn vanished(id: ObjectId, seen: &Seen, last: &[Lying]) -> bool {
    !seen.teleporting && last.iter().any(|l| l.id == id) && !seen.lying.iter().any(|l| l.id == id)
}

/// Whether two of the game's last outside deaths are one.
fn same_place(a: Place, b: Place) -> bool {
    a.cell == b.cell && Corpse::outside(a).metres_from(at(&b)) < 0.5
}

/// `p` as `(ns, ew)`.
const fn at(p: &Place) -> (f64, f64) {
    (p.ns, p.ew)
}

/// The corpse as a place with no name.
const fn place(c: &Corpse) -> Waypoint {
    Waypoint {
        ns: c.ns,
        ew: c.ew,
        name: None,
    }
}

/// The file, in the settings folder, the marker kept for `who` is in.
#[must_use]
pub fn file_name(who: &str) -> String {
    let safe: String = who
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("horizon-corpse-{safe}.txt")
}

/// A marker's lines in its file: `ns=`, `ew=`, `dungeon=` and, when known, `cell=`.
#[must_use]
pub fn to_text(c: &Corpse) -> String {
    let mut text = format!(
        "ns={}\new={}\ndungeon={}\n",
        c.ns,
        c.ew,
        u8::from(c.in_dungeon)
    );
    if let Some(cell) = c.cell {
        text.push_str(&format!("cell={cell:08X}\n"));
    }
    text
}

/// The last outside death's line in the file: `outside=` its landcell, north and east.
#[must_use]
pub fn outside_text(p: &Place) -> String {
    format!("outside={:08X} {} {}\n", p.cell, p.ns, p.ew)
}

/// The corpse a file holds; `None` for one that names no place.
#[must_use]
pub fn parse(text: &str) -> Option<Corpse> {
    let (mut ns, mut ew, mut in_dungeon, mut cell) = (None, None, false, None);
    for line in text.lines() {
        match line.split_once('=') {
            Some(("ns", v)) => ns = finite(v),
            Some(("ew", v)) => ew = finite(v),
            Some(("dungeon", v)) => in_dungeon = v.trim() == "1",
            Some(("cell", v)) => cell = u32::from_str_radix(v.trim(), 16).ok(),
            _ => {}
        }
    }
    Some(Corpse {
        ns: ns?,
        ew: ew?,
        in_dungeon,
        cell,
    })
}

/// The last outside death a file holds.
#[must_use]
pub fn parse_outside(text: &str) -> Option<Place> {
    let v = text.lines().find_map(|l| l.strip_prefix("outside="))?;
    let mut parts = v.split_whitespace();
    Some(Place {
        cell: u32::from_str_radix(parts.next()?, 16).ok()?,
        ns: finite(parts.next()?)?,
        ew: finite(parts.next()?)?,
    })
}

fn finite(v: &str) -> Option<f64> {
    v.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

/// The marker's picture: a pale headstone with a cross on it, in a dark edge, its foot at the
/// middle of the bottom edge.
#[must_use]
pub fn mark_image() -> Image {
    // A round top over straight sides, as points about the stone's edge.
    let mut shape: Vec<(f32, f32)> = vec![(48.0, 61.0), (16.0, 61.0)];
    for i in 0..=12 {
        #[allow(clippy::cast_precision_loss)] // LINT-OK: a dozen steps.
        let a = std::f32::consts::PI * (1.0 - i as f32 / 12.0);
        shape.push((
            32.0 + 16.0 * pyreal_primitives::num::math::cosf(a),
            22.0 - 16.0 * pyreal_primitives::num::math::sinf(a),
        ));
    }
    let side = MARK_SIDE as usize;
    let mut bgra = vec![0; side * side * 4];
    for (i, px) in bgra.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        #[allow(clippy::cast_precision_loss)] // LINT-OK: within the picture's few pixels.
        let (x, y) = ((i % side) as f32 + 0.5, (i / side) as f32 + 0.5);
        let d = edge_distance(&shape, x, y);
        let in_cross = ((29.0..35.0).contains(&x) && (18.0..48.0).contains(&y))
            || ((22.0..42.0).contains(&x) && (25.0..31.0).contains(&y));
        let face = if in_cross { CROSS } else { STONE };
        let fill = ((-d - 3.5) + 0.5).clamp(0.0, 1.0);
        let cover = (0.5 - d).clamp(0.0, 1.0);
        let edge = crate::ui::waypoint::EDGE;
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
            mix(edge[2], face[2]),
            mix(edge[1], face[1]),
            mix(edge[0], face[0]),
            (cover * 255.0) as u8,
        ]);
    }
    Image {
        width: MARK_SIDE,
        height: MARK_SIDE,
        bgra,
    }
}

/// The stone's foot in its picture, as a share of its side from the top.
pub const MARK_FOOT: f32 = 61.0 / 64.0;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::waypoint::METRES_PER_UNIT;

    fn units(metres: f64) -> f64 {
        metres / METRES_PER_UNIT
    }

    fn seen(deaths: i32, here: (f64, f64)) -> Seen {
        Seen {
            deaths: Some(deaths),
            here: Some(here),
            ..Seen::default()
        }
    }

    /// The death cost something.
    fn lost(s: Seen) -> Seen {
        Seen {
            kept: Some(false),
            ..s
        }
    }

    /// A landcell on the land about `(ns, ew)`, and the place there.
    fn outside(ns: f64, ew: f64) -> Place {
        let (bx, x) = split(ew);
        let (by, y) = split(ns);
        let p = Place::at((bx << 24) | (by << 16) | 0x0001, x, y);
        assert!((p.ns - ns).abs() < 1e-3 && (p.ew - ew).abs() < 1e-3);
        p
    }

    fn split(coord: f64) -> (u32, f32) {
        let m = (coord + 101.95) * 240.0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let block = (m / 192.0) as u32;
        #[allow(clippy::cast_possible_truncation)]
        let x = (m - f64::from(block) * 192.0) as f32;
        (block, x)
    }

    fn lying(id: u32, place: Place) -> Lying {
        Lying {
            id: ObjectId(id),
            place,
        }
    }

    /// The marker, with a death at `at` marked and its corpse object `0x8000_0001` lying there.
    fn marked(at: (f64, f64)) -> CorpseMarker {
        let mut m = CorpseMarker::default();
        m.follow(&seen(1, at), 0.0);
        let corpse = vec![lying(0x8000_0001, outside(at.0, at.1))];
        let s = Seen {
            lying: corpse,
            ..lost(seen(2, at))
        };
        assert_eq!(m.follow(&s, 1.0), Some(CorpseNews::Marked));
        assert_eq!(m.tracked, Some(ObjectId(0x8000_0001)));
        m
    }

    #[test]
    fn a_death_is_marked_once_it_cost_something_and_the_first_count_seen_is_no_death() {
        let mut m = CorpseMarker::default();
        assert_eq!(m.follow(&seen(4, (42.0, 33.0)), 0.0), None, "logging in");
        assert_eq!(m.follow(&seen(5, (42.1, 33.2)), 1.0), None, "not said yet");
        assert_eq!(m.active, None);
        assert_eq!(
            m.follow(&lost(seen(5, (42.1, 33.2))), 3.0),
            Some(CorpseNews::Marked)
        );
        assert_eq!(
            m.active,
            Some(Corpse {
                ns: 42.1,
                ew: 33.2,
                in_dungeon: false,
                cell: None,
            })
        );
        let w = m.waypoint("Ardent").unwrap();
        assert_eq!(w.label(), "Corpse of Ardent");
        assert!(m.is_followed(Some(&w)));
        // Unsaid, the place is marked after a while all the same.
        m.forget();
        m.follow(&seen(6, (10.0, 10.0)), 10.0);
        assert_eq!(m.follow(&seen(6, (10.0, 10.0)), 29.0), None);
        assert_eq!(
            m.follow(&seen(6, (10.0, 10.0)), 30.0),
            Some(CorpseNews::Marked)
        );
    }

    #[test]
    fn a_death_mid_recall_is_marked_where_the_game_lays_the_corpse() {
        // Mid-recall: the count rises with the player at the lifestone, far from the corpse.
        let stone = (12.0, 30.0);
        let fell = outside(42.25, 33.5);
        let mut m = CorpseMarker::default();
        let mut s = Seen {
            last_outside: Some(outside(1.0, 1.0)),
            ..seen(1, stone)
        };
        m.follow(&s, 0.0);
        s.deaths = Some(2);
        m.follow(&s, 1.0);
        // The game's last outside death, then the corpse itself, come just after.
        s.last_outside = Some(fell);
        m.follow(&s, 2.0);
        assert_eq!(m.active, None);
        s.lying = vec![lying(0x8000_0002, fell)];
        assert_eq!(m.follow(&lost(s.clone()), 3.0), Some(CorpseNews::Marked));
        let c = m.active.unwrap();
        assert!(
            (c.ns - 42.25).abs() < 1e-3 && (c.ew - 33.5).abs() < 1e-3,
            "{c:?}"
        );
        assert!(!c.in_dungeon);
        assert_eq!(m.tracked, Some(ObjectId(0x8000_0002)));

        // In a dungeon there is no outside death: the corpse object gives the place.
        let deep = Place {
            ns: 1.0,
            ew: 90.0,
            cell: 0x0160_0123,
        };
        let mut m = CorpseMarker::default();
        let here = Seen {
            cell: Some(0x0170_0110),
            in_dungeon: true,
            ..seen(1, (1.5, 91.0))
        };
        m.follow(&here, 0.0);
        m.follow(
            &Seen {
                deaths: Some(2),
                ..here.clone()
            },
            1.0,
        );
        let come = Seen {
            deaths: Some(2),
            lying: vec![lying(0x8000_0003, deep)],
            kept: Some(false),
            ..here
        };
        assert_eq!(m.follow(&come, 2.0), Some(CorpseNews::Marked));
        assert_eq!(
            m.active,
            Some(Corpse {
                ns: 1.0,
                ew: 90.0,
                in_dungeon: true,
                cell: Some(0x0160_0123),
            })
        );
    }

    #[test]
    fn a_death_that_kept_every_item_marks_nothing_and_leaves_the_old_corpse_marked() {
        let at = (42.0, 33.0);
        let mut m = marked(at);
        let old = m.active;
        m.follow(&seen(3, (10.0, 10.0)), 50.0);
        let kept = Seen {
            kept: Some(true),
            ..seen(3, (10.0, 10.0))
        };
        assert_eq!(m.follow(&kept, 52.0), None);
        assert_eq!(m.follow(&seen(3, (10.0, 10.0)), 100.0), None, "settled");
        assert_eq!(m.active, old);
    }

    #[test]
    fn an_empty_corpse_rotting_at_once_marks_nothing() {
        let at = (42.0, 33.0);
        let stone = (at.0 + units(40.0), at.1);
        let mut m = CorpseMarker::default();
        m.follow(&seen(1, at), 0.0);
        m.follow(&seen(2, at), 1.0);
        let corpse = Seen {
            lying: vec![lying(0x8000_0004, outside(at.0, at.1))],
            ..seen(2, stone)
        };
        m.follow(&corpse, 4.0);
        m.follow(&corpse, 10.0);
        // Gone fifteen seconds on, in sight of the lifestone.
        assert_eq!(m.follow(&seen(2, stone), 19.0), None);
        assert_eq!(m.follow(&seen(2, stone), 40.0), None);
        assert_eq!(m.active, None);
    }

    #[test]
    fn a_corpse_vanishing_close_by_decayed_and_one_vanishing_far_off_left_sight() {
        let at = (42.0, 33.0);
        let spot = outside(at.0, at.1);
        let there = |here: (f64, f64)| Seen {
            lying: vec![lying(0x8000_0001, spot)],
            ..seen(2, here)
        };
        // Walked off 150 m: it is out of sight, not gone.
        let mut m = marked(at);
        let far = (at.0 + units(150.0), at.1);
        m.follow(&there(far), 600.0);
        assert_eq!(m.follow(&seen(2, far), 601.0), None);
        assert!(m.active.is_some());
        // Back by it, it vanishes before the player's eyes.
        let by = (at.0 + units(6.0), at.1);
        m.follow(&there(by), 700.0);
        assert_eq!(m.follow(&seen(2, by), 701.0), Some(CorpseNews::Gone));
        assert_eq!(m.active, None);

        // Its id come back at another place is another object: that one's going is nothing.
        let mut m = marked(at);
        m.follow(&there(by), 600.0);
        let elsewhere = Seen {
            lying: vec![lying(0x8000_0001, outside(at.0 + units(5.0), at.1))],
            ..seen(2, by)
        };
        m.follow(&elsewhere, 601.0);
        assert_eq!(m.tracked, None);
        m.follow(&elsewhere, 602.0);
        assert_eq!(m.tracked, None, "not taken up again");
        let kept = Seen {
            corpse_metres: Some(1.0),
            ..seen(2, by)
        };
        assert_eq!(m.follow(&kept, 603.0), None);
        assert!(m.active.is_some());
    }

    #[test]
    fn the_marker_clears_when_the_corpse_is_emptied_or_is_gone_or_by_hand() {
        let at = (42.0, 33.0);
        let mut m = CorpseMarker::default();
        m.follow(&seen(1, at), 0.0);
        m.follow(&lost(seen(2, at)), 1.0);
        assert!(m.active.is_some());
        // Standing by it while it is laid, then away from it after the grace: kept.
        assert_eq!(m.follow(&seen(2, at), 2.0), None);
        assert_eq!(m.follow(&seen(2, (40.0, 33.0)), 100.0), None);
        // Opened as it arrives empty, then filled, then emptied: recovered.
        let open = |n| Seen {
            opened: Some(n),
            corpse_metres: Some(1.0),
            ..seen(2, at)
        };
        assert_eq!(m.follow(&open(0), 101.0), None, "its items not come yet");
        assert_eq!(m.follow(&open(3), 102.0), None);
        assert_eq!(m.follow(&open(0), 103.0), Some(CorpseNews::Recovered));
        assert_eq!(m.active, None);

        // Back by the place with no corpse there for five seconds: it decayed.
        m.follow(&lost(seen(3, at)), 200.0);
        let by = (at.0 + units(8.0), at.1);
        assert_eq!(m.follow(&seen(3, by), 300.0), None);
        assert_eq!(m.follow(&seen(3, by), 304.0), None);
        assert_eq!(m.follow(&seen(3, by), 305.0), Some(CorpseNews::Gone));
        // While the corpse is there, it is kept.
        m.follow(&lost(seen(4, at)), 400.0);
        let there = Seen {
            corpse_metres: Some(3.0),
            ..seen(4, by)
        };
        for t in [500.0, 510.0, 520.0] {
            assert_eq!(m.follow(&there, t), None);
        }
        assert!(m.active.is_some());
        m.forget();
        assert_eq!(m.active, None);
    }

    #[test]
    fn a_dungeon_s_corpse_is_shown_only_in_its_landblock_and_has_no_waypoint() {
        let mut m = CorpseMarker::default();
        let down = |deaths, here, cell| Seen {
            in_dungeon: true,
            cell: Some(cell),
            kept: Some(false),
            ..seen(deaths, here)
        };
        m.follow(&down(1, (1.0, 90.0), 0x0160_0110), 0.0);
        m.follow(&down(2, (1.0, 90.0), 0x0160_0110), 1.0);
        assert!(m.active.is_some_and(|c| c.in_dungeon));
        assert_eq!(m.waypoint("Ardent"), None);
        assert!(m
            .way((1.0 + units(250.0), 90.0), true, Some(0x0160_0150))
            .is_some());
        assert!(
            m.way((1.0, 90.0), false, Some(0x0160_0001)).is_none(),
            "outdoors"
        );
        assert!(
            m.way((1.0 + units(50.0), 90.0), true, Some(0x0161_0100))
                .is_none(),
            "the next landblock is another dungeon"
        );
        assert!(m.way((1.0 + units(50.0), 90.0), true, None).is_some());
        assert!(m.way((1.0 + units(500.0), 90.0), true, None).is_none());
    }

    #[test]
    fn a_marker_is_kept_per_character_and_a_death_not_seen_is_marked_at_login() {
        let dir = std::env::temp_dir().join(format!("pyreal-corpse-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let who = "127.0.0.1:9000|Ardent";
        let first = outside(12.3, -45.6);
        let mut m = CorpseMarker::default();
        m.load_for(Some(&dir), who);
        let s = Seen {
            last_outside: Some(first),
            ..lost(seen(7, (12.3, -45.6)))
        };
        m.follow(&s, 0.0);
        m.follow(
            &Seen {
                deaths: Some(8),
                ..s.clone()
            },
            1.0,
        );
        let mut other = CorpseMarker::default();
        other.load_for(Some(&dir), "127.0.0.1:9000|Bram");
        assert_eq!(other.active, None);
        let mut again = CorpseMarker::default();
        again.load_for(Some(&dir), who);
        assert_eq!(again.active, m.active);
        // The count is learnt anew after loading: no death is read into it.
        assert_eq!(again.follow(&s, 2.0), None);
        // Recovered, it stays forgotten at the next login: the outside death is the same.
        again.forget();
        let mut back = CorpseMarker::default();
        back.load_for(Some(&dir), who);
        assert_eq!(back.follow(&s, 0.0), None);
        assert_eq!(back.active, None);
        // A death elsewhere since, as another client saw it, is marked at login.
        let since = outside(-20.0, 60.0);
        let mut back = CorpseMarker::default();
        back.load_for(Some(&dir), who);
        let login = Seen {
            last_outside: Some(since),
            ..seen(9, (0.0, 0.0))
        };
        assert_eq!(back.follow(&login, 0.0), Some(CorpseNews::Marked));
        assert_eq!(back.active, Some(Corpse::outside(since)));
        back.forget();
        back.out_of_world();
        let mut gone = CorpseMarker::default();
        gone.load_for(Some(&dir), who);
        assert_eq!(gone.follow(&login, 0.0), None, "recorded once forgotten");
        std::fs::remove_dir_all(&dir).ok();
    }
}
