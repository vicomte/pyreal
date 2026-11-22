//! Interiors: the environment cells' volumes and portals in render space, the portal walk that
//! decides which cells and whether the outside are seen, and the volumes the ground is cut by.
//!
//! A cell's cell BSP is a chain of planes whose positive half-spaces meet in the room's convex
//! volume (`pyreal_physics::geom::bsp`). That volume serves three things here: finding the cell
//! a camera stands in, cutting the ground (the terrain shader discards whatever lies inside a
//! volume, so no ground passes through a floor or fills a cellar's stairwell), and nothing else:
//! drawing order is the depth buffer's.
//!
//! Every placed instance carries a cell slot: [`OUTSIDE`] for the ground, scenery, statics and
//! buildings' shells, the cell's own slot for its room and furniture. Each frame the walk gives
//! each slot a screen rectangle (empty when unseen); the cull pass tests an instance against its
//! slot's rectangle as well as the frustum, so visibility narrows the draw list and never orders it.

use std::collections::HashMap;

use glam::{Mat4, Vec3, Vec4, Vec4Swizzles};

/// The slot of everything outdoors.
pub const OUTSIDE: u32 = 0;

/// A screen rectangle in normalised device coordinates, `[x0, y0, x1, y1]`.
pub type Rect = [f32; 4];
/// Nothing seen.
pub const EMPTY: Rect = [1.0, 1.0, -1.0, -1.0];
/// The whole screen.
pub const FULL: Rect = [-1.0, -1.0, 1.0, 1.0];

/// Metres a point may stand off a plane and still count as on its inner side.
const SIDE_TOLERANCE: f32 = 0.05;
/// Within this distance of a portal's plane, and over its opening, the eye sees through it whole.
const IN_DOORWAY: f32 = 0.3;
/// Pops before the walk stops growing rectangles.
const WALK_LIMIT: usize = 20_000;

#[must_use]
pub fn is_empty(r: &Rect) -> bool {
    r[0] >= r[2] || r[1] >= r[3]
}

#[must_use]
pub fn intersect(a: &Rect, b: &Rect) -> Rect {
    [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ]
}

#[must_use]
pub fn union(a: &Rect, b: &Rect) -> Rect {
    if is_empty(a) {
        return *b;
    }
    if is_empty(b) {
        return *a;
    }
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

fn contains(outer: &Rect, inner: &Rect) -> bool {
    is_empty(inner)
        || (!is_empty(outer)
            && outer[0] <= inner[0]
            && outer[1] <= inner[1]
            && outer[2] >= inner[2]
            && outer[3] >= inner[3])
}

/// A convex volume in render space: inside where every plane's `n · p + d >= 0`.
#[derive(Debug, Clone, PartialEq)]
pub struct Volume {
    pub planes: Vec<Vec4>,
    pub min: Vec3,
    pub max: Vec3,
    /// The mean of its corners, which a convex volume holds.
    pub center: Vec3,
}

impl Volume {
    /// Whether `p` is inside, `slack` metres of grace on every plane.
    #[must_use]
    pub fn contains(&self, p: Vec3, slack: f32) -> bool {
        p.cmpge(self.min - slack).all()
            && p.cmple(self.max + slack).all()
            && self
                .planes
                .iter()
                .all(|pl| pl.xyz().dot(p) + pl.w >= -slack)
    }

    /// Moves the volume by `delta`.
    pub fn translate(&mut self, delta: Vec3) {
        self.min += delta;
        self.max += delta;
        self.center += delta;
        for p in &mut self.planes {
            p.w -= p.xyz().dot(delta);
        }
    }

    /// The volume's words for the cut buffer: its box with the plane count and the reach past
    /// which it no longer cuts (see [`Cell::reach`]), then the planes.
    pub fn write(&self, reach: f32, out: &mut Vec<[f32; 4]>) {
        let n = u32::try_from(self.planes.len()).unwrap_or(0);
        out.push([self.min.x, self.min.y, self.min.z, f32::from_bits(n)]);
        out.push([self.max.x, self.max.y, self.max.z, reach.min(f32::MAX)]);
        out.extend(self.planes.iter().map(|p| p.to_array()));
    }
}

/// One portal: its opening in render space, its plane facing into the cell that owns it, and the
/// slot it leads to (`None` for the outside).
#[derive(Debug, Clone, PartialEq)]
pub struct Portal {
    pub points: Vec<Vec3>,
    pub plane: Vec4,
    pub to: Option<u32>,
    /// The dat id it leads to, until [`CellGraph::link`] resolves it.
    pub to_id: Option<u32>,
}

impl Portal {
    /// A portal from its opening, its plane turned to face `inside`.
    #[must_use]
    pub fn new(points: Vec<Vec3>, inside: Vec3, to_id: Option<u32>) -> Self {
        let mut n = Vec3::ZERO;
        for (i, a) in points.iter().enumerate() {
            let b = points[(i + 1) % points.len()];
            n += Vec3::new(
                (a.y - b.y) * (a.z + b.z),
                (a.z - b.z) * (a.x + b.x),
                (a.x - b.x) * (a.y + b.y),
            );
        }
        let n = n.normalize_or_zero();
        #[allow(clippy::cast_precision_loss)]
        let c = points.iter().copied().sum::<Vec3>() / points.len().max(1) as f32;
        let mut plane = n.extend(-n.dot(c));
        if plane.xyz().dot(inside) + plane.w < 0.0 {
            plane = -plane;
        }
        Self {
            points,
            plane,
            to: None,
            to_id,
        }
    }

    fn side(&self, p: Vec3) -> f32 {
        self.plane.xyz().dot(p) + self.plane.w
    }

    /// Whether `eye` stands in the opening: near its plane and over its outline's box.
    fn in_doorway(&self, eye: Vec3) -> bool {
        if self.side(eye).abs() > IN_DOORWAY {
            return false;
        }
        let (lo, hi) = self
            .points
            .iter()
            .fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| {
                (lo.min(*p), hi.max(*p))
            });
        eye.cmpge(lo - IN_DOORWAY).all() && eye.cmple(hi + IN_DOORWAY).all()
    }

    /// The opening's screen rectangle seen through `view_proj`, clipped to the near side.
    #[must_use]
    pub fn screen_rect(&self, view_proj: &Mat4) -> Rect {
        const NEAR_W: f32 = 1e-3;
        let clip: Vec<Vec4> = self
            .points
            .iter()
            .map(|p| *view_proj * p.extend(1.0))
            .collect();
        let (mut lo, mut hi) = (glam::Vec2::MAX, glam::Vec2::MIN);
        let mut add = |c: Vec4| {
            let p = c.xy() / c.w;
            lo = lo.min(p);
            hi = hi.max(p);
        };
        for (i, a) in clip.iter().enumerate() {
            let b = clip[(i + 1) % clip.len()];
            if a.w >= NEAR_W {
                add(*a);
            }
            if (a.w >= NEAR_W) != (b.w >= NEAR_W) {
                let t = (NEAR_W - a.w) / (b.w - a.w);
                add(*a + (b - *a) * t);
            }
        }
        if lo.cmpgt(hi).any() {
            return EMPTY;
        }
        intersect(&[lo.x, lo.y, hi.x, hi.y], &FULL)
    }
}

/// One environment cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    /// The dat cell id, `0xXXYY0100` and up.
    pub id: u32,
    pub portals: Vec<Portal>,
    /// Its room's volume; `None` when its cell BSP is empty.
    pub volume: Option<Volume>,
    pub seen_outside: bool,
    /// How far (in degrade metres) its building's shell is drawn: past it the shell degrades to
    /// nothing, and the room is not looked into from outdoors.
    pub reach: f32,
}

/// What the walk decided for one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Visibility {
    /// Per slot, [`OUTSIDE`] first.
    pub rects: Vec<Rect>,
    /// The cell the eye stands in, `None` outdoors.
    pub eye_cell: Option<u32>,
}

impl Visibility {
    /// Everything seen whole: no cells known.
    #[must_use]
    pub fn everything(slots: usize) -> Self {
        Self {
            rects: vec![FULL; slots.max(1)],
            eye_cell: None,
        }
    }

    #[must_use]
    pub fn outside(&self) -> bool {
        self.rects.first().is_some_and(|r| !is_empty(r))
    }

    /// Slots seen, the outside not counted (a room's spill slot counts as one).
    #[must_use]
    pub fn cells_seen(&self) -> usize {
        self.rects.iter().skip(1).filter(|r| !is_empty(r)).count()
    }
}

/// A room's furniture that reaches past its walls (stairs that climb into the next room): seen
/// through its own slot wherever the room or any room it reaches into is seen, as retail lists a
/// static object in every cell its sphere touches.
#[derive(Debug, Clone)]
struct Spill {
    /// Its slot, which holds no cell.
    slot: u32,
    /// The room it stands in.
    home: u32,
    /// Its pieces' spheres.
    spheres: Vec<(Vec3, f32)>,
    /// The rooms the spheres reach into through portals, once linked.
    into: Vec<u32>,
}

/// The loaded cells, by slot (slot `k` is `cells[k - 1]`; a removed cell's slot is reused).
#[derive(Debug, Clone, Default)]
pub struct CellGraph {
    cells: Vec<Option<Cell>>,
    free: Vec<u32>,
    // ORDER-OK: looked up by dat id, never iterated.
    by_id: HashMap<u32, u32>,
    spills: Vec<Spill>,
}

impl CellGraph {
    /// Adds `cell`; its slot.
    pub fn add(&mut self, cell: Cell) -> u32 {
        let id = cell.id;
        let slot = self.take_slot(Some(cell));
        self.by_id.insert(id, slot);
        slot
    }

    fn take_slot(&mut self, cell: Option<Cell>) -> u32 {
        match self.free.pop() {
            Some(slot) => {
                self.cells[slot as usize - 1] = cell;
                slot
            }
            None => {
                self.cells.push(cell);
                u32::try_from(self.cells.len()).unwrap_or(u32::MAX)
            }
        }
    }

    /// The slot a piece of furniture in room `home` with bounding sphere `center`, `radius` is
    /// seen through: `home`'s own when the sphere stays within its volume, else the room's
    /// spill slot ([`Spill`]), which [`Self::link`] gives the rooms it reaches.
    pub fn furniture_slot(&mut self, home: u32, center: Vec3, radius: f32) -> u32 {
        let inside = self
            .cell(home)
            .and_then(|c| c.volume.as_ref())
            .is_none_or(|v| v.planes.iter().all(|p| p.xyz().dot(center) + p.w >= radius));
        if inside {
            return home;
        }
        if let Some(s) = self.spills.iter_mut().find(|s| s.home == home) {
            s.spheres.push((center, radius));
            return s.slot;
        }
        let slot = self.take_slot(None);
        self.spills.push(Spill {
            slot,
            home,
            spheres: vec![(center, radius)],
            into: Vec::new(),
        });
        slot
    }

    /// The rooms `spill`'s spheres reach from its room through portals, each crossing a portal's
    /// opening into a room whose volume it touches.
    fn reached(&self, spill: &Spill) -> Vec<u32> {
        let mut into: Vec<u32> = Vec::new();
        let mut open = vec![spill.home];
        while let Some(slot) = open.pop() {
            let Some(cell) = self.cell(slot) else {
                continue;
            };
            for p in &cell.portals {
                let Some(next) = p.to else { continue };
                if next == spill.home || into.contains(&next) {
                    continue;
                }
                let (lo, hi) = p.points.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), q| {
                    (lo.min(*q), hi.max(*q))
                });
                let touches = self
                    .cell(next)
                    .and_then(|c| c.volume.as_ref())
                    .is_some_and(|v| {
                        spill.spheres.iter().any(|&(c, r)| {
                            p.side(c) < r
                                && c.cmpge(lo - r).all()
                                && c.cmple(hi + r).all()
                                && c.cmpge(v.min - r).all()
                                && c.cmple(v.max + r).all()
                                && v.planes.iter().all(|q| q.xyz().dot(c) + q.w >= -r)
                        })
                    });
                if touches {
                    into.push(next);
                    open.push(next);
                }
            }
        }
        into
    }

    fn link_spills(&mut self, homes: Option<&[u32]>) {
        let mut spills = std::mem::take(&mut self.spills);
        for s in &mut spills {
            if homes.is_none_or(|h| h.contains(&s.home)) {
                s.into = self.reached(s);
            }
        }
        self.spills = spills;
    }

    /// Takes the cell in `slot` out; its slot is reused. Portals into it lead nowhere once
    /// [`Self::link_slots`] has run over them (a block's rooms leave together).
    pub fn remove(&mut self, slot: u32) {
        let Some(c) = (slot as usize)
            .checked_sub(1)
            .and_then(|i| self.cells.get_mut(i))
            .and_then(Option::take)
        else {
            return;
        };
        if self.by_id.get(&c.id) == Some(&slot) {
            self.by_id.remove(&c.id);
        }
        self.free.push(slot);
        let free = &mut self.free;
        self.spills.retain(|s| {
            if s.home == slot {
                free.push(s.slot);
            }
            s.home != slot
        });
    }

    /// Resolves every portal's dat id to a loaded slot; a portal to a cell not loaded leads
    /// nowhere.
    pub fn link(&mut self) {
        for c in self.cells.iter_mut().flatten() {
            for p in &mut c.portals {
                p.to = p.to_id.and_then(|id| self.by_id.get(&id).copied());
            }
        }
        self.link_spills(None);
    }

    /// [`Self::link`] for the cells in `slots` alone (one block's rooms, which lead only to each
    /// other and outdoors).
    pub fn link_slots(&mut self, slots: &[u32]) {
        for &slot in slots {
            let Some(Some(c)) = (slot as usize)
                .checked_sub(1)
                .and_then(|i| self.cells.get_mut(i))
            else {
                continue;
            };
            for p in &mut c.portals {
                p.to = p.to_id.and_then(|id| self.by_id.get(&id).copied());
            }
        }
        self.link_spills(Some(slots));
    }

    /// Moves every cell by `delta`.
    pub fn translate(&mut self, delta: Vec3) {
        for c in self.cells.iter_mut().flatten() {
            if let Some(v) = c.volume.as_mut() {
                v.translate(delta);
            }
            for p in &mut c.portals {
                for q in &mut p.points {
                    *q += delta;
                }
                p.plane.w -= p.plane.xyz().dot(delta);
            }
        }
        for s in &mut self.spills {
            for (c, _) in &mut s.spheres {
                *c += delta;
            }
        }
    }

    #[must_use]
    pub fn slot_of(&self, id: u32) -> Option<u32> {
        self.by_id.get(&id).copied()
    }

    #[must_use]
    pub fn cell(&self, slot: u32) -> Option<&Cell> {
        self.cells.get((slot as usize).checked_sub(1)?)?.as_ref()
    }

    /// The loaded cells.
    #[must_use]
    pub fn cells(&self) -> Vec<&Cell> {
        self.cells.iter().flatten().collect()
    }

    /// Slots, the outside's included.
    #[must_use]
    pub fn slots(&self) -> usize {
        self.cells.len() + 1
    }

    /// The rooms `v` saw.
    #[must_use]
    pub fn rooms_seen(&self, v: &Visibility) -> usize {
        v.rects
            .iter()
            .enumerate()
            .skip(1)
            .filter(|&(k, r)| {
                !is_empty(r) && u32::try_from(k).is_ok_and(|k| self.cell(k).is_some())
            })
            .count()
    }

    /// The rooms a lamp in `slot` at `at` lights: its own and those its light reaches through
    /// openings within `range`. `None` lights everywhere: a lamp outdoors, one whose light reaches
    /// the outside, or one reaching more than `most` rooms.
    #[must_use]
    pub fn lamp_rooms(&self, slot: u32, at: Vec3, range: f32, most: usize) -> Option<Vec<u32>> {
        if slot == OUTSIDE {
            return None;
        }
        self.cell(slot)?;
        let mut rooms = vec![slot];
        let mut next = 0;
        while let Some(&here) = rooms.get(next) {
            next += 1;
            for p in &self.cell(here)?.portals {
                let (lo, hi) = p.points.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), q| {
                    (lo.min(*q), hi.max(*q))
                });
                if at.clamp(lo, hi).distance(at) >= range {
                    continue;
                }
                let to = p.to?;
                if !rooms.contains(&to) {
                    if rooms.len() == most {
                        return None;
                    }
                    rooms.push(to);
                }
            }
        }
        Some(rooms)
    }

    /// The cell holding `p`: `slot` itself, a room its portals lead to, or any; `slot` when none
    /// does.
    #[must_use]
    pub fn cell_near(&self, slot: u32, p: Vec3) -> u32 {
        let holds = |k: u32| {
            self.cell(k)
                .and_then(|c| c.volume.as_ref())
                .is_some_and(|v| v.contains(p, 0.0))
        };
        if slot == OUTSIDE || holds(slot) {
            return slot;
        }
        self.cell(slot)
            .and_then(|c| c.portals.iter().filter_map(|q| q.to).find(|&k| holds(k)))
            .or_else(|| self.cell_at(p))
            .unwrap_or(slot)
    }

    /// The cell whose volume holds `p`, the first by slot.
    #[must_use]
    pub fn cell_at(&self, p: Vec3) -> Option<u32> {
        self.cells
            .iter()
            .position(|c| {
                c.as_ref()
                    .and_then(|c| c.volume.as_ref())
                    .is_some_and(|v| v.contains(p, 0.0))
            })
            .and_then(|i| u32::try_from(i + 1).ok())
    }

    /// The slot the eye starts in: the cell `hint` names when it is loaded (an outdoor cell id
    /// means outdoors), else the cell holding `eye`.
    #[must_use]
    pub fn eye_cell(&self, eye: Vec3, hint: Option<u32>) -> Option<u32> {
        match hint {
            Some(id) if id & 0xFFFF < 0x100 => None,
            Some(id) => self.slot_of(id).or_else(|| self.cell_at(eye)),
            None => self.cell_at(eye),
        }
    }

    /// Walks the portals from the eye's cell or from outdoors, and from the outdoors wherever it is
    /// reached into the rooms through every opening within `reach` metres (and within its room's
    /// own reach once scaled by `lod_scale` as the degrade levels are), and gives each slot the
    /// screen rectangle it is seen through.
    #[must_use]
    pub fn visibility(
        &self,
        view_proj: &Mat4,
        eye: Vec3,
        hint: Option<u32>,
        reach: f32,
        lod_scale: f32,
    ) -> Visibility {
        let mut rects = vec![EMPTY; self.slots()];
        let mut stack: Vec<(u32, Rect)> = Vec::new();
        let eye_cell = self.eye_cell(eye, hint);
        stack.push((eye_cell.unwrap_or(OUTSIDE), FULL));
        let mut pops = 0;
        while let Some((slot, r)) = stack.pop() {
            pops += 1;
            if pops > WALK_LIMIT {
                break;
            }
            let Some(seen) = rects.get_mut(slot as usize) else {
                continue;
            };
            if contains(seen, &r) {
                continue;
            }
            *seen = union(seen, &r);
            let r = *seen;
            if slot == OUTSIDE {
                self.enter_from_outside(&r, view_proj, eye, reach, lod_scale, &mut stack);
                continue;
            }
            let Some(cell) = self.cell(slot) else {
                continue;
            };
            for p in &cell.portals {
                if p.side(eye) < -SIDE_TOLERANCE {
                    continue;
                }
                let through = if p.in_doorway(eye) {
                    r
                } else {
                    intersect(&r, &p.screen_rect(view_proj))
                };
                if is_empty(&through) {
                    continue;
                }
                match (p.to, p.to_id) {
                    (Some(next), _) => stack.push((next, through)),
                    (None, None) => stack.push((OUTSIDE, through)),
                    // A portal to a cell that is not loaded.
                    (None, Some(_)) => {}
                }
            }
        }
        for s in &self.spills {
            let r = s.into.iter().fold(rects[s.home as usize], |r, &k| {
                union(&r, &rects[k as usize])
            });
            rects[s.slot as usize] = r;
        }
        Visibility { rects, eye_cell }
    }

    /// Pushes each room seen from outdoors through `r`: through every opening within `reach`
    /// metres and its room's own reach, the eye on its outer side.
    fn enter_from_outside(
        &self,
        r: &Rect,
        view_proj: &Mat4,
        eye: Vec3,
        reach: f32,
        lod_scale: f32,
        stack: &mut Vec<(u32, Rect)>,
    ) {
        for (i, c) in self.cells.iter().enumerate() {
            let Some(c) = c else { continue };
            let slot = u32::try_from(i + 1).unwrap_or(0);
            for p in c.portals.iter().filter(|p| p.to_id.is_none()) {
                let d = p
                    .points
                    .iter()
                    .map(|q| q.distance(eye))
                    .fold(f32::MAX, f32::min);
                let near = d < reach && d * lod_scale < c.reach;
                if !near || p.side(eye) > SIDE_TOLERANCE {
                    continue;
                }
                let through = if p.in_doorway(eye) {
                    *r
                } else {
                    intersect(r, &p.screen_rect(view_proj))
                };
                if !is_empty(&through) {
                    stack.push((slot, through));
                }
            }
        }
    }

    /// Limits how far the room `id` is looked into from outdoors (see [`Cell::reach`]).
    pub fn limit_reach(&mut self, id: u32, reach: f32) {
        if let Some(Some(c)) = self
            .slot_of(id)
            .and_then(|s| self.cells.get_mut(s as usize - 1))
        {
            c.reach = c.reach.min(reach);
        }
    }

    /// The volumes of every cell, for cutting the ground.
    pub fn volumes(&self) -> impl Iterator<Item = &Volume> {
        self.cells
            .iter()
            .flatten()
            .filter_map(|c| c.volume.as_ref())
    }
}
