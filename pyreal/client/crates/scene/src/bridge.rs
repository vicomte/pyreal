//! The world handed to the engine: the runtime's world state ([`pyreal_client_runtime::world_state::WorldState`]) is the truth, and
//! each frame the bridge makes the engine's scene say what it says.
//!
//! - **The land** is the engine's own, streamed from the dats block by block round the viewer's
//!   block as it moves ([`pyreal_engine::stream`]: ground, statics, buildings, scenery, rooms, far
//!   terrain and trees), within a few milliseconds a frame; a jump past the loaded square (a
//!   teleport, a portal, a dungeon) loads the square round the new place at once. The client's
//!   render space is relative to the viewer's landblock and the engine's to the stream's origin
//!   block, so every position crosses by one offset.
//! - **Server objects and the player** are posed by the runtime's own motion drivers; their parts
//!   become dynamic instances, dressed as the part array says (graphics object, texture changes,
//!   palettes), each carrying the object's number, which maps back to its GUID for picking.
//! - **An art pack's body** ([`pyreal_engine::skin`]) is drawn in place of the parts it names,
//!   for the objects whose setup and look the pack has one for, unless the pack keeps them.
//! - **Their particles** are the drivers' own emitters, gathered each frame; **their lamps** move
//!   with them.

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Affine3A, Vec3};
use pyreal_dat::RetailDatStore;
use pyreal_engine::body::part_affine;
use pyreal_engine::cells::OUTSIDE;
use pyreal_engine::scene::Instance;
use pyreal_engine::stream::Stream;
use pyreal_engine::world::{AssetLoader, Dress, HeadLevel};
use pyreal_engine::{Camera, Engine, SkyLight};
use pyreal_primitives::{CellId, DataId, Frame, ObjectId};

use pyreal_client_runtime::world_state::WorldState;

/// Object numbers from here on are server objects' (and the player's); below are the dats'
/// statics, which the engine numbers itself.
pub const SERVER_NUMBERS: u32 = 0x8000_0000;

/// How far the loaded world reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reach {
    /// Blocks of exact ground, statics and scenery each way round the centre.
    pub radius: i32,
    /// Blocks of far terrain each way; no further than `radius` for none.
    pub far: i32,
}

impl Default for Reach {
    fn default() -> Self {
        Self { radius: 4, far: 12 }
    }
}

/// One drawn object: its number and, per part, what it was dressed as and its instance; or the
/// art pack's model drawn in place of all its parts, and that model's instance.
#[derive(Debug)]
struct Drawn {
    number: u32,
    /// Each part as drawn: its model, its dress, and a player's head's level of detail.
    parts: Vec<Option<(DataId, Dress, Option<HeadLevel>)>>,
    instances: Vec<Option<u32>>,
    whole: Option<(DataId, u32)>,
    /// The art pack's body drawn in place of its parts, if any.
    skinned: Option<pyreal_engine::skin::SkinnedBody>,
    /// Its setup, where an older world draws its parts from End of Retail's files
    /// ([`AssetLoader::later_part`]).
    later: Option<DataId>,
}

impl Drawn {
    fn new(number: u32) -> Self {
        Self {
            number,
            parts: Vec::new(),
            instances: Vec::new(),
            whole: None,
            skinned: None,
            later: None,
        }
    }

    /// Every instance it has in the scene but its body's.
    fn all_instances(&self) -> impl Iterator<Item = u32> + '_ {
        self.instances
            .iter()
            .flatten()
            .copied()
            .chain(self.whole.map(|(_, i)| i))
    }

    /// Takes everything it drew out of the engine.
    fn remove(self, engine: &mut Engine) {
        for i in self.all_instances() {
            engine.scene.remove_instance(i);
        }
        if let Some(b) = self.skinned {
            b.remove(engine);
        }
    }
}

/// What the last sync said about the frame.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SyncCounts {
    pub objects: usize,
    pub instances: usize,
    /// Server objects drawn whole as the art pack's model of their setup.
    pub packed: usize,
    /// Squares of land loaded at once (the first, and after each jump).
    pub reloads: u64,
    /// How long the last such load took, ms.
    pub reload_ms: f64,
    /// How long this frame's [`Bridge::prepare`] took, ms.
    pub prepare_ms: f64,
}

/// The engine's view of one loaded world.
pub struct Bridge {
    pub store: Arc<RetailDatStore>,
    pub region: Arc<pyreal_engine::world::Region>,
    pub reach: Reach,
    /// The land, once anything is loaded.
    pub stream: Option<Stream>,
    /// The block the client's render space is round this frame.
    viewer: (i32, i32),
    heavens: bool,
    // ORDER-OK: looked up by id; the instances are removed in any order.
    objects: HashMap<ObjectId, Drawn>,
    player: Option<Drawn>,
    // ORDER-OK: looked up by number.
    numbers: HashMap<u32, ObjectId>,
    next_number: u32,
    /// The engine's own lamps (the dats' statics'), before the moving objects' are added.
    static_lamps: usize,
    /// Each server object's setup, as its create named it, for its lamps.
    // ORDER-OK: looked up by id.
    pub setups: HashMap<ObjectId, DataId>,
    /// Each setup's ground clearing at scale one, as worked out.
    // ORDER-OK: a memo, looked up by id.
    grounds: HashMap<u32, Option<f32>>,
    pub counts: SyncCounts,
    /// Blocks the eye is carried from the viewer's block (`PYREAL_FLY`, a measured traversal).
    pub nudge: (i32, i32),
    /// Every frame places all the land wanted before it is drawn (a headless capture).
    pub settle: bool,
    /// Portal space covers the world: a jump is placed over several frames.
    pub staged: bool,
    /// Landscape Smoothing's strength (0 off): the ground drawn smoothed and what stands on it
    /// moved with it.
    pub smoothing: u8,
    /// The objects that have held a launcher, for an art pack that keeps them as they are.
    // ORDER-OK: looked up by id.
    held_launchers: std::collections::HashSet<ObjectId>,
    /// When the last sync ran, for how far bodies' clips step.
    last_sync: Option<std::time::Instant>,
}

impl std::fmt::Debug for Bridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bridge")
            .field("centre", &self.centre())
            .field("objects", &self.objects.len())
            .field("counts", &self.counts)
            .finish_non_exhaustive()
    }
}

/// A block's id from its grid coordinates.
fn block_id((x, y): (i32, i32)) -> u16 {
    u16::try_from(((x & 0xFF) << 8) | (y & 0xFF)).unwrap_or(0)
}

impl Bridge {
    #[must_use]
    pub fn new(
        store: Arc<RetailDatStore>,
        region: pyreal_engine::world::Region,
        reach: Reach,
    ) -> Self {
        Self::new_with(store, Arc::new(region), reach)
    }

    fn new_with(
        store: Arc<RetailDatStore>,
        region: Arc<pyreal_engine::world::Region>,
        reach: Reach,
    ) -> Self {
        Self {
            store,
            region,
            reach,
            stream: None,
            viewer: (0, 0),
            heavens: false,
            objects: HashMap::new(),
            player: None,
            numbers: HashMap::new(),
            next_number: SERVER_NUMBERS,
            static_lamps: 0,
            setups: HashMap::new(),
            grounds: HashMap::new(),
            counts: SyncCounts::default(),
            nudge: (0, 0),
            settle: false,
            staged: false,
            smoothing: 0,
            held_launchers: std::collections::HashSet::new(),
            last_sync: None,
        }
    }

    /// The same world, nothing of it placed: what a new engine loads it into.
    #[must_use]
    pub fn again(&self) -> Self {
        Self {
            setups: self.setups.clone(),
            nudge: self.nudge,
            settle: self.settle,
            staged: self.staged,
            smoothing: self.smoothing,
            ..Self::new_with(
                Arc::clone(&self.store),
                Arc::clone(&self.region),
                self.reach,
            )
        }
    }

    /// The block the loaded square is round.
    #[must_use]
    pub fn centre(&self) -> Option<(i32, i32)> {
        self.stream.as_ref()?.centre()
    }

    /// The client's render space to the engine's: add this.
    #[must_use]
    pub fn offset(&self) -> Vec3 {
        let (cx, cy) = self
            .stream
            .as_ref()
            .filter(|s| s.centre().is_some())
            .map_or(self.viewer, Stream::origin);
        #[allow(clippy::cast_precision_loss)]
        Vec3::new(
            ((self.viewer.0 - cx) * 192) as f32,
            ((self.viewer.1 - cy) * 192) as f32,
            0.0,
        )
    }

    /// The server object (or the player) drawn with object number `n`.
    #[must_use]
    pub fn object_of(&self, n: u32) -> Option<ObjectId> {
        self.numbers.get(&n).copied()
    }

    /// Object `id`'s number, when it is drawn.
    #[must_use]
    pub fn number_of(&self, id: ObjectId) -> Option<u32> {
        if let Some(d) = self.objects.get(&id) {
            return Some(d.number);
        }
        self.player
            .as_ref()
            .filter(|p| self.numbers.get(&p.number) == Some(&id))
            .map(|p| p.number)
    }

    /// Every drawn object's parts taken out, to be placed again at the next sync with the meshes
    /// they take now (Detailed Faces changed).
    pub fn redress(&mut self, engine: &mut Engine) {
        for d in self.objects.values_mut().chain(self.player.as_mut()) {
            for i in d.instances.drain(..).flatten() {
                engine.scene.remove_instance(i);
            }
            d.parts.clear();
        }
    }

    /// Forces the world to be loaded again at the next sync (a change of reach, another pack).
    pub fn reload(&mut self) {
        self.stream = None;
    }

    /// Takes the block the client's render space is round this frame (`fallback` before its
    /// window has one) and the eye's place in it (`eye`, metres from its south-west corner), and
    /// streams the land round it: after this, [`Self::offset`] is the frame's.
    pub fn prepare(
        &mut self,
        engine: &mut Engine,
        loader: &mut AssetLoader,
        ws: &WorldState,
        fallback: (i32, i32),
        eye: glam::Vec2,
    ) {
        let started = std::time::Instant::now();
        let v = ws.viewer_block().unwrap_or(fallback);
        self.viewer = (v.0 + self.nudge.0, v.1 + self.nudge.1);
        // The statics' lamps are the stream's; the moving objects' are added again by the sync.
        engine.lamps.truncate(self.static_lamps);
        let (store, region, reach) = (&self.store, &self.region, self.reach);
        let smoothing = self.smoothing;
        let stream = self.stream.get_or_insert_with(|| {
            let mut s = Stream::new(
                Arc::clone(store),
                Arc::clone(region),
                reach.radius,
                reach.far,
                loader,
            );
            s.set_smoothing(smoothing);
            // `PYREAL_POOL_BUDGET=<share>`: a smaller share of the pools before eviction, for a
            // journey measured offline.
            if let Some(share) = std::env::var("PYREAL_POOL_BUDGET")
                .ok()
                .and_then(|v| v.parse::<f32>().ok())
            {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let bytes = (f64::from(share) * f64::from(3u32 << 30)) as u64;
                s.budget = pyreal_engine::evict::Budget::share(share, bytes);
            }
            s
        });
        stream.staged = self.staged;
        let was_jumping = stream.jumping();
        let changed = if self.settle {
            stream.finish(engine, loader, self.viewer, eye)
        } else {
            stream.update(engine, loader, self.viewer, eye)
        };
        if changed.loaded {
            // The engine's world was cleared: every object's instances with it, and the pack
            // bodies' meshes are let go.
            for mut d in self
                .objects
                .drain()
                .map(|(_, d)| d)
                .chain(self.player.take())
            {
                if let Some(mut b) = d.skinned.take() {
                    b.instance = None;
                    b.remove(engine);
                }
            }
            self.numbers.clear();
            self.counts.reloads += 1;
            self.counts.reload_ms = stream.stats.full_load_ms;
        }
        if was_jumping && !stream.jumping() {
            tracing::info!(
                "the engine placed {} blocks round {:#06X} behind portal space in {:.0} ms",
                stream.blocks().count(),
                block_id(self.viewer),
                stream.stats.full_load_ms
            );
        }
        if changed.loaded && !stream.jumping() {
            tracing::info!(
                "the engine loaded {} blocks round {:#06X} in {:.0} ms",
                stream.blocks().count(),
                block_id(self.viewer),
                self.counts.reload_ms
            );
        }
        if changed.loaded && !self.heavens {
            let h =
                pyreal_engine::heavens::Heavens::load(engine, loader, &self.store, &self.region);
            engine.heavens = Some(h);
            self.heavens = true;
        }
        self.static_lamps = engine.lamps.len();
        self.counts.prepare_ms = started.elapsed().as_secs_f64() * 1e3;
    }

    /// Whether a jump is being placed behind portal space.
    #[must_use]
    pub fn jumping(&self) -> bool {
        self.stream.as_ref().is_some_and(Stream::jumping)
    }

    /// What the land's streaming has done.
    #[must_use]
    pub fn stream_stats(&self) -> pyreal_engine::stream::StreamStats {
        self.stream.as_ref().map(|s| s.stats).unwrap_or_default()
    }

    /// Makes the engine's scene say what `ws` says, for an eye at `eye` (engine space); after
    /// [`Self::prepare`].
    pub fn sync(
        &mut self,
        engine: &mut Engine,
        loader: &mut AssetLoader,
        ws: &WorldState,
        eye: Vec3,
    ) {
        let offset = self.offset();
        // Lamps of objects that move are placed again each frame.
        engine
            .lamps
            .truncate(self.static_lamps.min(engine.lamps.len()));
        engine.bodies.clear();
        engine.umbrae.clear();
        engine.clearings.clear();
        let now = std::time::Instant::now();
        #[allow(clippy::cast_possible_truncation)]
        let dt = self.last_sync.map_or(0.0, |t| {
            now.duration_since(t).as_secs_f64().min(0.25) as f32
        });
        self.last_sync = Some(now);
        // Who holds a launcher now.
        for o in ws.objects.values().filter(|o| o.launcher) {
            if let Some((holder, _)) = o.sim.parent {
                self.held_launchers.insert(holder);
            }
        }
        // Server objects.
        let mut seen: Vec<ObjectId> = Vec::with_capacity(ws.objects.len());
        for (&id, o) in &ws.objects {
            if !o.drawn {
                continue;
            }
            seen.push(id);
            let cell = ws
                .object_draw_cell(o)
                .map_or(OUTSIDE, |c| self.cell_slot(engine, c));
            let lift = lift(engine, cell, frame_point(&o.frame) + offset);
            let at = frame_point(&o.frame) + offset + lift;
            let offset = offset + lift;
            // The particle half of the object's tick (a portal's swirl), which the runtime's step
            // leaves to the presentation.
            {
                let mut d = o.sim.driver.borrow_mut();
                engine.particles.step_object(&mut d, at, eye);
            }
            let driver = o.sim.driver.borrow();
            let number = match self.objects.get(&id) {
                Some(d) => d.number,
                None => {
                    let n = self.new_number(id);
                    self.objects.insert(id, Drawn::new(n));
                    n
                }
            };
            let mut drawn = self.objects.remove(&id).expect("inserted above");
            let smoke = pyreal_engine::body::shadow_parts(engine, loader, &self.store, &driver);
            let kept = loader.keeps(o.wcid, self.held_launchers.contains(&id));
            let setup = self.setups.get(&id).copied();
            let skip = Self::take_skin(
                engine,
                loader,
                &self.store,
                &mut drawn,
                setup.filter(|_| !kept),
                &driver,
            );
            // A thing the pack has a model for (no body, not a creature, nor held) is drawn as
            // that model.
            let whole = setup
                .filter(|_| {
                    drawn.skinned.is_none()
                        && !o.is_creature
                        && !o.is_player
                        && o.sim.parent.is_none()
                })
                .and_then(|s| Some((s, loader.pack_object(engine, &self.store, s)?)));
            let frame = Affine3A::from_translation(offset)
                * pyreal_engine::asset::object::frame_affine(&o.frame)
                * Affine3A::from_scale(Vec3::splat(driver.scale));
            let word = pyreal_engine::scene::faded(cell, 0.0);
            Self::pose_whole(engine, &mut drawn, whole, frame, word);
            drawn.later = setup.filter(|&s| pyreal_engine::later_models::listed(s) == Some(false));
            if whole.is_none_or(|(s, _)| pyreal_engine::pack::lays_over(s)) {
                let parts = &driver.part_array;
                self.pose(
                    engine,
                    loader,
                    &mut drawn,
                    parts,
                    smoke.as_deref(),
                    &skip,
                    offset,
                    cell,
                    eye,
                );
            } else {
                for i in drawn.instances.drain(..).flatten() {
                    engine.scene.remove_instance(i);
                }
                drawn.parts.clear();
            }
            Self::draw_skin(
                engine, loader, &mut drawn, &driver, &o.frame, offset, cell, dt,
            );
            let held = o.sim.parent.is_some();
            self.gather(
                engine,
                loader,
                &driver,
                cell,
                offset,
                eye,
                smoke.is_some(),
                held,
            );
            if at.distance(eye) < 40.0 {
                engine.bodies.push(at);
            }
            if smoke.is_some() {
                engine.umbrae.push(at);
            }
            let foot = frame_point(&o.frame) + offset - lift;
            if let Some(r) = self.clearing(engine, id, o, cell, foot, driver.scale) {
                engine.clearings.push(at.truncate().extend(r));
            }
            // Its setup's lamps, less those its emitters' own lamps stand for (its fire's, its glow's).
            if let Some(&setup) = self.setups.get(&id) {
                let t = Affine3A::from_translation(offset)
                    * pyreal_engine::asset::object::frame_affine(&o.frame);
                let lamps = loader.lamps_at(&self.store, setup, &t, cell, number, o.is_creature);
                engine
                    .lamps
                    .extend(engine.particles.setup_lamps(&driver, lamps));
            }
            drop(driver);
            self.objects.insert(id, drawn);
        }
        // Objects gone from the world (`seen` is in the world map's order, sorted).
        let gone: Vec<ObjectId> = self
            .objects
            .keys()
            .filter(|id| seen.binary_search(id).is_err())
            .copied()
            .collect();
        for id in gone {
            if let Some(d) = self.objects.remove(&id) {
                self.numbers.remove(&d.number);
                self.held_launchers.remove(&id);
                d.remove(engine);
            }
        }
        // The player.
        match ws.character.as_ref() {
            Some(c) => {
                let id = c.object_id();
                let number = match self.player.as_ref() {
                    Some(p) if self.numbers.get(&p.number) == Some(&id) => p.number,
                    _ => {
                        if let Some(old) = self.player.take() {
                            self.numbers.remove(&old.number);
                            old.remove(engine);
                        }
                        let n = self.new_number(id);
                        self.player = Some(Drawn::new(n));
                        n
                    }
                };
                let _ = number;
                let cell = self.cell_slot(engine, c.position().cell);
                let offset = offset + lift(engine, cell, frame_point(&c.render_frame()) + offset);
                // The body's particles (a level-up's sparks) on the clock its step stamped.
                {
                    let at = frame_point(&c.render_frame()) + offset;
                    let mut d = c.driver_mut();
                    engine.particles.step_object(&mut d, at, eye);
                }
                let driver = c.driver();
                let mut drawn = self.player.take().expect("made above");
                let kept = loader.keeps(0, self.held_launchers.contains(&id));
                let setup = (!kept).then(|| c.setup_id());
                let skip = Self::take_skin(engine, loader, &self.store, &mut drawn, setup, &driver);
                self.pose(
                    engine,
                    loader,
                    &mut drawn,
                    &driver.part_array,
                    None,
                    &skip,
                    offset,
                    cell,
                    eye,
                );
                let frame = c.render_frame();
                Self::draw_skin(
                    engine, loader, &mut drawn, &driver, &frame, offset, cell, dt,
                );
                self.gather(engine, loader, &driver, cell, offset, eye, false, false);
                drop(driver);
                self.player = Some(drawn);
                engine.bodies.push(frame_point(&c.render_frame()) + offset);
            }
            None => {
                if let Some(old) = self.player.take() {
                    self.numbers.remove(&old.number);
                    old.remove(engine);
                }
            }
        }
        // The nearest shadows pool their darkness.
        engine
            .umbrae
            .sort_by(|a, b| a.distance(eye).total_cmp(&b.distance(eye)));
        self.counts.objects = self.objects.len() + usize::from(self.player.is_some());
        self.counts.packed = self.objects.values().filter(|d| d.whole.is_some()).count();
        self.counts.instances = self
            .objects
            .values()
            .chain(self.player.iter())
            .map(|d| d.all_instances().count())
            .sum();
    }

    /// The ground server object `o` clears of cover (its radius) when it is a solid thing standing
    /// on the land outdoors, its foot at `foot`: not a creature, the player or held, nor too small
    /// or too large ([`pyreal_engine::cover::clearing_radius`]).
    fn clearing(
        &mut self,
        engine: &Engine,
        id: ObjectId,
        o: &pyreal_client_runtime::world_state::WorldObject,
        cell: u32,
        foot: Vec3,
        scale: f32,
    ) -> Option<f32> {
        use pyreal_engine::cover::{CLEARING_GROUND, CLEARING_LEAST, CLEARING_MOST};
        let s = o.sim.state;
        if cell != OUTSIDE
            || o.is_player
            || o.is_creature
            || o.sim.parent.is_some()
            || s.is_ethereal()
            || s.is_missile()
            || s.is_hidden()
            || s.is_nodraw()
        {
            return None;
        }
        let ground = engine.land.as_ref()?.height_at(foot.truncate())?;
        if (foot.z - ground).abs() > CLEARING_GROUND {
            return None;
        }
        let setup = *self.setups.get(&id)?;
        let store = &self.store;
        let r = (*self
            .grounds
            .entry(setup.0)
            .or_insert_with(|| pyreal_engine::world::ground_radius(store, setup)))?
            * scale;
        (CLEARING_LEAST..=CLEARING_MOST).contains(&r).then_some(r)
    }

    fn new_number(&mut self, id: ObjectId) -> u32 {
        let n = self.next_number;
        self.next_number = self.next_number.wrapping_add(1).max(SERVER_NUMBERS);
        self.numbers.insert(n, id);
        n
    }

    /// The engine's cell slot for client cell `cell`: its room's when it is a room the engine has,
    /// the outdoors' otherwise.
    fn cell_slot(&self, engine: &Engine, cell: CellId) -> u32 {
        if pyreal_physics::landdefs::is_outdoors(cell) {
            return OUTSIDE;
        }
        engine.cells.slot_of(cell.0).unwrap_or(OUTSIDE)
    }

    /// Makes or lets go of `drawn`'s pack body for `setup` (none for an object the pack keeps)
    /// and `driver`'s look; each part it is drawn in place of.
    fn take_skin(
        engine: &mut Engine,
        loader: &mut AssetLoader,
        store: &RetailDatStore,
        drawn: &mut Drawn,
        setup: Option<DataId>,
        driver: &pyreal_animation::MotionDriver,
    ) -> Vec<bool> {
        let want = setup.and_then(|s| {
            let palettes = pyreal_engine::skin::palettes_of(driver);
            loader.skin_for(Some(store), s, &palettes).map(|(k, _)| k)
        });
        if drawn.skinned.as_ref().map(|b| b.skin) != want {
            if let Some(old) = drawn.skinned.take() {
                old.remove(engine);
            }
            drawn.skinned =
                want.and_then(|k| pyreal_engine::skin::SkinnedBody::new(engine, loader, k));
        }
        let named = drawn
            .skinned
            .as_ref()
            .and_then(|b| loader.pack()?.pack.skins.get(b.skin))
            .map(|k| k.model.rig.parts())
            .unwrap_or_default();
        (0..driver.part_array.parts.len())
            .map(|k| u16::try_from(k).is_ok_and(|k| named.contains(&k)))
            .collect()
    }

    /// Poses `drawn`'s pack body, if it has one, where `frame` (render space) and `offset` put
    /// the object, `dt` seconds on.
    #[allow(clippy::too_many_arguments)]
    fn draw_skin(
        engine: &mut Engine,
        loader: &AssetLoader,
        drawn: &mut Drawn,
        driver: &pyreal_animation::MotionDriver,
        frame: &Frame,
        offset: Vec3,
        cell: u32,
        dt: f32,
    ) {
        let Some(body) = drawn.skinned.as_mut() else {
            return;
        };
        let Some(skin) = loader.pack().and_then(|p| p.pack.skins.get(body.skin)) else {
            return;
        };
        let lift = Affine3A::from_translation(offset);
        let parts: Vec<Affine3A> = pyreal_engine::skin::part_frames(driver)
            .into_iter()
            .map(|p| lift * p)
            .collect();
        let scale = Affine3A::from_scale(Vec3::splat(driver.scale.max(1e-4)));
        let root = lift * pyreal_engine::asset::object::frame_affine(frame) * scale;
        body.update(engine, skin, root, &parts, driver, dt, drawn.number, cell);
    }

    /// The pack's model `whole` (the setup it stands for, its mesh) as `drawn`'s one instance at
    /// `frame`, made again when the model changes; taken away with none.
    fn pose_whole(
        engine: &mut Engine,
        drawn: &mut Drawn,
        whole: Option<(DataId, pyreal_engine::scene::MeshId)>,
        frame: Affine3A,
        word: u32,
    ) {
        match (drawn.whole, whole) {
            (Some((was, i)), Some((s, _))) if was == s => {
                engine.scene.set_transform(i, frame);
                engine.scene.set_cell(i, word);
            }
            (old, new) => {
                if let Some((_, i)) = old {
                    engine.scene.remove_instance(i);
                }
                drawn.whole = new.and_then(|(s, mesh)| {
                    let i = engine
                        .scene
                        .add_dynamic(Instance::at(mesh, frame, drawn.number, word))
                        .ok()?;
                    Some((s, i))
                });
            }
        }
    }

    /// The parts of `parts` as dynamic instances of `drawn`: rebuilt where a part's dress
    /// changed, moved where it did not; those `smoke` marks drawn as a shadow's smoke, and those
    /// `skip` marks (a pack body's) not at all. A part the pack has a model for is drawn as that
    /// model.
    #[allow(clippy::too_many_arguments)]
    fn pose(
        &mut self,
        engine: &mut Engine,
        loader: &mut AssetLoader,
        drawn: &mut Drawn,
        parts: &pyreal_animation::parts::PartArray,
        smoke: Option<&[bool]>,
        skip: &[bool],
        offset: Vec3,
        cell: u32,
        eye: Vec3,
    ) {
        let n = parts.parts.len();
        if drawn.parts.len() != n {
            for i in drawn.instances.drain(..).flatten() {
                engine.scene.remove_instance(i);
            }
            drawn.parts = vec![None; n];
            drawn.instances = vec![None; n];
        }
        for (k, p) in parts.parts.iter().enumerate() {
            let hidden =
                pyreal_engine::body::part_hidden(p) || skip.get(k).copied().unwrap_or(false);
            let mut at = part_affine(&p.pos, p.gfxobj_scale);
            at.translation += glam::Vec3A::from(offset);
            // A player's head: its detail by how near it stands (`AssetLoader::head_mesh`).
            // A capped or hooded head keeps the mesh it is worn as, its face given detail near.
            let was_near = matches!(drawn.parts[k], Some((_, _, Some(HeadLevel::Near))));
            let face_at = pyreal_engine::face_detail::is_player_head(&self.store, p.gfxobj_id)
                .then(|| (Vec3::from(at.translation) - eye).length());
            let mut head = None;
            let want = (!hidden).then(|| {
                let mut dress = Dress::of(p.surface_overrides.as_ref());
                dress.smoke = smoke.and_then(|s| s.get(k)).copied().unwrap_or(false);
                if let (Some(d), true) = (
                    face_at,
                    pyreal_engine::face_detail::is_bare_head(&self.store, p.gfxobj_id),
                ) {
                    let own = parts.setup.as_ref().and_then(|s| s.parts.get(k)).copied();
                    head = loader.head_mesh(
                        engine,
                        &self.store,
                        p.gfxobj_id,
                        own,
                        &dress,
                        d,
                        was_near,
                    );
                }
                (p.gfxobj_id, dress, head.map(|h| h.1))
            });
            let word = pyreal_engine::body::part_cell(p, cell);
            if want != drawn.parts[k] {
                if let Some(i) = drawn.instances[k].take() {
                    engine.scene.remove_instance(i);
                }
                if let Some((gfx, dress, _)) = &want {
                    let mesh = head
                        .map(|h| h.0)
                        .or_else(|| loader.pack_object(engine, &self.store, *gfx))
                        .or_else(|| {
                            let setup = drawn.later?;
                            loader.later_part(engine, &self.store, setup, *gfx)
                        })
                        .or_else(|| {
                            let g = pyreal_engine::asset::object::near_mesh(&self.store, *gfx)?;
                            let own = parts.setup.as_ref().and_then(|s| s.parts.get(k)).copied();
                            let m = loader.mesh_worn(engine, &self.store, g, own, dress)?;
                            loader.link_degrades_dressed(engine, &self.store, m, g, false, dress);
                            Some(m)
                        });
                    drawn.instances[k] = mesh.and_then(|m| {
                        engine
                            .scene
                            .add_dynamic(Instance::at(m, at, drawn.number, word))
                            .ok()
                    });
                }
                drawn.parts[k] = want;
            } else if let Some(i) = drawn.instances[k] {
                engine.scene.set_transform(i, at);
                engine.scene.set_cell(i, word);
            }
            if let (Some(d), None, Some(i)) = (face_at, head, drawn.instances[k]) {
                if d < pyreal_engine::world::FACE_REACH {
                    if let Some(m) = engine.scene.instance(i).map(|x| x.mesh) {
                        loader.want_faces(engine, &self.store, m);
                    }
                }
            }
        }
    }

    /// `driver`'s live particles into the next frame's, their pictures made ready first; a
    /// `shadow`'s eyes glow, and a `held` thing's light lights as a held thing's.
    #[allow(clippy::too_many_arguments)]
    fn gather(
        &mut self,
        engine: &mut Engine,
        loader: &mut AssetLoader,
        driver: &pyreal_animation::MotionDriver,
        cell: u32,
        offset: Vec3,
        eye: Vec3,
        shadow: bool,
        held: bool,
    ) {
        if driver.particles.is_empty() || !engine.particles.on {
            return;
        }
        for g in engine.particles.missing_sprites(driver) {
            pyreal_engine::particles::prepare_sprite(engine, loader, &self.store, g);
        }
        engine
            .particles
            .gather(driver, cell, offset, eye, shadow, held);
    }
}

/// How far a body at engine-space `at`, seen through cell slot `cell`, is drawn above where
/// physics puts it: onto Landscape Smoothing's drawn ground outdoors, nought elsewhere.
fn lift(engine: &Engine, cell: u32, at: Vec3) -> Vec3 {
    if cell != OUTSIDE {
        return Vec3::ZERO;
    }
    Vec3::Z * engine.land.as_ref().map_or(0.0, |l| l.lift(at))
}

/// How far the camera at engine-space `eye` is drawn from where it stands, `feet` (engine space,
/// where physics puts them) the player's it follows outdoors: it rises with the player's body,
/// or by the ground under it, and keeps a little room over the drawn ground.
#[must_use]
pub fn camera_lift(land: &pyreal_engine::land::LandField, eye: Vec3, feet: Option<Vec3>) -> Vec3 {
    if land.drawn.is_empty() {
        return Vec3::ZERO;
    }
    let p = eye.truncate();
    let rise = feet.map_or_else(|| land.drawn_offset(p), |f| land.lift(f));
    let mut z = eye.z + rise;
    if let Some(g) = land.drawn_height(p) {
        z = z.max(g + CAMERA_CLEARANCE);
    }
    Vec3::Z * (z - eye.z)
}

/// The least room the camera keeps over Landscape Smoothing's drawn ground and over the water's
/// waves, metres.
const CAMERA_CLEARANCE: f32 = 0.3;

/// How far the camera at engine-space `eye` is lifted to keep its room over the water's drawn
/// surface there, `surface` (none where no water is drawn). Presentation only: the camera's sweep
/// and the body's place stay retail's.
#[must_use]
pub fn water_lift(eye: Vec3, surface: Option<f32>) -> Vec3 {
    surface.map_or(Vec3::ZERO, |s| {
        Vec3::Z * (s + CAMERA_CLEARANCE - eye.z).max(0.0)
    })
}

/// A frame's origin as a point.
#[must_use]
pub fn frame_point(f: &Frame) -> Vec3 {
    Vec3::new(f.origin.x, f.origin.y, f.origin.z)
}

/// The engine's camera for a client camera `frame` (client render space) moved by `offset`, with
/// vertical field of view `fov_y`, standing in client cell `cell` when it is a room.
#[must_use]
pub fn camera(frame: &Frame, offset: Vec3, fov_y: f32, far: f32, cell: Option<CellId>) -> Camera {
    let q = glam::Quat::from_xyzw(
        frame.rotation.x,
        frame.rotation.y,
        frame.rotation.z,
        frame.rotation.w,
    );
    let q = if q.length_squared() > 0.0 {
        q.normalize()
    } else {
        glam::Quat::IDENTITY
    };
    let f = q * Vec3::Y;
    Camera {
        position: frame_point(frame) + offset,
        yaw: pyreal_primitives::num::math::atan2f(-f.x, f.y),
        pitch: pyreal_primitives::num::math::asinf(f.z.clamp(-1.0, 1.0)),
        fov_y,
        near: 0.05,
        far,
        cell: cell.map(|c| c.0),
    }
}

/// What falls from the sky for the world's weather: on the day's own rain (a rainy day of the
/// calendar) rain over most land and snow over snowy land, drop by drop; rain or a storm asked
/// for with `@weather`, rain everywhere; snow asked for, snow everywhere. With it, whether it
/// storms.
#[must_use]
pub fn fall(ws: &WorldState) -> (pyreal_engine::weather::Fall, bool) {
    use pyreal_client_runtime::weather::{days_own, storms, Weather};
    use pyreal_engine::weather::Fall;
    let form = match ws.weather {
        Weather::Auto if matches!(days_own(ws), "rain" | "storm") => Fall::ByLand,
        Weather::Auto | Weather::Clear => Fall::Nothing,
        Weather::Rain | Weather::Storm => Fall::Rain,
        Weather::Snow => Fall::Snow,
    };
    if form == Fall::Nothing {
        return (form, false);
    }
    (form, storms(ws))
}

/// The sky's light for the world's clock: the day's group (or the weather asked for) and the
/// time of day (noon while it is always day).
#[must_use]
pub fn light(region: &pyreal_engine::world::Region, ws: &WorldState) -> SkyLight {
    let time = if ws.always_daylight {
        0.5
    } else {
        ws.clock.present_time_of_day
    };
    let asked = match ws.weather {
        pyreal_client_runtime::weather::Weather::Auto => None,
        pyreal_client_runtime::weather::Weather::Clear => {
            Some(pyreal_world_render::sky::DayKind::Clear)
        }
        _ => Some(pyreal_world_render::sky::DayKind::Rainy),
    };
    let group = region
        .sky_info
        .as_ref()
        .and_then(|sky| {
            let present = pyreal_world_render::sky::drawn_day_group(
                region,
                ws.clock.current_year,
                ws.clock.current_day,
                asked,
            )?;
            sky.day_groups.iter().position(|g| std::ptr::eq(g, present))
        })
        .unwrap_or(0);
    SkyLight::at(region, group, time)
}

/// The volumetric clouds' knobs with what `@clouds` has asked for in place of their own.
#[must_use]
pub fn cloud_knobs(
    asked: &pyreal_client_runtime::clouds::Clouds,
) -> pyreal_engine::clouds::CloudKnobs {
    use pyreal_client_runtime::clouds::{CloudOption as O, OPTIONS};
    let mut k = pyreal_engine::clouds::CloudKnobs::default();
    for option in OPTIONS {
        let Some(v) = asked.get(option) else {
            continue;
        };
        let knob = match option {
            O::Cover => &mut k.cover,
            O::Speed => &mut k.speed,
            O::Height => &mut k.height,
            O::Depth => &mut k.depth,
            O::Size => &mut k.size,
            O::Detail => &mut k.detail,
            O::Light => &mut k.light,
            O::Shade => &mut k.shade,
            O::Shadows => &mut k.shadows,
        };
        *knob = v;
    }
    k
}

/// How much of each moon's face is drawn lit `days` into the calendar, by its graphics object:
/// what `@moon` forces, or the calendar's.
#[must_use]
pub fn moon_phases(asked: &pyreal_client_runtime::moon::Moons, days: f64) -> [(u32, f32); 2] {
    pyreal_client_runtime::moon::MOONS.map(|m| (m.gfx(), asked.phase(m, days)))
}

#[cfg(test)]
mod moon_tests {
    use pyreal_client_runtime::moon::{calendar_days, Moon, MoonCommand as C, Moons, MOONS};

    #[test]
    fn the_moons_are_drawn_at_the_phase_asked_for_or_the_calendars() {
        let days = calendar_days(12, 200, 0.25);
        let mut asked = Moons::default();
        let calendar = super::moon_phases(&asked, days);
        for (m, (gfx, lit)) in MOONS.iter().zip(calendar) {
            assert_eq!((gfx, lit), (m.gfx(), m.calendar_phase(days)));
        }
        assert_ne!(
            calendar[0].1, calendar[1].1,
            "each moon keeps its own month"
        );
        asked.obey(C::Set(Some(Moon::Albarel), 0.0));
        let forced = super::moon_phases(&asked, days);
        assert_eq!(forced[0], calendar[0], "Rez'arel keeps the calendar's");
        assert_eq!(forced[1], (Moon::Albarel.gfx(), 0.0));
        asked.obey(C::Unset(Moon::Albarel));
        assert_eq!(super::moon_phases(&asked, days), calendar);
    }
}

#[cfg(test)]
mod cloud_tests {
    use pyreal_client_runtime::clouds::{CloudOption as O, Clouds, CloudsCommand as C};

    #[test]
    fn the_clouds_asked_for_replace_the_knobs_and_reset_returns_to_them() {
        let own = pyreal_engine::clouds::CloudKnobs::default();
        let mut asked = Clouds::default();
        assert_eq!(super::cloud_knobs(&asked), own, "nothing asked for");
        asked.obey(C::Set(O::Cover, 0.0));
        asked.obey(C::Set(O::Size, 3.0));
        asked.obey(C::Set(O::Height, 2500.0));
        let k = super::cloud_knobs(&asked);
        assert_eq!((k.cover, k.size, k.height), (0.0, 3.0, 2500.0));
        assert_eq!(k.light, own.light, "what is not asked for stays the knob's");
        asked.obey(C::Reset);
        assert_eq!(super::cloud_knobs(&asked), own);
    }
}
