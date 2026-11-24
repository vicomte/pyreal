//! The map window: the world's picture, zoomed with the wheel or its buttons and panned by dragging.
//!
//! The picture draws three of its pixels to a landcell, 6123 to a side. It is drawn from the
//! landblocks of the data the client plays with ([`pyreal_scene::world_map`]): the world is read
//! and the picture of the whole at a third of that size ([`OVERVIEW`]) drawn in the background as
//! the player enters the world ([`MapView::preload`]), and kept until the data changes. Tiles of it
//! at its own size ([`TILE`]) are drawn as a view first needs them, one a frame, and let go when no
//! view has needed them for a while or the map closes.
//!
//! The picture names nothing: the places are drawn over it, the world's own when its server
//! publishes them ([`GameState::map_places`], a place store, [`drawn_places`]) and otherwise those
//! built in for the world's era ([`places_file`]). Players see the major towns, each a [`town_icon`] with its name, and the
//! houses as the map is zoomed in far enough to tell them apart; an administrator also sees every
//! other kind, and picks which with the Places button.

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use pyreal_client_contract::panels::map::{MapButton, MapClick};
use pyreal_navigation::{MarkKind, PlaceKind as KnownKind, PlaceStore};
use pyreal_primitives::num::math::hypotf;
use pyreal_primitives::Position;
use pyreal_scene::world_map::{Built, MapBuild, Pixels, WorldMap};
use pyreal_ui_logic::panels::map as graphic;

use super::Windows;
use crate::art::{AcArt as _, Art, Family, Image, Sprite};
use crate::draw::{Rect, WHITE};
use crate::ui::game::GameState;
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Painter, TextStyle};
use crate::ui::Outcome;

/// The picture's side, in its own pixels.
#[allow(clippy::cast_precision_loss)] // LINT-OK: 6123.
pub const MAP_SIZE: f32 = pyreal_scene::world_map::SIDE as f32;
/// The side of the picture of the whole.
#[allow(clippy::cast_precision_loss)] // LINT-OK: 2048.
const OVERVIEW: f32 = pyreal_scene::world_map::OVERVIEW as f32;
/// What the picture of the whole is kept under.
pub const OVERVIEW_KEY: &str = "map.overview";
/// A tile's side, in the picture's own pixels.
#[allow(clippy::cast_precision_loss)] // LINT-OK: 1024.
const TILE: f32 = pyreal_scene::world_map::TILE as f32;
/// How many tiles make a side.
#[allow(clippy::cast_possible_truncation)] // LINT-OK: 6.
const TILES: u32 = pyreal_scene::world_map::TILES as u32;
/// How far past its own size the picture of the whole is drawn before the tiles take over.
const OVERVIEW_STRETCH: f32 = 1.3;
/// How long a tile no view needs is kept, in seconds.
const TILE_KEPT: f64 = 30.0;

/// The picture's pixels to a unit of the game's coordinates: ten landcells of three pixels.
const PER_UNIT: f64 = 30.0;
/// Where 0E and 0N fall on the picture: landcell vertex `v` is drawn as the three pixels from `3v`,
/// and stands at `v / 10 - 101.95` on either axis.
const ORIGIN: (f64, f64) = (3060.0, 3063.0);
/// The west and south edge of the landscape, then its east and north edge.
const WORLD: (f64, f64) = (-101.95, 102.05);

/// How much a notch of the wheel zooms.
const WHEEL_STEP: f32 = 1.25;
/// How much a press of a zoom button zooms.
const BUTTON_STEP: f32 = 1.5;
/// How many screen pixels a picture pixel is drawn at, at the most.
const MOST_MAGNIFIED: f32 = 3.0;
/// The places built in for each era, by the era's name.
pub const PLACES_FILES: [&str; 3] = [
    "map-places-launch.tsv",
    "map-places-infiltration.tsv",
    "map-places-eor.tsv",
];
/// A town's icon, on the screen at this many layout units a side.
const TOWN_UNITS: f32 = 16.0;
/// The town icon's picture's side, in its pixels.
const TOWN_SIDE: u32 = 64;
/// The zoom from which portals' and dungeons' names are written beside their marks.
const PORTAL_NAMES_FROM: f32 = 4.0;
/// The zoom from which spawn areas' names are written beside their marks.
const SPAWN_NAMES_FROM: f32 = 2.0;
/// A row of the Places list, in layout units.
const KIND_ROW: f32 = 20.0;
/// The Places list's width, in layout units.
const KIND_LIST_W: f32 = 150.0;
/// How near the pointer, in layout units, a mark answers it.
const MARK_REACH: f32 = 6.0;
/// A mark's side, in layout units.
const MARK_SIDE: f32 = 5.0;
/// How far, in layout units, the pointer may move between a press and its release for a click.
const CLICK_SLOP: f32 = 4.0;
/// How long, in seconds, explore mode waits after a click before it moves there, so a double
/// click sets a waypoint and goes nowhere.
const GO_AFTER: f64 = 0.5;
/// What a click on the picture offers at the place clicked, and a click on a link's arrow there.
pub const CREATE_WAYPOINT: &str = "Create Waypoint";
/// What puts the waypoint's place on the chat line.
pub const SHARE_WAYPOINT: &str = "Share Waypoint";

/// The zoom the map opens at to show a journal page's place.
const JOURNAL_ZOOM: f32 = 2.5;
/// A journal page's mark's side, in layout units, and its colour.
const JOURNAL_MARK: f32 = 9.0;
const JOURNAL_COLOUR: u32 = 0xFFD9_C27A;
/// The waypoint's marker, on the screen at this many layout units a side.
const WAYPOINT_UNITS: f32 = 26.0;
/// The corpse's marker, on the screen at this many layout units a side.
const CORPSE_UNITS: f32 = 22.0;
/// What sets the waypoint to the corpse, here and under the radar.
pub const SET_WAYPOINT_TO_CORPSE: &str = "Set Waypoint to Corpse";

/// An administrator's arrow, on the screen at this many layout units tall.
const ARROW_UNITS: f32 = 30.0;
/// The arrow's picture's side, in its pixels.
const ARROW_SIDE: u32 = 64;
/// A link's arrow, on the screen at this many layout units a side, what it is kept under and its
/// fill: blue, green and red.
const PIN_UNITS: f32 = 26.0;
const PIN_KEY: &str = "map.pin";
const PIN_FILL: [f32; 3] = [240.0, 200.0, 60.0];
/// An administrator's arrow's fill.
const ADMIN_FILL: [f32; 3] = [32.0, 32.0, 224.0];
/// The arrow's corners in its picture, from its tip, which points straight down at the middle of
/// the bottom edge but for a margin for its edge's filter.
const ARROW: [(f32, f32); 4] = [(32.0, 62.0), (10.0, 4.0), (32.0, 18.0), (54.0, 4.0)];

/// The pixel of the picture that `ns`, `ew` falls on.
#[must_use]
pub fn map_pixel(ns: f64, ew: f64) -> (f32, f32) {
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: within the picture's few thousand.
    (
        (ORIGIN.0 + PER_UNIT * ew) as f32,
        (ORIGIN.1 - PER_UNIT * ns) as f32,
    )
}

/// The place `(ns, ew)` at pixel `(x, y)` of the picture; `None` off the landscape.
#[must_use]
pub fn map_place(x: f32, y: f32) -> Option<(f64, f64)> {
    let ew = (f64::from(x) - ORIGIN.0) / PER_UNIT;
    let ns = (ORIGIN.1 - f64::from(y)) / PER_UNIT;
    let on = |v: f64| (WORLD.0..WORLD.1).contains(&v);
    (on(ns) && on(ew)).then_some((ns, ew))
}

/// What a place on the map is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaceKind {
    /// A major town: the retail map draws its town's icon.
    Town,
    /// A smaller place the retail map marks with a cross, or a region it names.
    Landmark,
    Cottage,
    Villa,
    Mansion,
    Lifestone,
    /// A portal on the land into a dungeon.
    Dungeon,
    /// A portal on the land to elsewhere on the land.
    Portal,
    /// Where a portal on the land leads.
    Drop,
    MeetingHall,
    /// A portal into a block of apartments.
    Apartments,
    Vendor,
    /// Someone with something to say.
    Npc,
    /// Someone who takes items: where quests are turned in.
    Collector,
    Bindstone,
    /// Landblocks where the same region's creatures spawn.
    Spawn,
    /// Generators and creatures placed outdoors.
    Camp,
}

impl PlaceKind {
    /// Every kind, in the order the Places list shows them.
    pub const ALL: [Self; 17] = [
        Self::Town,
        Self::Landmark,
        Self::Cottage,
        Self::Villa,
        Self::Mansion,
        Self::Lifestone,
        Self::Dungeon,
        Self::Portal,
        Self::Drop,
        Self::MeetingHall,
        Self::Apartments,
        Self::Vendor,
        Self::Npc,
        Self::Collector,
        Self::Bindstone,
        Self::Spawn,
        Self::Camp,
    ];

    /// The kind a places document names `name`; `None` for one this client does not know.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }

    /// Its name in a places document.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Town => "town",
            Self::Landmark => "landmark",
            Self::Cottage => "cottage",
            Self::Villa => "villa",
            Self::Mansion => "mansion",
            Self::Lifestone => "lifestone",
            Self::Dungeon => "dungeon",
            Self::Portal => "portal",
            Self::Drop => "drop",
            Self::MeetingHall => "meeting_hall",
            Self::Apartments => "apartments",
            Self::Vendor => "vendor",
            Self::Npc => "npc",
            Self::Collector => "collector",
            Self::Bindstone => "bindstone",
            Self::Spawn => "spawn",
            Self::Camp => "camp",
        }
    }

    /// Its name in the Places list.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Town => "Towns",
            Self::Landmark => "Landmarks",
            Self::Cottage => "Cottages",
            Self::Villa => "Villas",
            Self::Mansion => "Mansions",
            Self::Lifestone => "Lifestones",
            Self::Dungeon => "Dungeons",
            Self::Portal => "Portals",
            Self::Drop => "Portal drops",
            Self::MeetingHall => "Meeting halls",
            Self::Apartments => "Apartments",
            Self::Vendor => "Vendors",
            Self::Npc => "People",
            Self::Collector => "Collectors",
            Self::Bindstone => "Bindstones",
            Self::Spawn => "Spawn areas",
            Self::Camp => "Camps",
        }
    }

    /// The zoom from which the map marks it.
    #[must_use]
    pub const fn marked_from(self) -> f32 {
        match self {
            Self::Town | Self::Landmark => 1.0,
            Self::Spawn => 1.5,
            Self::Portal | Self::Lifestone | Self::Dungeon => 2.0,
            Self::MeetingHall | Self::Apartments | Self::Bindstone => 3.0,
            Self::Mansion | Self::Drop | Self::Vendor | Self::Collector | Self::Camp => 4.0,
            Self::Villa | Self::Cottage | Self::Npc => 6.0,
        }
    }

    /// Whether a player sees it, or only an administrator.
    #[must_use]
    pub const fn players_see(self) -> bool {
        matches!(
            self,
            Self::Town | Self::Cottage | Self::Villa | Self::Mansion
        )
    }

    /// Its mark's colour.
    #[must_use]
    pub const fn colour(self) -> u32 {
        match self {
            Self::Town | Self::Landmark => 0xFFF0_C860,
            Self::Portal => 0xFFB0_70E8,
            Self::Lifestone => 0xFF80_E8FF,
            Self::Cottage => 0xFF40_B050,
            Self::Villa => 0xFF40_70E0,
            Self::Mansion => 0xFFE0_4040,
            Self::Dungeon => 0xFFE0_8830,
            Self::Drop => 0xFFE8_E8E8,
            Self::MeetingHall => 0xFFF0_90C8,
            Self::Apartments => 0xFF50_B0B0,
            Self::Vendor => 0xFFFF_E040,
            Self::Npc => 0xFF90_D8A8,
            Self::Collector => 0xFFFF_A080,
            Self::Bindstone => 0xFF98_98FF,
            Self::Spawn => 0xFFC0_2870,
            Self::Camp => 0xFF90_4818,
        }
    }

    /// Whether its name is written beside its mark at `zoom`.
    #[must_use]
    pub fn named_at(self, zoom: f32) -> bool {
        match self {
            Self::Landmark => true,
            Self::Spawn => zoom >= SPAWN_NAMES_FROM,
            Self::Portal | Self::Dungeon => zoom >= PORTAL_NAMES_FROM,
            _ => false,
        }
    }
}

/// A place the map marks: what it is, its name, where it stands and a line more about it.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub kind: PlaceKind,
    pub name: String,
    pub ns: f64,
    pub ew: f64,
    /// Where a portal leads, what a dungeon holds, a spawn area's levels: a sentence a line.
    pub detail: Option<String>,
}

impl PlaceKind {
    /// The kind a world's map mark is.
    #[must_use]
    pub const fn of_mark(kind: MarkKind) -> Self {
        match kind {
            MarkKind::Town => Self::Town,
            MarkKind::Landmark => Self::Landmark,
            MarkKind::Cottage => Self::Cottage,
            MarkKind::Villa => Self::Villa,
            MarkKind::Mansion => Self::Mansion,
            MarkKind::Lifestone => Self::Lifestone,
            MarkKind::Dungeon => Self::Dungeon,
            MarkKind::Portal => Self::Portal,
            MarkKind::Drop => Self::Drop,
            MarkKind::MeetingHall => Self::MeetingHall,
            MarkKind::Apartments => Self::Apartments,
            MarkKind::Vendor => Self::Vendor,
            MarkKind::Npc => Self::Npc,
            MarkKind::Collector => Self::Collector,
            MarkKind::Bindstone => Self::Bindstone,
            MarkKind::Spawn => Self::Spawn,
            MarkKind::Camp => Self::Camp,
        }
    }
}

/// The game's coordinates `(ns, ew)` of `at`: its landblock and its spot in it.
#[must_use]
pub fn spot(at: &Position) -> (f64, f64) {
    spot_in(at.cell.0, at.frame.origin.x, at.frame.origin.y)
}

/// The game's coordinates `(ns, ew)` of `x`, `y` metres into the landblock of `cell`.
#[must_use]
pub fn spot_in(cell: u32, x: f32, y: f32) -> (f64, f64) {
    let unit = |block: u32, m: f32| (f64::from(block) * 192.0 + f64::from(m)) / 240.0 - 101.95;
    let (bx, by) = (cell >> 24, (cell >> 16) & 0xFF);
    (unit(by, y), unit(bx, x))
}

/// A place store's map marks as places; none from text that is no place store. The built-in
/// files are such stores, of marks alone.
#[must_use]
pub fn read_places(bytes: &[u8]) -> Vec<Place> {
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|t| PlaceStore::from_text(t).ok())
        .map(|store| marks(&store))
        .unwrap_or_default()
}

fn marks(store: &PlaceStore) -> Vec<Place> {
    store
        .marks()
        .iter()
        .map(|m| {
            let (ns, ew) = spot(&m.at);
            Place {
                kind: PlaceKind::of_mark(m.kind),
                name: m.name.clone(),
                ns,
                ew,
                detail: m.detail.clone(),
            }
        })
        .collect()
}

/// The kinds a world's own places stand for when its server publishes no marks.
const SERVED_KINDS: [PlaceKind; 9] = [
    PlaceKind::Lifestone,
    PlaceKind::Dungeon,
    PlaceKind::Portal,
    PlaceKind::Drop,
    PlaceKind::MeetingHall,
    PlaceKind::Apartments,
    PlaceKind::Vendor,
    PlaceKind::Npc,
    PlaceKind::Collector,
];

/// How near, in the game's coordinates, a built-in place stands to a world's own for the two to
/// be the same: a few metres.
const SAME_SPOT: f64 = 0.02;

/// The places the map draws for a world whose server published `served` (a place store), over
/// `built_in`, those built in for its era:
///
/// - its marks, when it published them (version 2), and those alone;
/// - else its lifestones, vendors, people and portals in place of the built-in ones. What stands
///   in a dungeon is left out (a room in a landblock with nothing outdoors and nothing built in),
///   a portal into a dungeon in another landblock is that dungeon's entrance, and a portal across
///   the land marks where it leads. A built-in place of a kind they stand for at the same spot gives its kind
///   (a collector, a meeting hall) and its detail. The other kinds are the built-in ones;
/// - else, or for text that is no place store, the built-in places.
#[must_use]
pub fn drawn_places(served: Option<&str>, built_in: Vec<Place>) -> Vec<Place> {
    let Some(store) = served.and_then(|t| PlaceStore::from_text(t).ok()) else {
        return built_in;
    };
    if !store.marks().is_empty() {
        return marks(&store);
    }
    if store.places().is_empty() {
        return built_in;
    }
    let outdoors = |at: &Position| at.cell.0 & 0xFFFF < 0x100;
    let block = |at: &Position| at.cell.0 >> 16;
    // A landblock with anything of the world's outdoors, or anything built in for its era, has
    // land; a room in one is a building's.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // LINT-OK: 0 to 255.
    let block_at = |v: f64| (((v + 101.95) * 240.0 / 192.0).floor().clamp(0.0, 255.0)) as u32;
    let land: std::collections::HashSet<u32> = store
        .places()
        .iter()
        .filter(|p| outdoors(&p.at))
        .map(|p| block(&p.at))
        .chain(
            built_in
                .iter()
                .map(|p| block_at(p.ew) << 8 | block_at(p.ns)),
        )
        .collect();
    // The built-in places a world's own stand for, by the square of the map they are in.
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: map coordinates are about ±100.
    let square = |(ns, ew): (f64, f64)| ((ns * 10.0).floor() as i32, (ew * 10.0).floor() as i32);
    let (stood_for, mut out): (Vec<Place>, Vec<Place>) = built_in
        .into_iter()
        .partition(|p| SERVED_KINDS.contains(&p.kind));
    let mut near: HashMap<(i32, i32), Vec<&Place>> = HashMap::new();
    for p in &stood_for {
        near.entry(square((p.ns, p.ew))).or_default().push(p);
    }
    let same = |(ns, ew): (f64, f64), kinds: &[PlaceKind]| {
        let (a, b) = square((ns, ew));
        (a - 1..=a + 1)
            .flat_map(|i| (b - 1..=b + 1).map(move |j| (i, j)))
            .filter_map(|k| near.get(&k))
            .flatten()
            .find(|p| kinds.contains(&p.kind) && hypot(p.ns - ns, p.ew - ew) <= SAME_SPOT)
            .copied()
    };
    let mut drops: Vec<Place> = Vec::new();
    for p in store.places() {
        if !outdoors(&p.at) && !land.contains(&block(&p.at)) {
            continue;
        }
        let at = spot(&p.at);
        let (kind, kinds): (PlaceKind, &[PlaceKind]) = match p.kind {
            KnownKind::Lifestone => (PlaceKind::Lifestone, &[PlaceKind::Lifestone]),
            KnownKind::Vendor => (PlaceKind::Vendor, &[PlaceKind::Vendor]),
            KnownKind::Npc => (PlaceKind::Npc, &[PlaceKind::Npc, PlaceKind::Collector]),
            KnownKind::Portal => match &p.to {
                Some(to) if !outdoors(to) && block(to) != block(&p.at) => (
                    PlaceKind::Dungeon,
                    &[
                        PlaceKind::Dungeon,
                        PlaceKind::MeetingHall,
                        PlaceKind::Apartments,
                    ],
                ),
                _ => (
                    PlaceKind::Portal,
                    &[PlaceKind::Portal, PlaceKind::MeetingHall],
                ),
            },
            KnownKind::House => continue,
        };
        let known = same(at, kinds);
        let name = match (kind, known) {
            (PlaceKind::Dungeon, Some(k)) => k.name.clone(),
            (PlaceKind::Dungeon, None) => dungeon_name(&p.name).to_owned(),
            _ => p.name.clone(),
        };
        if kind == PlaceKind::Portal {
            if let Some(to) = p.to.as_ref().filter(|to| outdoors(to)) {
                let there = spot(to);
                let from = coords_text_f64(at);
                match drops
                    .iter_mut()
                    .find(|d| hypot(d.ns - there.0, d.ew - there.1) <= SAME_SPOT)
                {
                    Some(d) if !d.name.split(", ").any(|n| n == p.name) => {
                        d.name.push_str(", ");
                        d.name.push_str(&p.name);
                    }
                    Some(_) => {}
                    None => drops.push(Place {
                        kind: PlaceKind::Drop,
                        name: p.name.clone(),
                        ns: there.0,
                        ew: there.1,
                        detail: Some(format!("A portal at {from} leads here.")),
                    }),
                }
            }
        }
        out.push(Place {
            kind: known.map_or(kind, |k| k.kind),
            name,
            ns: at.0,
            ew: at.1,
            detail: known.and_then(|k| k.detail.clone()),
        });
    }
    out.extend(drops);
    out
}

/// A portal's name as its dungeon's: "Portal to Drudge Hideout" and "Drudge Hideout Portal" both
/// name "Drudge Hideout".
fn dungeon_name(portal: &str) -> &str {
    portal
        .strip_prefix("Portal to ")
        .or_else(|| portal.strip_suffix(" Portal"))
        .unwrap_or(portal)
}

fn hypot(a: f64, b: f64) -> f64 {
    (a * a + b * b).sqrt()
}

#[allow(clippy::cast_possible_truncation)] // LINT-OK: a coordinate within the world.
fn coords_text_f64((ns, ew): (f64, f64)) -> String {
    coords_text(ns as f32, ew as f32)
}

/// The built-in places of `era`.
#[must_use]
pub const fn places_file(era: pyreal_primitives::EraId) -> &'static str {
    match era {
        pyreal_primitives::EraId::Launch => PLACES_FILES[0],
        pyreal_primitives::EraId::Infiltration => PLACES_FILES[1],
        pyreal_primitives::EraId::Eor => PLACES_FILES[2],
    }
}

/// `ns`, `ew` as the game writes a place: `42.2N 33.8E`.
fn coords_text(ns: f32, ew: f32) -> String {
    format!(
        "{:.1}{} {:.1}{}",
        ns.abs(),
        if ns >= 0.0 { "N" } else { "S" },
        ew.abs(),
        if ew >= 0.0 { "E" } else { "W" }
    )
}

/// Where the map's picture comes from.
#[derive(Debug, Default)]
enum Picture {
    /// Not asked for yet, or let go.
    #[default]
    None,
    /// Being built from the data the art holds.
    Building(MapBuild),
    /// Built, or handed over ([`MapView::show`]).
    Ready(Arc<WorldMap>),
    /// The data held no world to draw: the plain ground until other data comes.
    Failed,
}

/// The world the map draws, kept from the first frame in the world until the data changes, and
/// over a log out and back in to the same data.
#[derive(Debug, Default)]
pub struct MapWorld {
    picture: Picture,
    /// The data it is drawn from; `None` for a picture handed over.
    data: Option<Weak<pyreal_dat::RetailDatStore>>,
    /// A picture of the whole built or handed over, given to the art when the map next draws.
    overview: Option<Pixels>,
}

/// What the places were read from: the world's era, and its own document by its address.
type PlacesFrom = (pyreal_primitives::EraId, Option<usize>);

/// Which part of the picture the map window shows.
#[derive(Debug)]
pub struct MapView {
    /// How many times the fit: 1 shows the whole picture.
    pub zoom: f32,
    /// The picture's pixel in the middle of the view.
    pub centre: (f32, f32),
    /// Where the out, in and fit buttons were drawn this frame.
    pub buttons: Option<[Rect; 3]>,
    /// What was drawn under the picture this frame, by name: the buttons, the coordinates and the
    /// journal's controls, which must never overlap.
    pub bar: Vec<(&'static str, Rect)>,
    /// Where the pointer last was in a pan under way.
    drag: Option<(f32, f32)>,
    /// Where the press that began it was.
    press: Option<(f32, f32)>,
    /// The place under the pointer this frame, `(ns, ew)`, and its name.
    pointed: Option<(f64, f64)>,
    pointed_name: Option<String>,
    /// Where explore mode goes once no second click comes, and when it was clicked.
    go_waiting: Option<((f64, f64), f64)>,
    /// The waypoint marker's tooltip, while the pointer is on it.
    waypoint_tip: Option<(String, Vec<String>)>,
    /// Where the waypoint's marker was drawn this frame.
    waypoint_mark: Option<Rect>,
    /// Where the corpse's marker was drawn this frame.
    pub corpse_mark: Option<Rect>,
    /// The arrow a coordinate link stood on the map: its place, and the name a waypoint there takes.
    pub pin: Option<((f64, f64), Option<String>)>,
    /// Where the link's arrow was drawn this frame.
    pub pin_mark: Option<Rect>,
    /// Where Create Waypoint is offered, and the name the waypoint takes.
    pub offer: Option<((f64, f64), Option<String>)>,
    /// Where Create Waypoint was drawn this frame.
    pub offer_button: Option<Rect>,
    /// The tiles drawn, and when a view last needed each.
    tiles: HashMap<(u32, u32), f64>,
    /// The places, once read, and what from: the era, and the world's own document by address.
    places: Option<(PlacesFrom, Arc<Vec<Place>>)>,
    /// The kinds an administrator has turned off in the Places list.
    pub hidden: Vec<PlaceKind>,
    /// Whether the Places list is open.
    pub kinds_open: bool,
    /// The world the picture is drawn from.
    pub world: MapWorld,
}

impl Default for MapView {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            centre: (MAP_SIZE / 2.0, MAP_SIZE / 2.0),
            buttons: None,
            bar: Vec::new(),
            drag: None,
            press: None,
            pointed: None,
            pointed_name: None,
            go_waiting: None,
            waypoint_tip: None,
            waypoint_mark: None,
            corpse_mark: None,
            pin: None,
            pin_mark: None,
            offer: None,
            offer_button: None,
            tiles: HashMap::new(),
            places: None,
            hidden: Vec::new(),
            kinds_open: false,
            world: MapWorld::default(),
        }
    }
}

impl MapView {
    /// The part of the picture shown, in its pixels.
    #[must_use]
    pub fn shown(&self) -> Rect {
        let span = MAP_SIZE / self.zoom;
        Rect::new(
            self.centre.0 - span / 2.0,
            self.centre.1 - span / 2.0,
            span,
            span,
        )
    }

    /// The screen point of picture pixel `at`, the view drawn over `area`.
    #[must_use]
    pub fn on_screen(&self, area: Rect, at: (f32, f32)) -> (f32, f32) {
        let shown = self.shown();
        let k = area.w / shown.w;
        (area.x + (at.0 - shown.x) * k, area.y + (at.1 - shown.y) * k)
    }

    /// The picture pixel under screen point `at`, the view drawn over `area`.
    #[must_use]
    pub fn on_picture(&self, area: Rect, at: (f32, f32)) -> (f32, f32) {
        let shown = self.shown();
        let k = shown.w / area.w;
        (shown.x + (at.0 - area.x) * k, shown.y + (at.1 - area.y) * k)
    }

    /// The view held on the picture, at a zoom `area` can show.
    fn settle(&mut self, area: Rect) {
        self.zoom = self.zoom.clamp(1.0, most_zoom(area));
        let half = MAP_SIZE / self.zoom / 2.0;
        self.centre.0 = self.centre.0.clamp(half, MAP_SIZE - half);
        self.centre.1 = self.centre.1.clamp(half, MAP_SIZE - half);
    }

    /// Zoom to `zoom`, picture pixel `anchor` staying where it is on the screen.
    fn zoom_about(&mut self, area: Rect, zoom: f32, anchor: (f32, f32)) {
        let was = self.on_screen(area, anchor);
        self.zoom = zoom;
        self.settle(area);
        let now = self.on_screen(area, anchor);
        let k = self.shown().w / area.w;
        self.centre.0 += (now.0 - was.0) * k;
        self.centre.1 += (now.1 - was.1) * k;
        self.settle(area);
    }

    /// The tiles under the view, each with the part of the picture it draws.
    fn tiles_shown(&self) -> Vec<((u32, u32), Rect)> {
        let shown = self.shown();
        let mut tiles = Vec::new();
        for row in 0..TILES {
            for col in 0..TILES {
                #[allow(clippy::cast_precision_loss)]
                let (x, y) = (col as f32 * TILE, row as f32 * TILE);
                let tile = Rect::new(x, y, TILE.min(MAP_SIZE - x), TILE.min(MAP_SIZE - y));
                if let Some(part) = tile.intersect(&shown) {
                    tiles.push(((col, row), part));
                }
            }
        }
        tiles
    }

    /// Let go of the tiles no view has needed since `before`.
    fn let_tiles_go(&mut self, art: &Art, before: f64) {
        self.tiles.retain(|&(col, row), used| {
            let keep = *used >= before;
            if !keep {
                art.let_go(&tile_key(col, row));
            }
            keep
        });
    }

    /// Draw from `built` rather than the data the art holds: a world drawn elsewhere.
    pub fn show(&mut self, built: Built) {
        self.world = MapWorld {
            picture: Picture::Ready(built.map),
            data: None,
            overview: built.overview,
        };
    }

    /// Whether the world the tiles are drawn from is here.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        matches!(self.world.picture, Picture::Ready(_))
    }

    /// Whether the world is being built.
    #[must_use]
    pub fn is_building(&self) -> bool {
        matches!(self.world.picture, Picture::Building(_))
    }

    /// The world kept up with the data `art` holds, a frame at a time and off the frame: built in
    /// the background once there is data, with the picture of the whole unless the art has it,
    /// taken once built, and built again for other data, the old pictures let go.
    pub fn preload(&mut self, art: &Art) {
        let store = art.ac_data();
        let given = self.world.data.is_none() && self.is_ready();
        let same = match (&self.world.data, &store) {
            (Some(was), Some(now)) => Weak::ptr_eq(was, &Arc::downgrade(now)),
            (was, now) => was.is_none() && now.is_none(),
        };
        if !given && !same {
            if self.world.data.is_some() {
                art.let_go(OVERVIEW_KEY);
                self.let_tiles_go(art, f64::INFINITY);
            }
            self.world = MapWorld {
                data: store.as_ref().map(Arc::downgrade),
                ..MapWorld::default()
            };
        }
        if let Picture::Building(build) = &mut self.world.picture {
            match build.poll() {
                Some(Ok(built)) => {
                    self.world.picture = Picture::Ready(built.map);
                    if built.overview.is_some() {
                        self.world.overview = built.overview;
                    }
                }
                Some(Err(e)) => {
                    tracing::warn!("the world map was not drawn: {e}");
                    self.world.picture = Picture::Failed;
                }
                None => {}
            }
        }
        let whole = self.world.overview.is_some() || art.lookup(OVERVIEW_KEY).is_some();
        let wanted = match self.world.picture {
            Picture::None => true,
            // Art drawn afresh over the same data has no picture of the whole.
            Picture::Ready(_) => !given && !whole,
            Picture::Building(_) | Picture::Failed => false,
        };
        if let Some(store) = store.filter(|_| wanted && !given) {
            self.world.picture = Picture::Building(MapBuild::in_background(store, !whole));
        }
    }

    /// The picture kept up with the data `art` holds ([`MapView::preload`]), and the picture of
    /// the whole given to the art once it is drawn.
    fn keep_picture(&mut self, art: &Art) {
        self.preload(art);
        if let Some(overview) = self.world.overview.take() {
            art.let_go(OVERVIEW_KEY);
            art.cached(OVERVIEW_KEY, || Some(image(overview)));
        }
    }
}

/// `pixels` as the art keeps a picture.
fn image(pixels: Pixels) -> Image {
    Image {
        width: pixels.width,
        height: pixels.height,
        bgra: pixels.bgra,
    }
}

/// The arrow's picture: red, in a white edge that shows on every ground the map has.
fn arrow_image(fill: [f32; 3]) -> Image {
    let side = ARROW_SIDE as usize;
    let mut bgra = vec![0; side * side * 4];
    // Distance from `(x, y)` to the arrow's edge, negative inside.
    let distance = |x: f32, y: f32| {
        let mut nearest = f32::MAX;
        let mut inside = false;
        for (i, &(ax, ay)) in ARROW.iter().enumerate() {
            let (bx, by) = ARROW[(i + 1) % ARROW.len()];
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
    };
    for (i, px) in bgra.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        #[allow(clippy::cast_precision_loss)] // LINT-OK: within the picture's few pixels.
        let d = distance((i % side) as f32 + 0.5, (i / side) as f32 + 0.5);
        let red = ((-d - 4.0) + 0.5).clamp(0.0, 1.0);
        let cover = (0.5 - d).clamp(0.0, 1.0);
        let blend = |white: f32, fill: f32| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // LINT-OK: a colour channel, clamped to its range above.
            {
                (white * (1.0 - red) + fill * red) as u8
            }
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: a coverage in 0..1 scaled to a byte.
        px.copy_from_slice(&[
            blend(255.0, fill[0]),
            blend(255.0, fill[1]),
            blend(255.0, fill[2]),
            (cover * 255.0) as u8,
        ]);
    }
    Image {
        width: ARROW_SIDE,
        height: ARROW_SIDE,
        bgra,
    }
}

/// A town's icon: a castle on a gold disc in a dark rim, drawn at [`TOWN_SIDE`] a side.
#[must_use]
pub fn town_icon() -> Image {
    // Signed distance from `(x, y)` to a box, negative inside.
    let boxed = |x: f32, y: f32, (x0, y0, x1, y1): (f32, f32, f32, f32)| {
        let dx = (x0 - x).max(x - x1);
        let dy = (y0 - y).max(y - y1);
        hypotf(dx.max(0.0), dy.max(0.0)) + dx.max(dy).min(0.0)
    };
    let cover = |d: f32| (0.5 - d).clamp(0.0, 1.0);
    let mix = |a: [f32; 3], b: [f32; 3], t: f32| {
        [
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
        ]
    };
    let (rim, gold, stone) = ([40.0, 28.0, 16.0], [236.0, 196.0, 96.0], [58.0, 40.0, 22.0]);
    // Two crenellated towers either side of a lower wall.
    let castle = [
        (16.0, 22.0, 26.0, 47.0),
        (38.0, 22.0, 48.0, 47.0),
        (26.0, 30.0, 38.0, 47.0),
        (16.0, 17.0, 19.5, 22.0),
        (22.5, 17.0, 26.0, 22.0),
        (38.0, 17.0, 41.5, 22.0),
        (44.5, 17.0, 48.0, 22.0),
        (29.0, 26.0, 35.0, 30.0),
    ];
    // The gate: an arch over a doorway.
    let gate = (29.0, 39.0, 35.0, 47.0);
    let arch = ((32.0, 39.0), 3.0);
    let side = TOWN_SIDE as usize;
    let mut bgra = vec![0; side * side * 4];
    for (i, px) in bgra.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        #[allow(clippy::cast_precision_loss)] // LINT-OK: within the picture's few pixels.
        let (x, y) = ((i % side) as f32 + 0.5, (i / side) as f32 + 0.5);
        let centre = hypotf(x - 32.0, y - 32.0);
        let mut colour = mix(rim, gold, cover(centre - 26.5));
        let walls = castle
            .iter()
            .map(|&b| boxed(x, y, b))
            .fold(f32::MAX, f32::min);
        colour = mix(colour, stone, cover(walls));
        let opening = boxed(x, y, gate).min(hypotf(x - arch.0 .0, y - arch.0 .1) - arch.1);
        colour = mix(colour, gold, cover(opening));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: colour channels and a coverage, each clamped to its range.
        px.copy_from_slice(&[
            colour[2] as u8,
            colour[1] as u8,
            colour[0] as u8,
            (cover(centre - 30.0) * 255.0) as u8,
        ]);
    }
    Image {
        width: TOWN_SIDE,
        height: TOWN_SIDE,
        bgra,
    }
}

/// Where the Places list is drawn over the picture `area`, at scale `k`.
fn kinds_rect(area: Rect, k: f32) -> Rect {
    #[allow(clippy::cast_precision_loss)] // LINT-OK: a handful of rows.
    let rows = PlaceKind::ALL.len() as f32;
    let w = KIND_LIST_W * k;
    Rect::new(
        area.right() - w - 8.0 * k,
        area.y + 8.0 * k,
        w,
        (rows * KIND_ROW + 8.0) * k,
    )
}

/// The most `area` zooms the picture.
fn most_zoom(area: Rect) -> f32 {
    (MOST_MAGNIFIED * MAP_SIZE / area.w.max(1.0)).max(1.0)
}

/// `text` written as a link from `(x, y)`: underlined in its own colour. Where it was drawn, and
/// whether it was clicked.
pub(super) fn link_text(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    style: &TextStyle,
    (x, y): (f32, f32),
    text: &str,
) -> (Rect, bool) {
    let k = p.scale;
    let r = Rect::new(x, y, p.measure(style, text), p.line_height(style));
    let over = ctx.over(&r);
    p.text(style, x, y, text);
    p.fill(
        Rect::new(x, r.bottom() - 2.0 * k, r.w, k.max(1.0)),
        style.colour,
    );
    (r, over && ctx.input.clicked(&r))
}

/// What tile `(col, row)` is kept under.
#[must_use]
pub fn tile_key(col: u32, row: u32) -> String {
    format!("map.tile-{col}-{row}")
}

impl Windows {
    /// Let the map's tiles go while its window is closed; the world is kept.
    pub(super) fn map_closed(&mut self, p: &Painter<'_>) {
        if !self.map_view.tiles.is_empty() {
            self.map_view.let_tiles_go(p.art, f64::INFINITY);
        }
        self.map_view.pin = None;
        self.map_view.offer = None;
    }

    pub(super) fn map(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        p.fill(body, 0xFF2A_2418);
        // The picture, as large as the window holds it over two rows of controls.
        let top = 8.0 * k;
        let side = (body.w - 24.0 * k).min(body.h - 78.0 * k - top + 8.0 * k);
        self.map_view.bar.clear();
        let area = Rect::new(body.x + (body.w - side) / 2.0, body.y + top, side, side);
        self.map_view.settle(area);
        self.map_zoom_buttons(p, ctx, state, body, area);
        // A press anywhere else in the window lets Create Waypoint go, back to a link's arrow.
        self.map_offer_pin(state);
        let offered = self.map_offer_rect(p, area);
        let mouse = ctx.input.mouse;
        if ctx.input.pressed[0]
            && ctx.input.hover(&body)
            && !offered.is_some_and(|r| r.contains(mouse.0, mouse.1))
        {
            self.map_view.offer = None;
            self.map_view.offer_button = None;
            self.map_offer_pin(state);
        }
        self.map_pointer(ctx, area, p.scale);
        self.draw_map(p, ctx, area);
        // The pad's focus starts on the map itself, none of its places chosen.
        {
            let input = &mut *ctx.input;
            input
                .nav
                .note(area, crate::ui::nav::Kind::Plain, &input.occluders);
            input.nav.home(area);
        }
        self.map_picture = Some(area);
        self.tell_map_presses(ctx, area, p.scale);
        self.map_places(p, ctx, state, area);
        self.map_journal(p, ctx, state, area, out);
        self.map_corpse(p, ctx, state, area);
        self.map_waypoint(p, ctx, state, area);
        self.map_kinds(p, ctx, state, area);
        let clicked = self.map_click(ctx, area, k);
        self.map_offer(p, ctx, state, area, clicked);
        // The plugins' points and lines on the map.
        if !self.plugins.frame.marks.map.is_empty() {
            let view = &self.map_view;
            let place = |ns: f64, ew: f64| {
                let (x, y) = view.on_screen(area, map_pixel(ns, ew));
                Some((
                    pyreal_primitives::num::to_i32(x),
                    pyreal_primitives::num::to_i32(y),
                ))
            };
            let glyphs = pyreal_plugin_ui::marks::map_glyphs(&self.plugins.frame.marks.map, &place);
            p.list.push_clip(area);
            pyreal_plugin_ui::layer::glyphs(p, &glyphs);
            p.list.pop_clip();
        }
        if let Some((n, e)) = state.coords {
            self.map_player(p, state, area, (n, e));
            let style =
                TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
            let label = coords_text(n, e);
            let w = p.measure(&style, &label);
            let at = (body.x + (body.w - w) / 2.0, area.bottom() + 8.0 * k);
            let (r, hit) = link_text(p, ctx, &style, at, &label);
            self.map_view.bar.push(("Coordinates", r));
            // The player's own place, a link like any other.
            if hit {
                let here = (f64::from(n), f64::from(e));
                self.show_place_on_map(here, None, ctx.time);
            }
        }
        // The second row: the journal's pages on the map or not, and the waypoint made a page.
        let second = area.bottom() + 34.0 * k;
        let style = TextStyle::new(Family::Body, 12.0, ctx.colours.text()).edge(ctx.colours.edge());
        let caption = "Journal pages";
        let toggle = Rect::new(
            body.x + 8.0 * k,
            second + 2.0 * k,
            p.measure(&style, caption) + 28.0 * k,
            22.0 * k,
        );
        self.map_view.bar.push(("Journal pages", toggle));
        if let Some(on) = kit::check_row(p, ctx, toggle, !self.map_journal_hidden, &style, caption)
        {
            self.map_journal_hidden = !on;
        }
        // From the right: the waypoint made a page, and shared in chat.
        let mut right = body.right() - 8.0 * k;
        let mut next = |p: &Painter<'_>, label: &str| {
            let w = kit::button_label_width(p, label) + 24.0 * k;
            right -= w;
            let r = Rect::new(right, second, w, 26.0 * k);
            right -= 8.0 * k;
            r
        };
        if let Some(w) = self.waypoints.active.clone() {
            let r = next(p, "Add Waypoint to Journal");
            self.map_view.bar.push(("Add Waypoint to Journal", r));
            if kit::button(p, ctx, r, "Add Waypoint to Journal", true) {
                out.requests
                    .push(pyreal_client_contract::UiRequest::Journal(
                        pyreal_client_contract::journal::JournalAction::NewPageAt {
                            title: w.label(),
                            ns: w.ns,
                            ew: w.ew,
                        },
                    ));
                self.open(super::WindowId::Journal, ctx.time);
                self.show_journal_tab(super::JournalTab::Notebook);
            }
            // Explore mode has no chat to share it in.
            if state.explore.is_none() {
                let r = next(p, SHARE_WAYPOINT);
                self.map_view.bar.push((SHARE_WAYPOINT, r));
                if ctx.over(&r) {
                    self.map_view.waypoint_tip = Some((
                        SHARE_WAYPOINT.to_owned(),
                        vec!["Puts the waypoint's place on the chat line.".to_owned()],
                    ));
                }
                if kit::button(p, ctx, r, SHARE_WAYPOINT, true) {
                    out.chat_paste = Some(crate::ui::waypoint::share_text(&w));
                }
            }
        }
        self.map_teleport(p, ctx, state, (body, area), out);
        self.map_explore_go(ctx, state, (area, k), clicked.is_some());
        self.map_explore_went(ctx, state, out);
        // The marker's own tooltip over explore mode's.
        if let Some(tip) = self.map_view.waypoint_tip.take() {
            self.tip = Some(tip);
        }
    }

    /// In explore mode a click on the picture that does not pan moves the camera there, onto the
    /// place under the pointer if there is one, once no second click follows it.
    fn map_explore_go(
        &mut self,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        (area, k): (Rect, f32),
        still: bool,
    ) {
        const GO: &str = "Click to go there.";
        let view = &mut self.map_view;
        if state.explore.is_none() || !ctx.input.hover(&area) {
            return;
        }
        let mouse = ctx.input.mouse;
        if view.kinds_open && kinds_rect(area, k).contains(mouse.0, mouse.1) {
            return;
        }
        let (x, y) = view.on_picture(area, mouse);
        let Some(at) = view.pointed.or_else(|| map_place(x, y)) else {
            return;
        };
        match &mut self.tip {
            Some((_, lines)) if view.pointed.is_some() => lines.push(GO.to_owned()),
            _ => {
                #[allow(clippy::cast_possible_truncation)]
                // LINT-OK: a coordinate within the world.
                let here = coords_text(at.0 as f32, at.1 as f32);
                self.tip = Some((here, vec![GO.to_owned()]));
            }
        }
        if still {
            view.go_waiting = Some((at, ctx.time));
        }
    }

    /// Explore mode's move to a place clicked, once no second click has made it a waypoint.
    fn map_explore_went(&mut self, ctx: &Ctx<'_>, state: &GameState, out: &mut Outcome) {
        let view = &mut self.map_view;
        if state.explore.is_none() {
            view.go_waiting = None;
            return;
        }
        if let Some((at, _)) = view.go_waiting.filter(|(_, t)| ctx.time - t >= GO_AFTER) {
            view.go_waiting = None;
            out.explore
                .push(pyreal_client_runtime::explore::Ask::Go(at.0, at.1));
        }
    }

    /// A press on the picture let go where it began: a click, where it was, and no pan.
    fn map_click(&mut self, ctx: &Ctx<'_>, area: Rect, k: f32) -> Option<(f32, f32)> {
        let press = if ctx.input.released[0] {
            self.map_view.press.take()
        } else {
            None
        }?;
        let mouse = ctx.input.mouse;
        let still = hypotf(press.0 - mouse.0, press.1 - mouse.1) <= CLICK_SLOP * k;
        (still && ctx.input.hover(&area)).then_some(mouse)
    }

    /// Open the map on a place written as a link, an arrow standing on it with Create Waypoint
    /// beside it, named `name`.
    pub(super) fn show_place_on_map(&mut self, at: (f64, f64), name: Option<String>, now: f64) {
        self.show_on_map(at.0, at.1, now);
        self.map_view.pin = Some((at, name));
        self.map_view.offer = None;
    }

    /// Offer Create Waypoint at a link's arrow while nothing else is offered, unless the player
    /// stands there, the waypoint is there already or a teleport is armed.
    fn map_offer_pin(&mut self, state: &GameState) {
        let view = &mut self.map_view;
        let Some((at, _)) = view.pin.as_ref().filter(|_| view.offer.is_none()) else {
            return;
        };
        let here = crate::ui::waypoint::Waypoint {
            ns: at.0,
            ew: at.1,
            name: None,
        };
        let near = |from: (f64, f64)| here.way_from(from).1 <= crate::ui::waypoint::ARRIVED_METRES;
        let followed = self
            .waypoints
            .active
            .as_ref()
            .is_some_and(|w| near((w.ns, w.ew)));
        if !self.admin.map_armed && !state.here.is_some_and(near) && !followed {
            view.offer.clone_from(&view.pin);
        }
    }

    /// Where Create Waypoint stands beside the place it is offered at, inside the picture; `None`
    /// with nothing offered or the place out of view.
    fn map_offer_rect(&mut self, p: &Painter<'_>, area: Rect) -> Option<Rect> {
        let k = p.scale;
        let view = &mut self.map_view;
        let r = view.offer.as_ref().and_then(|((ns, ew), _)| {
            let (x, y) = view.on_screen(area, map_pixel(*ns, *ew));
            if !area.contains(x, y) {
                return None;
            }
            let (w, h, gap) = (
                kit::button_label_width(p, CREATE_WAYPOINT) + 18.0 * k,
                26.0 * k,
                12.0 * k,
            );
            let bx = if x + gap + w <= area.right() {
                x + gap
            } else {
                x - gap - w
            };
            let by = (y - h / 2.0).clamp(area.y, area.bottom() - h);
            Some(Rect::new(bx, by, w, h))
        });
        view.offer_button = r;
        r
    }

    /// A link's arrow, and Create Waypoint: offered at a place clicked, unless explore mode goes
    /// there, and at the arrow when it is clicked. Choosing it sets the waypoint there.
    fn map_offer(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        area: Rect,
        clicked: Option<(f32, f32)>,
    ) {
        use crate::ui::waypoint as wp;
        let k = p.scale;
        let view = &mut self.map_view;
        view.pin_mark = None;
        if let Some(((ns, ew), name)) = view.pin.clone() {
            let (px, py) = view.on_screen(area, map_pixel(ns, ew));
            if area.contains(px, py) {
                let side = PIN_UNITS * k;
                #[allow(clippy::cast_precision_loss)]
                let tip = ARROW[0].1 / ARROW_SIDE as f32 * side;
                let dst = Rect::new(px - side / 2.0, py - tip, side, side);
                if let Some(arrow) = p.art.cached(PIN_KEY, || Some(arrow_image(PIN_FILL))) {
                    p.list.push_clip(area);
                    p.sprite(&arrow, dst, WHITE);
                    p.list.pop_clip();
                }
                view.pin_mark = Some(dst);
                if ctx.over(&dst) {
                    let here = coords_text_f64((ns, ew));
                    view.waypoint_tip = Some(match name.clone() {
                        Some(name) => (name, vec![here]),
                        None => (here, Vec::new()),
                    });
                }
            }
        }
        if self.admin.map_armed {
            view.offer = None;
        } else if let Some(at) = clicked {
            let on = |r: Option<Rect>| r.is_some_and(|r| r.contains(at.0, at.1));
            let on_list = view.kinds_open && kinds_rect(area, k).contains(at.0, at.1);
            if on(view.pin_mark) {
                view.offer.clone_from(&view.pin);
            } else if state.explore.is_none()
                && !on(view.waypoint_mark)
                && !on(view.corpse_mark)
                && !on_list
            {
                let (x, y) = view.on_picture(area, at);
                view.offer = match view.pointed {
                    Some(at) => Some((at, view.pointed_name.clone())),
                    None => map_place(x, y).map(|at| (at, None)),
                };
            }
        }
        let Some((at, name)) = self.map_view.offer.clone() else {
            self.map_view.offer_button = None;
            return;
        };
        let Some(r) = self.map_offer_rect(p, area) else {
            return;
        };
        if kit::button(p, ctx, r, CREATE_WAYPOINT, true) {
            self.waypoints.set(wp::Waypoint {
                ns: at.0,
                ew: at.1,
                name,
            });
            let view = &mut self.map_view;
            if view.pin.as_ref().is_some_and(|(pin, _)| *pin == at) {
                view.pin = None;
            }
            view.offer = None;
            view.offer_button = None;
        }
    }

    /// Open the map on a place, close enough to see what is round it.
    pub(super) fn show_on_map(&mut self, ns: f64, ew: f64, now: f64) {
        self.open(super::WindowId::Map, now);
        self.map_view.centre = map_pixel(ns, ew);
        self.map_view.zoom = self.map_view.zoom.max(JOURNAL_ZOOM);
    }

    /// The journal's pages with a place, unless turned off: a mark and the page's name each, its
    /// notes' first line under the pointer, and a click opens the page.
    fn map_journal(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        area: Rect,
        out: &mut Outcome,
    ) {
        if self.map_journal_hidden {
            return;
        }
        let k = p.scale;
        let small = TextStyle::new(Family::Body, 11.0, ctx.colours.text()).edge(ctx.colours.edge());
        let side = JOURNAL_MARK * k;
        let mut open = None;
        p.list.push_clip(area);
        for page in state.journal.pages.iter().filter(|pg| pg.location_set) {
            let at = self.map_view.on_screen(area, map_pixel(page.ns, page.ew));
            if !area.contains(at.0, at.1) {
                continue;
            }
            let mark = Rect::new(at.0 - side / 2.0, at.1 - side / 2.0, side, side);
            p.fill(mark, 0xFF10_1010);
            p.fill(
                Rect::new(mark.x + k, mark.y + k, side - 2.0 * k, side - 2.0 * k),
                JOURNAL_COLOUR,
            );
            let name = if page.name().is_empty() {
                format!("Page {}", page.page_number)
            } else {
                page.name().to_owned()
            };
            p.text(&small, mark.right() + 3.0 * k, at.1 - 7.0 * k, &name);
            let reach = mark.inset(-3.0 * k);
            if ctx.over(&reach) {
                #[allow(clippy::cast_possible_truncation)]
                // LINT-OK: a coordinate within the world.
                let mut lines = vec![coords_text(page.ns as f32, page.ew as f32)];
                if let Some(first) = page.notes.lines().find(|l| !l.trim().is_empty()) {
                    lines.insert(0, first.to_owned());
                }
                lines.push("Click to open the page.".to_owned());
                self.map_view.waypoint_tip = Some((name, lines));
                if ctx.input.clicked(&reach) {
                    open = Some(page.page_number);
                }
            }
        }
        p.list.pop_clip();
        if let Some(page) = open {
            self.map_view.press = None;
            self.map_view.drag = None;
            self.open_notebook_page(page, ctx.time, out);
        }
    }

    /// The corpse on the map, its stone's foot on the place the player died: a click on it sets
    /// the waypoint there, and a right click forgets it.
    fn map_corpse(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        area: Rect,
    ) {
        use crate::ui::corpse as cp;
        let k = p.scale;
        self.map_view.corpse_mark = None;
        let Some(c) = self.waypoints.corpse.active.filter(|c| !c.in_dungeon) else {
            return;
        };
        let (px, py) = self.map_view.on_screen(area, map_pixel(c.ns, c.ew));
        if !area.contains(px, py) {
            return;
        }
        let Some(mark) = p.art.cached(cp::MARK_KEY, || Some(cp::mark_image())) else {
            return;
        };
        let side = CORPSE_UNITS * k;
        let dst = Rect::new(px - side / 2.0, py - side * cp::MARK_FOOT, side, side);
        p.list.push_clip(area);
        p.sprite(&mark, dst, WHITE);
        p.list.pop_clip();
        self.map_view.corpse_mark = Some(dst);
        if !ctx.over(&dst) {
            return;
        }
        let mut lines = vec![coords_text_f64((c.ns, c.ew))];
        if let Some(here) = state.here.filter(|_| !state.in_dungeon) {
            if let Some((_, metres)) = self.waypoints.corpse.way(here, false, None) {
                lines.push(format!(
                    "{} away.",
                    crate::ui::waypoint::distance_text(metres)
                ));
            }
        }
        let followed = self
            .waypoints
            .corpse
            .is_followed(self.waypoints.active.as_ref());
        if !followed {
            lines.push("Click to set a waypoint to it.".to_owned());
        }
        lines.push(crate::ui::corpse::FORGET_TIP.to_owned());
        self.map_view.waypoint_tip = Some(("Your corpse".to_owned(), lines));
        if ctx.input.right_clicked(&dst) {
            self.waypoints.corpse.forget();
            self.map_view.corpse_mark = None;
        } else if !followed && ctx.input.clicked(&dst) {
            if let Some(w) = self.waypoints.corpse.waypoint(&state.name) {
                self.waypoints.set(w);
            }
            self.map_view.press = None;
            self.map_view.drag = None;
        }
    }

    /// The waypoint on the map: a double click on the picture sets it there (on the place under
    /// the pointer, named for it, when there is one), and its marker stands on it, its tip on the
    /// very place.
    fn map_waypoint(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        area: Rect,
    ) {
        use crate::ui::waypoint as wp;
        let k = p.scale;
        let mouse = ctx.input.mouse;
        let on_list = self.map_view.kinds_open && kinds_rect(area, k).contains(mouse.0, mouse.1);
        let on_offer = self
            .map_view
            .offer_button
            .is_some_and(|r| r.contains(mouse.0, mouse.1));
        self.map_view.waypoint_mark = None;
        if !self.admin.map_armed && !on_list && !on_offer && ctx.input.double_clicked(&area) {
            let (x, y) = self.map_view.on_picture(area, mouse);
            let picked = match self.map_view.pointed {
                Some(at) => Some((at, self.map_view.pointed_name.clone())),
                None => map_place(x, y).map(|at| (at, None)),
            };
            if let Some(((ns, ew), name)) = picked {
                self.waypoints.set(wp::Waypoint { ns, ew, name });
            }
            // The double click neither pans, offers Create Waypoint nor moves explore mode's camera.
            self.map_view.offer = None;
            self.map_view.offer_button = None;
            self.map_view.press = None;
            self.map_view.drag = None;
            self.map_view.go_waiting = None;
        }
        let Some(w) = self.waypoints.active.clone() else {
            return;
        };
        let view = &self.map_view;
        let (px, py) = view.on_screen(area, map_pixel(w.ns, w.ew));
        if !area.contains(px, py) {
            return;
        }
        let Some(arrow) = p.art.cached(wp::ARROW_KEY, || Some(wp::arrow_image())) else {
            return;
        };
        let side = WAYPOINT_UNITS * k;
        let dst = Rect::new(
            px - side / 2.0,
            py - side * (1.0 - wp::ARROW_TIP),
            side,
            side,
        );
        p.list.push_clip(area);
        p.sprite_turned(&arrow, dst, WHITE, std::f32::consts::PI);
        p.list.pop_clip();
        self.map_view.waypoint_mark = Some(dst);
        if ctx.over(&dst) {
            let mut lines = vec![wp::set_text(&w)];
            if let Some(here) = state.here {
                lines.push(format!("{} away.", wp::distance_text(w.way_from(here).1)));
            }
            self.map_view.waypoint_tip = Some(("Waypoint".to_owned(), lines));
        }
    }

    /// The buttons under the picture: closer about the player while they are in view and otherwise
    /// about the view's middle, further out the same way, and back to the fit.
    fn map_zoom_buttons(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        area: Rect,
    ) {
        let k = p.scale;
        let view = &mut self.map_view;
        let shown = view.shown();
        let anchor = state
            .coords
            .map(|(n, e)| map_pixel(f64::from(n), f64::from(e)))
            .filter(|(x, y)| shown.contains(*x, *y))
            .unwrap_or(view.centre);
        let most = most_zoom(area);
        let y = area.bottom() + 4.0 * k;
        let mut x = body.x + 8.0 * k;
        let mut next = |w: f32| {
            let r = Rect::new(x, y, w * k, 26.0 * k);
            x += (w + 4.0) * k;
            r
        };
        let (out_at, in_at, fit_at) = (next(30.0), next(30.0), next(44.0));
        view.buttons = Some([out_at, in_at, fit_at]);
        view.bar
            .extend([("Zoom Out", out_at), ("Zoom In", in_at), ("Fit", fit_at)]);
        if state.is_admin {
            let places = next(64.0);
            view.bar.push(("Places", places));
            if kit::button(p, ctx, places, "Places", true) {
                view.kinds_open = !view.kinds_open;
            }
        }
        if kit::button(p, ctx, out_at, "-", view.zoom > 1.0) {
            view.zoom_about(area, view.zoom / BUTTON_STEP, anchor);
        }
        if kit::button(p, ctx, in_at, "+", view.zoom < most) {
            view.zoom_about(area, view.zoom * BUTTON_STEP, anchor);
        }
        if kit::button(p, ctx, fit_at, "Fit", view.zoom > 1.0) {
            view.zoom_about(area, 1.0, anchor);
        }
    }

    /// The wheel over the picture zooms about the pointer, and a drag begun on it pans, unless a
    /// teleport is waiting for that press.
    fn map_pointer(&mut self, ctx: &mut Ctx<'_>, area: Rect, k: f32) {
        let armed = self.admin.map_armed;
        let view = &mut self.map_view;
        let mouse = ctx.input.mouse;
        let on_list = view.kinds_open && kinds_rect(area, k).contains(mouse.0, mouse.1);
        let over = ctx.input.hover(&area) && !on_list;
        if over && ctx.input.wheel != 0.0 {
            let zoom = view.zoom * pyreal_primitives::num::math::powf(WHEEL_STEP, ctx.input.wheel);
            let anchor = view.on_picture(area, mouse);
            view.zoom_about(area, zoom, anchor);
            ctx.input.wheel = 0.0;
        }
        if let Some(last) = view.drag {
            if ctx.input.down[0] {
                let k = view.shown().w / area.w;
                view.centre.0 += (last.0 - mouse.0) * k;
                view.centre.1 += (last.1 - mouse.1) * k;
                view.settle(area);
                view.drag = Some(mouse);
            } else {
                view.drag = None;
            }
        } else if over
            && !armed
            && ctx.input.pressed[0]
            && !view
                .offer_button
                .is_some_and(|r| r.contains(mouse.0, mouse.1))
        {
            view.drag = Some(mouse);
            view.press = Some(mouse);
        }
    }

    /// The part of the picture the view shows, the sea's colour until the picture of the whole is
    /// drawn. Zoomed in past what the picture of the whole holds it is drawn from the tiles, one
    /// newly drawn a frame, the picture of the whole under any not drawn yet.
    fn draw_map(&mut self, p: &mut Painter<'_>, ctx: &Ctx<'_>, area: Rect) {
        let view = &mut self.map_view;
        let shown = view.shown();
        let close = area.w * view.zoom > OVERVIEW * OVERVIEW_STRETCH;
        view.keep_picture(p.art);
        let overview = p.art.lookup(OVERVIEW_KEY).flatten();
        let draw_overview = |p: &mut Painter<'_>| match &overview {
            Some(whole) => {
                let k = whole.src.w / MAP_SIZE;
                let src = Rect::new(shown.x * k, shown.y * k, shown.w * k, shown.h * k);
                p.sprite(&Sprite { src, ..*whole }, area, WHITE);
            }
            None => {
                let [r, g, b] = pyreal_scene::world_map::sea_colour();
                p.fill(area, u32::from_be_bytes([0xFF, r, g, b]));
            }
        };
        if !close {
            draw_overview(p);
            view.let_tiles_go(p.art, ctx.time - TILE_KEPT);
            return;
        }
        let map = match &view.world.picture {
            Picture::Ready(map) => Some(Arc::clone(map)),
            _ => None,
        };
        let mut drawn = false;
        let mut parts = Vec::new();
        let mut waiting = false;
        for ((col, row), part) in view.tiles_shown() {
            let key = tile_key(col, row);
            let tile = p.art.lookup(&key).unwrap_or_else(|| {
                let map = map.as_ref().filter(|_| !drawn)?;
                drawn = true;
                p.art
                    .cached(&key, || Some(image(map.tile((col as usize, row as usize)))))
            });
            match tile {
                Some(tile) => {
                    view.tiles.insert((col, row), ctx.time);
                    parts.push((tile, (col, row), part));
                }
                None => waiting = true,
            }
        }
        if waiting {
            draw_overview(p);
        }
        let k = area.w / shown.w;
        for (tile, (col, row), part) in parts {
            #[allow(clippy::cast_precision_loss)]
            let src = Rect::new(
                part.x - col as f32 * TILE,
                part.y - row as f32 * TILE,
                part.w,
                part.h,
            );
            let dst = Rect::new(
                area.x + (part.x - shown.x) * k,
                area.y + (part.y - shown.y) * k,
                part.w * k,
                part.h * k,
            );
            p.sprite(&Sprite { src, ..tile }, dst, WHITE);
        }
        view.let_tiles_go(p.art, ctx.time - TILE_KEPT);
    }

    /// The world's places ([`drawn_places`]): its own when its server published them, over those
    /// built in for its era; read once, and again when either changes.
    pub(super) fn world_places(&mut self, art: &Art, state: &GameState) -> Arc<Vec<Place>> {
        let from: PlacesFrom = (
            state.era_id,
            state
                .map_places
                .as_ref()
                .map(|t| Arc::as_ptr(t).cast::<u8>() as usize),
        );
        match &self.map_view.places {
            Some((was, places)) if *was == from => Arc::clone(places),
            _ => {
                let built_in = art
                    .file(places_file(state.era_id))
                    .map(read_places)
                    .unwrap_or_default();
                let read = drawn_places(state.map_places.as_deref(), built_in);
                let read = Arc::new(read);
                self.map_view.places = Some((from, Arc::clone(&read)));
                read
            }
        }
    }

    /// The places over the picture: the major towns' icons and names, then the houses' marks as
    /// the zoom shows them, and for an administrator the other places' too, with the landmarks'
    /// names and the portals' where they fit; and the name and place of the one under the pointer.
    fn map_places(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        area: Rect,
    ) {
        let places = self.world_places(p.art, state);
        let view = &self.map_view;
        let k = p.scale;
        let town =
            TextStyle::new(Family::Heading, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let small = TextStyle::new(Family::Body, 11.0, ctx.colours.text()).edge(ctx.colours.edge());
        let mut taken: Vec<Rect> = Vec::new();
        let mut pointed_at: Option<&Place> = None;
        p.list.push_clip(area);
        // The towns first, so their names have the room.
        let icon = p.art.cached("map.town", || Some(town_icon()));
        let side = TOWN_UNITS * k;
        let towns = !(state.is_admin && view.hidden.contains(&PlaceKind::Town));
        for place in places
            .iter()
            .filter(|pl| towns && pl.kind == PlaceKind::Town)
        {
            let at = view.on_screen(area, map_pixel(place.ns, place.ew));
            if !area.contains(at.0, at.1) {
                continue;
            }
            let r = Rect::new(at.0 - side / 2.0, at.1 - side / 2.0, side, side);
            match &icon {
                Some(icon) => p.sprite(icon, r, WHITE),
                None => p.fill(r, PlaceKind::Town.colour()),
            }
            if ctx.over(&r) {
                pointed_at = Some(place);
            }
            taken.push(r);
            let (w, h) = (p.measure(&town, &place.name), p.line_height(&town));
            let name = Rect::new(at.0 - w / 2.0, r.bottom(), w, h);
            if !taken.iter().any(|t| t.intersect(&name).is_some()) {
                p.text(&town, name.x, name.y, &place.name);
                taken.push(name);
            }
        }
        {
            let mouse = ctx.input.mouse;
            let pointed = pointed_at.is_none() && ctx.input.hover(&area);
            let mark_side = MARK_SIDE * k;
            let mut nearest: Option<(f32, &Place)> = None;
            let shown = |kind: PlaceKind| {
                kind != PlaceKind::Town
                    && (kind.players_see() || state.is_admin)
                    && !(state.is_admin && view.hidden.contains(&kind))
                    && view.zoom >= kind.marked_from()
            };
            for place in places.iter().filter(|pl| shown(pl.kind)) {
                let at = view.on_screen(area, map_pixel(place.ns, place.ew));
                if !area.contains(at.0, at.1) {
                    continue;
                }
                let mark = Rect::new(
                    at.0 - mark_side / 2.0,
                    at.1 - mark_side / 2.0,
                    mark_side,
                    mark_side,
                );
                p.fill(mark, 0xFF10_1010);
                p.fill(
                    Rect::new(
                        mark.x + k,
                        mark.y + k,
                        mark_side - 2.0 * k,
                        mark_side - 2.0 * k,
                    ),
                    place.kind.colour(),
                );
                if pointed {
                    let d = hypotf(mouse.0 - at.0, mouse.1 - at.1);
                    if d <= MARK_REACH * k && nearest.is_none_or(|(best, _)| d < best) {
                        nearest = Some((d, place));
                    }
                }
                if place.kind.named_at(view.zoom) {
                    let (w, h) = (p.measure(&small, &place.name), p.line_height(&small));
                    let r = Rect::new(at.0 + mark_side, at.1 - h / 2.0, w, h);
                    if !taken.iter().any(|t| t.intersect(&r).is_some()) {
                        p.text(&small, r.x, r.y, &place.name);
                        taken.push(r);
                    }
                }
            }
            pointed_at = pointed_at.or(nearest.map(|(_, place)| place));
        }
        p.list.pop_clip();
        self.map_view.pointed = pointed_at.map(|pl| (pl.ns, pl.ew));
        self.map_view.pointed_name = pointed_at.map(|pl| pl.name.clone());
        if let Some(place) = pointed_at {
            #[allow(clippy::cast_possible_truncation)] // LINT-OK: a coordinate within the world.
            let at = coords_text(place.ns as f32, place.ew as f32);
            let lines = place
                .detail
                .iter()
                .flat_map(|d| d.lines().map(str::to_owned))
                .chain([at])
                .collect();
            self.tip = Some((place.name.clone(), lines));
        }
    }

    /// An administrator's Places list, when open: a box for each kind, by its mark's colour.
    fn map_kinds(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, state: &GameState, area: Rect) {
        let view = &mut self.map_view;
        if !state.is_admin || !view.kinds_open {
            return;
        }
        let k = p.scale;
        let list = kinds_rect(area, k);
        p.fill(list, 0xE01A_1610);
        let style = TextStyle::new(Family::Body, 12.0, ctx.colours.text()).edge(ctx.colours.edge());
        let swatch = 8.0 * k;
        for (n, kind) in PlaceKind::ALL.into_iter().enumerate() {
            #[allow(clippy::cast_precision_loss)] // LINT-OK: a handful of rows.
            let y = list.y + 4.0 * k + n as f32 * KIND_ROW * k;
            let row = Rect::new(
                list.x + 10.0 * k + swatch,
                y,
                list.w - 14.0 * k - swatch,
                KIND_ROW * k,
            );
            p.fill(
                Rect::new(list.x + 6.0 * k, y + (row.h - swatch) / 2.0, swatch, swatch),
                kind.colour(),
            );
            let on = !view.hidden.contains(&kind);
            if let Some(on) = kit::check_row(p, ctx, row, on, &style, kind.label()) {
                if on {
                    view.hidden.retain(|&h| h != kind);
                } else {
                    view.hidden.push(kind);
                }
            }
        }
    }

    /// Where the player is, when that is in view: the game's mark, or for an administrator and in
    /// explore mode an arrow pointing straight down with its tip on the very place.
    fn map_player(&self, p: &mut Painter<'_>, state: &GameState, area: Rect, at: (f32, f32)) {
        let (x, y) = map_pixel(f64::from(at.0), f64::from(at.1));
        if !self.map_view.shown().contains(x, y) {
            return;
        }
        let (px, py) = self.map_view.on_screen(area, (x, y));
        if state.is_admin || state.explore.is_some() {
            if let Some(arrow) = p.art.cached("map.arrow", || Some(arrow_image(ADMIN_FILL))) {
                let side = ARROW_UNITS * p.scale;
                #[allow(clippy::cast_precision_loss)]
                let tip = ARROW[0].1 / ARROW_SIDE as f32 * side;
                let dst = Rect::new(px - side / 2.0, py - tip, side, side);
                p.sprite(&arrow, dst, WHITE);
            }
            return;
        }
        let s = 12.0 * p.scale;
        match p.art.ac_icon(graphic::PLAYER_ICON.0) {
            Some(icon) => p.sprite(&icon, Rect::new(px - s, py - s, 2.0 * s, 2.0 * s), WHITE),
            None => p.fill(Rect::new(px - s / 2.0, py - s / 2.0, s, s), 0xFF40_E040),
        }
    }

    /// A press on the map's picture `area` told to the plugins: the place under the pointer, the
    /// button and the modifiers held (the pad's A at the map's cursor is a left press there). The
    /// press is left to the map, which does nothing else with it but pan; one on the picture's
    /// margin, beyond the world, is told to no one.
    fn tell_map_presses(&mut self, ctx: &mut Ctx<'_>, area: Rect, k: f32) {
        let input = &*ctx.input;
        let mouse = input.mouse;
        let on_offer = self
            .map_view
            .offer_button
            .is_some_and(|r| r.contains(mouse.0, mouse.1));
        if !input.hover(&area)
            || on_offer
            || (self.map_view.kinds_open && kinds_rect(area, k).contains(mouse.0, mouse.1))
        {
            return;
        }
        let (x, y) = self.map_view.on_picture(area, input.mouse);
        let Some((ns, ew)) = map_place(x, y) else {
            return;
        };
        for (pressed, button) in
            input
                .pressed
                .iter()
                .zip([MapButton::Left, MapButton::Right, MapButton::Middle])
        {
            if *pressed {
                self.plugins
                    .push_input(pyreal_plugin_ui::Input::Map(MapClick {
                        ns,
                        ew,
                        button,
                        ctrl: input.ctrl,
                        shift: input.shift,
                        alt: input.alt,
                    }));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A place and its pixel are each other's, and Holtburg's is where the picture draws it.
    #[test]
    fn a_place_and_its_pixel_are_each_other_s_and_holtburg_is_where_the_picture_draws_it() {
        let (x, y) = map_pixel(42.2, 33.8);
        assert!((x - 4074.0).abs() < 0.01 && (y - 1797.0).abs() < 0.01);
        let (ns, ew) = map_place(x, y).expect("on the landscape");
        assert!((ns - 42.2).abs() < 1e-3 && (ew - 33.8).abs() < 1e-3);
        assert_eq!(map_place(5.0, 5.0).map(|_| ()), Some(()));
        assert_eq!(
            map_place(0.0, 3000.0),
            None,
            "the picture's edge, off the world"
        );
    }

    #[test]
    fn a_zoom_keeps_its_anchor_where_it_is_and_the_view_stays_on_the_picture() {
        let area = Rect::new(100.0, 50.0, 500.0, 500.0);
        let mut view = MapView::default();
        let anchor = (4074.0, 1797.0);
        let was = view.on_screen(area, anchor);
        view.zoom_about(area, 4.0, anchor);
        let now = view.on_screen(area, anchor);
        assert!(
            (was.0 - now.0).abs() < 0.1 && (was.1 - now.1).abs() < 0.1,
            "{was:?} {now:?}"
        );
        // At a corner the picture's edge holds the view rather than the anchor.
        view.zoom_about(area, 1.0, anchor);
        view.zoom_about(area, 4.0, (0.0, 0.0));
        let shown = view.shown();
        assert!(shown.x >= 0.0 && shown.y >= 0.0);
        assert!(shown.right() <= MAP_SIZE && shown.bottom() <= MAP_SIZE);
        view.zoom_about(area, 1e6, (0.0, 0.0));
        assert!(
            (view.zoom - most_zoom(area)).abs() < 1e-3,
            "held at the most"
        );
    }

    /// The tiles under the view cover it exactly, and a view of the whole is every tile.
    #[test]
    fn the_tiles_under_a_view_cover_it() {
        let mut view = MapView::default();
        assert_eq!(view.tiles_shown().len(), 36);
        view.zoom = 8.0;
        view.centre = (1024.0, 1024.0);
        let parts = view.tiles_shown();
        assert_eq!(parts.len(), 4, "the view stands on a corner of four tiles");
        let area: f32 = parts.iter().map(|(_, r)| r.w * r.h).sum();
        let shown = view.shown();
        assert!((area - shown.w * shown.h).abs() < 1.0);
    }

    /// The places built in for each era read and each stands on the landscape: the end of
    /// retail's have every kind, and Launch's no houses.
    #[test]
    fn the_built_in_places_read_and_each_stands_on_the_landscape() {
        use pyreal_primitives::EraId;
        let pieces = crate::pieces::Pieces::built_in();
        let read = |era: EraId| read_places(pieces.get(places_file(era)).expect("built in"));
        for era in EraId::ALL {
            let places = read(era);
            assert!(places.iter().any(|p| p.kind == PlaceKind::Town), "{era:?}");
            for place in &places {
                let (x, y) = map_pixel(place.ns, place.ew);
                assert!(map_place(x, y).is_some(), "{place:?}");
            }
        }
        let eor = read(EraId::Eor);
        for kind in PlaceKind::ALL {
            assert!(eor.iter().any(|p| p.kind == kind), "{kind:?}");
        }
        assert!(!read(EraId::Launch)
            .iter()
            .any(|p| p.kind == PlaceKind::Cottage));
        assert!(read_places(b"<html>").is_empty());
        // Players see the towns and houses; every other kind is an administrator's.
        let players: Vec<PlaceKind> = PlaceKind::ALL
            .into_iter()
            .filter(|k| k.players_see())
            .collect();
        assert_eq!(
            players,
            [
                PlaceKind::Town,
                PlaceKind::Cottage,
                PlaceKind::Villa,
                PlaceKind::Mansion
            ]
        );
    }

    fn spot_of(cell: u32, x: f32, y: f32) -> (f64, f64) {
        use pyreal_primitives::{CellId, Frame, Quat, Vec3};
        spot(&Position::new(
            CellId(cell),
            Frame::new(Vec3::new(x, y, 0.0), Quat::IDENTITY),
        ))
    }

    fn place(kind: PlaceKind, name: &str, (ns, ew): (f64, f64), detail: Option<&str>) -> Place {
        Place {
            kind,
            name: name.to_owned(),
            ns,
            ew,
            detail: detail.map(str::to_owned),
        }
    }

    /// A world that publishes its map marks (version 2 of its places) has them drawn alone, each
    /// with its detail, and a mark of a kind this client does not know left out.
    #[test]
    fn a_world_s_marks_are_drawn_alone_with_their_detail_and_unknown_kinds_left_out() {
        let served = "dereth-places-store\t2\nworld\tW\n\
            place\tvendor\tpublished\t1\tShopkeep\t0xA9B40021 82.000 6.000 94.000\t0\t0\t-\t-\n\
            mark\tdungeon\tDrudge Hideout\t0xA9B40021 84.000 7.500 0.000\tFor levels 5 and up.\\nHolds a vendor.\n\
            mark\tfountain\tWishing Well\t0xA9B40021 84.000 7.500 0.000\t-\n";
        let built_in = vec![place(PlaceKind::Cottage, "Cottage", (1.0, 1.0), None)];
        let places = drawn_places(Some(served), built_in.clone());
        assert_eq!(places.len(), 1, "{places:?}");
        assert_eq!(places[0].kind, PlaceKind::Dungeon);
        assert_eq!(
            places[0].detail.as_deref(),
            Some("For levels 5 and up.\nHolds a vendor.")
        );
        let (ns, ew) = spot_of(0xA9B4_0021, 84.0, 7.5);
        assert!((places[0].ns - ns).abs() < 1e-9 && (places[0].ew - ew).abs() < 1e-9);
        assert_eq!(drawn_places(None, built_in.clone()), built_in);
        assert_eq!(
            drawn_places(Some("{\"places\":[]}"), built_in.clone()),
            built_in
        );
        assert_eq!(
            drawn_places(Some("dereth-places-store\t1\nworld\tW\n"), built_in.clone()),
            built_in,
            "a world with nothing placed"
        );
    }

    /// A world that publishes only its places (version 1) has its lifestones, vendors, people and
    /// portals drawn in place of the built-in ones: what stands in a dungeon is left out, a portal
    /// into a dungeon is its entrance and one across the land marks where it leads, and a built-in
    /// place at the same spot gives its kind and its detail. The other kinds stay built in.
    #[test]
    fn a_world_s_own_places_stand_for_the_built_in_ones_of_their_kinds() {
        let served = "dereth-places-store\t1\nworld\tW\n\
            place\tlifestone\tpublished\t509\tLife Stone\t0x01AC0118 60.000 -50.000 0.000\t0\t0\t-\t-\n\
            place\tvendor\tpublished\t1\tJonathan the Grocer\t0xA9B40021 82.000 6.000 94.000\t0\t0\t-\t-\n\
            place\tvendor\tpublished\t2\tIn A Shop\t0xA9B40150 85.000 6.000 94.000\t0\t0\t-\t-\n\
            place\tnpc\tpublished\t3\tIlsin\t0xA9B40021 83.000 9.000 94.000\t0\t0\t-\t-\n\
            place\tportal\tpublished\t4\tPortal to Drudge Hideout\t0xA9B40021 84.000 7.500 94.000\t0\t0\t-\t0x01AC0118 60.000 -50.000 0.000\n\
            place\tportal\tpublished\t5\tNew Hole\t0xA9B40021 90.000 7.500 94.000\t0\t0\t-\t0x01AD0100 60.000 -50.000 0.000\n\
            place\tportal\tpublished\t6\tArwic Portal\t0xA9B40021 86.000 8.000 94.000\t0\t0\t-\t0xC6A9000D 30.000 100.000 12.500\n";
        let built_in = vec![
            place(PlaceKind::Town, "Holtburg", (42.28, 33.31), None),
            place(PlaceKind::Vendor, "Gone", (0.0, 0.0), None),
            place(
                PlaceKind::Collector,
                "Ilsin the Collector",
                spot_of(0xA9B4_0021, 83.0, 9.0),
                None,
            ),
            place(
                PlaceKind::Dungeon,
                "Drudge Hideout",
                spot_of(0xA9B4_0021, 84.0, 7.5),
                Some("Holds a lifestone."),
            ),
        ];
        let places = drawn_places(Some(served), built_in);
        let named = |kind: PlaceKind| -> Vec<&str> {
            let mut v: Vec<&str> = places
                .iter()
                .filter(|p| p.kind == kind)
                .map(|p| p.name.as_str())
                .collect();
            v.sort_unstable();
            v
        };
        assert_eq!(named(PlaceKind::Town), ["Holtburg"], "built in");
        assert_eq!(
            named(PlaceKind::Vendor),
            ["In A Shop", "Jonathan the Grocer"],
            "the world's, a shop in a building among them"
        );
        assert!(named(PlaceKind::Lifestone).is_empty(), "the dungeon's");
        assert_eq!(named(PlaceKind::Collector), ["Ilsin"]);
        assert!(named(PlaceKind::Npc).is_empty());
        assert_eq!(named(PlaceKind::Dungeon), ["Drudge Hideout", "New Hole"]);
        let hideout = places.iter().find(|p| p.name == "Drudge Hideout").unwrap();
        assert_eq!(hideout.detail.as_deref(), Some("Holds a lifestone."));
        assert_eq!(named(PlaceKind::Portal), ["Arwic Portal"]);
        let drop = places.iter().find(|p| p.kind == PlaceKind::Drop).unwrap();
        assert_eq!(drop.name, "Arwic Portal");
        let (ns, ew) = spot_of(0xC6A9_000D, 30.0, 100.0);
        assert!((drop.ns - ns).abs() < 1e-9 && (drop.ew - ew).abs() < 1e-9);
    }

    /// Over the game's data the map's world and picture of the whole are built ahead, off the
    /// frame, so the first open draws them at once; a close keeps them, and only other data has
    /// them built again, the old picture let go.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn the_map_is_built_ahead_off_the_frame_and_opens_drawn() {
        use pyreal_dat::testing::{classic_shortfall, open_classic_store_or_fail, open_store};
        use std::time::Instant;
        let open = || {
            open_store()
                .or_else(|| {
                    classic_shortfall()
                        .is_none()
                        .then(open_classic_store_or_fail)
                })
                .expect("the retail dats, or the older ones")
        };
        let ms = |t: Instant| t.elapsed().as_secs_f64() * 1000.0;
        let store = Arc::new(open());
        // What the first open waited for before: the build begun as the map opened.
        let t = Instant::now();
        MapBuild::start(Arc::clone(&store), true)
            .wait()
            .expect("the map builds");
        let on_open = ms(t);

        let art = Art::empty();
        art.set_ac_store(Arc::clone(&store));
        let mut view = MapView::default();
        let t = Instant::now();
        view.preload(&art);
        assert!(view.is_building(), "begun on the first frame in the world");
        let mut frames = 1;
        while !view.is_ready() {
            view.preload(&art);
            frames += 1;
            assert!(t.elapsed().as_secs() < 60, "built in time");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let ahead = ms(t);
        assert!(frames > 1, "built off the frame");
        assert!(
            art.lookup(OVERVIEW_KEY).is_none(),
            "the picture of the whole waits for the map to open"
        );

        let t = Instant::now();
        view.keep_picture(&art);
        let first_open = ms(t);
        assert!(
            art.lookup(OVERVIEW_KEY).flatten().is_some(),
            "drawn at once"
        );
        let whole = art.lookup(OVERVIEW_KEY).flatten().map(|s| s.tex);
        for _ in 0..3 {
            view.preload(&art);
            view.keep_picture(&art);
        }
        assert!(
            view.is_ready() && !view.is_building(),
            "kept, not built again"
        );
        assert_eq!(art.lookup(OVERVIEW_KEY).flatten().map(|s| s.tex), whole);
        eprintln!(
            "built as the map opened: {on_open:.0} ms; built ahead: {ahead:.0} ms over {frames} \
             frames; the first open after: {first_open:.2} ms"
        );

        art.set_ac_store(Arc::new(open()));
        view.preload(&art);
        assert!(view.is_building(), "other data, built again");
        assert!(art.lookup(OVERVIEW_KEY).is_none(), "the old picture let go");
    }
}
