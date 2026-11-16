//! Explore mode: a world's data files with no server and no body, flown through with the free
//! camera at anything from a walk to Dereth's width in under a minute.
//!
//! [`read_place`](crate::explore::read_place) reads where to start from any place the client writes
//! (`@loc`'s text, a report line, or its `loc` or `camera` field).
//! [`Explore`](crate::explore::Explore) is the mode's state while it runs: how fast the camera
//! flies and the ground it keeps above. The keys are the front end's ([`Key`](crate::explore::Key))
//! and what the overlay asks is [`Ask`](crate::explore::Ask); the app carries both out.

use std::sync::Arc;

use pyreal_dat::RetailDatStore;
use pyreal_primitives::num::math;
use pyreal_primitives::{CellId, EraId, Frame, LandblockId, Position, Quat, Vec3};
use pyreal_world_data::land_source::DatLandSource;

use crate::report::{block_id, block_xy, outdoor_cell, ReportCamera, VisualReport};
use crate::world_state::WorldState;

/// The flying speeds, metres a second, slowest first.
pub const SPEEDS: [f32; 9] = [2.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0];

/// The fastest the camera flies, metres a second: the land is streamed in time up to it.
pub const MOST_SPEED: f32 = SPEEDS[SPEEDS.len() - 1];

/// The speed the camera starts at: 25 m/s.
pub const START_SPEED: usize = 3;

/// Metres the camera keeps above the ground.
pub const ROOM: f32 = 1.5;

/// Metres above the place a body stands that a camera started there stands: a person's eyes.
pub const EYES: f32 = 1.7;

/// A landblock's side, metres.
const BLOCK: f32 = 192.0;

/// Blocks of ground kept read before the ground is read afresh.
const KEPT_BLOCKS: usize = 256;

/// A key the front end hands the mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Forward,
    Back,
    Left,
    Right,
    Up,
    Down,
    Faster,
    Slower,
    /// Copy the camera's place as `@loc` writes one.
    CopyPlace,
    /// Copy the frame's report line.
    CopyReport,
}

/// What the overlay asks of the mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ask {
    /// Draw the sky at this fraction of the day.
    Time(f32),
    Weather(pyreal_client_model::cmd::weather::Weather),
    /// Cover this much of the sky with cloud, as `@clouds cover` does, or the day's own for none.
    Clouds(Option<f32>),
    /// Move the camera over the map's place `(north, east)`, keeping its height over the ground.
    Go(f64, f64),
    /// One step faster (above zero) or slower.
    Speed(i32),
    /// End the mode.
    Leave,
}

/// Where explore mode starts: the data files, the era they are drawn as (the files' own for
/// none) and the place (Holtburg for none).
#[derive(Debug, Clone, Default)]
pub struct ExploreStart {
    pub dat_dir: std::path::PathBuf,
    pub classic_dat_dir: Option<std::path::PathBuf>,
    pub era: Option<EraId>,
    pub place: Option<ReportCamera>,
}

/// What the overlay shows of the mode.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExploreView {
    /// Whose files are drawn.
    pub era: EraId,
    /// The map's coordinates under the camera, `42.2N, 33.8E`.
    pub coordinates: Option<String>,
    /// The camera's place as `@loc` writes one.
    pub place: String,
    /// Metres the camera stands above the ground.
    pub height: f32,
    /// Metres a second.
    pub speed: f32,
    /// The sky's fraction of the day, and its name for that part of the day.
    pub time: f32,
    pub time_name: String,
    pub weather: pyreal_client_model::cmd::weather::Weather,
    /// The cloud cover asked for, the day's own for none.
    pub clouds: Option<f32>,
    /// The map's coordinates under the camera as numbers, north and east.
    pub map_at: Option<(f32, f32)>,
    /// What was last copied, while it is fresh.
    pub copied: Option<String>,
}

/// Where a place given as text starts the camera: `None` for none given, which starts over
/// Holtburg.
///
/// # Errors
/// The text is no place the client writes, or it is a room or a dungeon, as a sentence.
pub fn read_place(text: &str) -> Result<Option<ReportCamera>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    if text.contains(crate::report::HEADER) {
        let r = VisualReport::parse(text)?;
        return match (r.camera, r.loc) {
            (Some(c), _) => outdoors_camera(c).map(Some),
            (None, Some(p)) => from_position(&p).map(Some),
            (None, None) => Err("The report names no place.".to_owned()),
        };
    }
    let text = text
        .strip_prefix("Your location is:")
        .unwrap_or(text)
        .trim();
    if let Some(field) = text.strip_prefix("camera ") {
        let line = format!("{} {}camera {field}", crate::report::HEADER, "; ");
        return match VisualReport::parse(&line)?.camera {
            Some(c) => outdoors_camera(c).map(Some),
            None => Err(format!("{text:?} is not a camera field.")),
        };
    }
    let text = text.strip_prefix("loc ").unwrap_or(text).trim();
    let p = parse_place(text).ok_or_else(|| {
        format!("{text:?} is not a place. Give @loc's text, a report line or its camera field.")
    })?;
    from_position(&p).map(Some)
}

/// `0xCCCCCCCC [x y z]`, with or without the turn after it.
fn parse_place(text: &str) -> Option<Position> {
    let (cell, rest) = text.split_once(' ')?;
    let cell =
        u32::from_str_radix(cell.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()?;
    let (point, turn) = rest.trim().strip_prefix('[')?.split_once(']')?;
    let numbers = |s: &str| -> Option<Vec<f32>> {
        s.replace(',', " ")
            .split_whitespace()
            .map(|n| n.parse().ok())
            .collect()
    };
    let p = numbers(point)?;
    let q = numbers(turn)?;
    let [x, y, z] = p.as_slice() else {
        return None;
    };
    let rotation = match q.as_slice() {
        [] => Quat::new(1.0, 0.0, 0.0, 0.0),
        [w, qx, qy, qz] => Quat::new(*w, *qx, *qy, *qz),
        _ => return None,
    };
    Some(Position::new(
        CellId(cell),
        Frame::new(Vec3::new(*x, *y, *z), rotation),
    ))
}

/// A camera at a body's eyes where `p` stands, facing its way and a little down.
fn from_position(p: &Position) -> Result<ReportCamera, String> {
    if p.cell.0 & 0xFFFF >= 0x100 {
        return Err(format!(
            "{:#010X} is a room or a dungeon. Explore starts outdoors.",
            p.cell.0
        ));
    }
    let q = p.frame.rotation;
    let o = p.frame.origin;
    // LINT-OK: the landblock is the top 16 bits of a cell id.
    #[allow(clippy::cast_possible_truncation)]
    let block = (p.cell.0 >> 16) as u16;
    Ok(ReportCamera {
        block,
        origin: Vec3::new(o.x, o.y, o.z + EYES),
        yaw: 2.0 * math::atan2f(q.z, q.w),
        pitch: -0.15,
        cell: None,
    })
}

/// `c`, refused when it stood in a room.
fn outdoors_camera(c: ReportCamera) -> Result<ReportCamera, String> {
    match c.cell {
        Some(cell) if cell & 0xFFFF >= 0x100 => Err(format!(
            "{cell:#010X} is a room or a dungeon. Explore starts outdoors."
        )),
        _ => Ok(ReportCamera { cell: None, ..c }),
    }
}

/// The landblock a place starts the world around: Holtburg's for none.
#[must_use]
pub fn start_block(place: Option<&ReportCamera>) -> u16 {
    place.map_or(pyreal_world_data::landblock::DEFAULT_LANDBLOCK, |c| {
        let (bx, by) = block_xy(c.block);
        // LINT-OK: a camera within a block or two of its block, as a block index.
        #[allow(clippy::cast_possible_truncation)]
        let (dx, dy) = (
            (c.origin.x / BLOCK).floor() as i32,
            (c.origin.y / BLOCK).floor() as i32,
        );
        block_id((bx + dx, by + dy))
    })
}

/// The camera in `ws` as a place: its outdoor cell, where it stands in that cell's block and its
/// turn, as `@loc` writes one.
#[must_use]
pub fn camera_place(ws: &WorldState) -> Option<Position> {
    let (vx, vy) = ws.viewer_block()?;
    let c = &ws.camera;
    // LINT-OK: block indices, at most 255, exact in f32.
    #[allow(clippy::cast_precision_loss)]
    let (gx, gy) = (
        vx as f32 * BLOCK + c.position.x,
        vy as f32 * BLOCK + c.position.y,
    );
    // LINT-OK: a point in the world as a block index, held inside it.
    #[allow(clippy::cast_possible_truncation)]
    let (bx, by) = (
        ((gx / BLOCK).floor() as i32).clamp(0, 255),
        ((gy / BLOCK).floor() as i32).clamp(0, 255),
    );
    #[allow(clippy::cast_precision_loss)]
    let local = Vec3::new(gx - bx as f32 * BLOCK, gy - by as f32 * BLOCK, c.position.z);
    let block = block_id((bx, by));
    let half = c.yaw * 0.5;
    Some(Position::new(
        CellId(outdoor_cell(block, local.x, local.y)),
        Frame::new(
            local,
            Quat::new(math::cosf(half), 0.0, 0.0, math::sinf(half)),
        ),
    ))
}

/// The map's coordinates of the outdoor cell `cell`, `42.2N, 33.8E`.
#[must_use]
pub fn coordinates(cell: CellId) -> Option<String> {
    let (north, east) = crate::hud::player_coords(cell)?;
    let ns = if north < 0.0 { 'S' } else { 'N' };
    let ew = if east < 0.0 { 'W' } else { 'E' };
    Some(format!("{:.1}{ns}, {:.1}{ew}", north.abs(), east.abs()))
}

/// The map's coordinates of `p`, north and east, to a fraction of a cell.
#[must_use]
pub fn map_at(p: &Position) -> (f32, f32) {
    let (bx, by) = block_xy(p.cell.landblock().0);
    // LINT-OK: block indices, at most 255, exact in f32.
    #[allow(clippy::cast_precision_loss)]
    let unit = |block: i32, m: f32| (block as f32 * BLOCK + m) / MAP_UNIT - MAP_ZERO;
    (unit(by, p.frame.origin.y), unit(bx, p.frame.origin.x))
}

/// Metres to one of the map's units.
const MAP_UNIT: f32 = 240.0;

/// The map's coordinate at the world's south-west corner, negated.
const MAP_ZERO: f32 = 101.95;

/// The mode's state while it runs. See the module header.
#[derive(Debug)]
pub struct Explore {
    pub era: EraId,
    /// Where it started, `None` for Holtburg: placed once the world is built.
    pub start: Option<ReportCamera>,
    speed: usize,
    /// The ground under the camera, read from the files as it flies, and the region it reads by.
    ground: Option<DatLandSource>,
    region: Option<pyreal_assets::Region>,
    /// What was copied last, and when.
    copied: Option<(String, f64)>,
    /// A copy asked for and not yet made.
    copy: Option<Key>,
    /// Metres the camera stood above the ground last frame.
    height: f32,
    /// A scripted run's own speed, metres a second (`PYREAL_EXPLORE_FLY`).
    pub scripted: Option<f32>,
}

impl Explore {
    #[must_use]
    pub fn new(era: EraId, start: Option<ReportCamera>) -> Self {
        Self {
            era,
            start,
            speed: START_SPEED,
            ground: None,
            region: None,
            copied: None,
            copy: None,
            height: 0.0,
            scripted: None,
        }
    }

    /// Metres the camera stood above the ground last frame.
    #[must_use]
    pub fn height(&self) -> f32 {
        self.height
    }

    /// Ask for a copy of the place or the report line, made once the frame is drawn.
    pub fn want_copy(&mut self, key: Key) {
        self.copy = Some(key);
    }

    /// The copy asked for, once.
    pub fn take_copy(&mut self) -> Option<Key> {
        self.copy.take()
    }

    /// Metres a second.
    #[must_use]
    pub fn speed(&self) -> f32 {
        if let Some(v) = self.scripted {
            return v.clamp(0.0, MOST_SPEED);
        }
        SPEEDS[self.speed.min(SPEEDS.len() - 1)]
    }

    /// `steps` speeds faster, or slower below zero, held between the slowest and the fastest.
    pub fn change_speed(&mut self, steps: i32) {
        let last = i32::try_from(SPEEDS.len() - 1).unwrap_or(0);
        let at = i32::try_from(self.speed).unwrap_or(0).saturating_add(steps);
        self.speed = usize::try_from(at.clamp(0, last)).unwrap_or(0);
    }

    /// Note that `what` was copied at `now`, for the overlay to say.
    pub fn note_copied(&mut self, what: &str, now: f64) {
        self.copied = Some((what.to_owned(), now));
    }

    /// What was copied, for three seconds after.
    #[must_use]
    pub fn copied(&self, now: f64) -> Option<&str> {
        self.copied
            .as_ref()
            .filter(|(_, at)| now - at < 3.0)
            .map(|(what, _)| what.as_str())
    }

    /// The ground's height under global `(x, y)`, metres.
    fn ground_height(&mut self, store: &Arc<RetailDatStore>, x: f32, y: f32) -> Option<f32> {
        if self
            .ground
            .as_ref()
            .is_some_and(|g| g.cached_block_count() > KEPT_BLOCKS)
        {
            self.ground = None;
        }
        if self.ground.is_none() {
            if self.region.is_none() {
                self.region = pyreal_world_data::landblock::load_region(store).ok();
            }
            self.ground = DatLandSource::new(Arc::clone(store), self.region.as_ref()?).ok();
        }
        // LINT-OK: a point in the world as a block index.
        #[allow(clippy::cast_possible_truncation)]
        let (bx, by) = ((x / BLOCK).floor() as i32, (y / BLOCK).floor() as i32);
        #[allow(clippy::cast_precision_loss)]
        let (lx, ly) = (x - bx as f32 * BLOCK, y - by as f32 * BLOCK);
        let id = LandblockId(block_id((bx, by)));
        self.ground.as_ref()?.ground_height(id, lx, ly)
    }

    /// Move the camera in `ws` over the map's `(north, east)`, as high over the ground there as it
    /// stands over the ground here, at a body's eyes at least. The window follows it next frame.
    pub fn go(&mut self, ws: &mut WorldState, store: &Arc<RetailDatStore>, north: f64, east: f64) {
        let Some((vx, vy)) = ws.viewer_block() else {
            return;
        };
        let edge = 256.0 * BLOCK - 0.01;
        // LINT-OK: a map coordinate within the world, as metres.
        #[allow(clippy::cast_possible_truncation)]
        let at =
            |u: f64| (((u + f64::from(MAP_ZERO)) * f64::from(MAP_UNIT)) as f32).clamp(0.0, edge);
        let (gx, gy) = (at(east), at(north));
        let above = self.height.max(EYES);
        let ground = self.ground_height(store, gx, gy).unwrap_or(0.0);
        #[allow(clippy::cast_precision_loss)]
        let (ox, oy) = (vx as f32 * BLOCK, vy as f32 * BLOCK);
        let c = &mut ws.camera;
        c.position = Vec3::new(gx - ox, gy - oy, ground + above);
        self.height = above;
    }

    /// Keep the camera in `ws` inside the world and [`ROOM`] above its ground. The camera's height
    /// above the ground, where it is known.
    pub fn hold_over_ground(
        &mut self,
        ws: &mut WorldState,
        store: &Arc<RetailDatStore>,
    ) -> Option<f32> {
        let (vx, vy) = ws.viewer_block()?;
        #[allow(clippy::cast_precision_loss)]
        let (ox, oy) = (vx as f32 * BLOCK, vy as f32 * BLOCK);
        let edge = 256.0 * BLOCK - 0.01;
        let c = &mut ws.camera;
        c.position.x = (c.position.x + ox).clamp(0.0, edge) - ox;
        c.position.y = (c.position.y + oy).clamp(0.0, edge) - oy;
        let ground = self.ground_height(store, c.position.x + ox, c.position.y + oy)?;
        let c = &mut ws.camera;
        c.position.z = c.position.z.max(ground + ROOM);
        self.height = c.position.z - ground;
        Some(self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behaviour: none (Pyreal's own: where explore mode starts)
    #[test]
    fn a_place_reads_from_loc_text_a_report_line_and_its_fields() {
        let at = |text: &str| read_place(text).expect(text).expect("a place");
        let loc = at("Your location is: 0xA9B40017 [84.000000 7.100000 94.005000] 1.000000 0.000000 0.000000 0.000000");
        assert_eq!(loc.block, 0xA9B4);
        assert!(
            (loc.origin.z - (94.005 + EYES)).abs() < 1e-3,
            "at a body's eyes"
        );
        assert!(loc.yaw.abs() < 1e-6, "facing north");
        let bare = at("0xA9B40017 [84 7.1 94]");
        assert_eq!((bare.block, bare.origin.x), (0xA9B4, 84.0));
        let facing_east = at("loc 0xA9B40017 [84 7.1 94] 0.707107 0 0 -0.707107");
        assert!((facing_east.yaw + std::f32::consts::FRAC_PI_2).abs() < 1e-3);
        let camera = at("camera 0xA8B3 [182.2 -0.1 91.25] yaw -1.5 pitch -0.26");
        assert_eq!(
            (camera.block, camera.yaw, camera.pitch),
            (0xA8B3, -1.5, -0.26)
        );
        let line = at("dereth report 1; loc 0xA8B3003A [186.3 32.4 89.7] 1 0 0 0; camera 0xA8B3 [182.2 -0.1 91.25] yaw -1.5 pitch -0.26 cell 0xA8B3003A; mode chase");
        assert_eq!(
            line.origin,
            Vec3::new(182.2, -0.1, 91.25),
            "the report's camera"
        );
        assert_eq!(read_place("  ").unwrap(), None, "none starts over Holtburg");
    }

    /// Behaviour: none (Pyreal's own: a place explore mode refuses says why)
    #[test]
    fn a_room_or_text_that_is_no_place_is_refused_with_why() {
        let room = read_place("0x01D90108 [20 -30 0] 1 0 0 0").unwrap_err();
        assert!(
            room.contains("0x01D90108") && room.contains("outdoors"),
            "{room}"
        );
        let nothing = read_place("Holtburg please").unwrap_err();
        assert!(nothing.contains("not a place"), "{nothing}");
        assert!(read_place("dereth report 1; mode free").is_err());
    }

    /// Behaviour: none (Pyreal's own: explore mode's speeds)
    #[test]
    fn the_speed_steps_from_a_walk_to_a_kilometre_a_second_and_holds_there() {
        let mut e = Explore::new(EraId::Eor, None);
        assert_eq!(e.speed(), 25.0);
        e.change_speed(-10);
        assert_eq!(e.speed(), 2.0);
        e.change_speed(100);
        assert_eq!(e.speed(), 1000.0);
        assert_eq!(e.speed(), MOST_SPEED);
        e.change_speed(-1);
        assert_eq!(e.speed(), 500.0);
        e.scripted = Some(5000.0);
        assert_eq!(e.speed(), MOST_SPEED, "a scripted flight is held to it too");
    }

    /// Behaviour: none (Pyreal's own: the block explore mode starts round)
    #[test]
    fn a_place_starts_the_world_round_its_own_block_and_names_its_coordinates() {
        let block = start_block(Some(&ReportCamera {
            block: 0xA9B4,
            origin: Vec3::new(200.0, -10.0, 50.0),
            yaw: 0.0,
            pitch: 0.0,
            cell: None,
        }));
        assert_eq!(
            block, 0xAAB3,
            "a camera past its block's corner starts the world there"
        );
        assert_eq!(start_block(None), 0xA9B4);
        let holtburg = Position::new(
            CellId(0xA9B4_0017),
            Frame::new(Vec3::new(84.0, 7.1, 94.0), Quat::new(1.0, 0.0, 0.0, 0.0)),
        );
        let (ns, ew) = map_at(&holtburg);
        assert!(
            (ns - 42.08).abs() < 0.01 && (ew - 33.6).abs() < 0.01,
            "{ns} {ew}"
        );
        assert_eq!(
            coordinates(CellId(0xA9B4_0001)).as_deref(),
            Some("42.1N, 33.3E")
        );
    }
}
