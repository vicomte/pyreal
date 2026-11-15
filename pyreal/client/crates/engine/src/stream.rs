//! The land streamed block by block as the eye moves, with no reload.
//!
//! The exact land is a square of blocks round a centre block: each block's rooms, ground,
//! statics, buildings and scenery, placed by [`crate::world`]'s per-block steps; past it the far
//! terrain and, out to [`crate::world::far_tree_rings`] more, the art pack's trees as cards,
//! thinned past [`crate::world::FAR_TREE_BLOCKS`].
//!
//! - **Planning** (decoding the landblock, its information and rooms, generating its scenery, the
//!   far terrain's field and the land field) runs on worker threads, with no device.
//! - **Placing** (meshes, pictures and instances into the pools) runs on the frame's thread, a
//!   step at a time (rooms, ground, statics, scenery in runs) within a budget of milliseconds a
//!   frame, nearest block first.
//! - When the eye goes past the centre block (and a cell beyond, so walking along a block line
//!   does not swing it back and forth), the square round the eye's block is loaded: its new blocks
//!   are placed while the old square is still drawn (their ground waits), and once they and the
//!   new far terrain and land field are ready, the switch is made at once: the ground of the new
//!   square drawn, the blocks left behind taken out, the far terrain and the land field swapped.
//!   Then, past the [`crate::evict::Budget`], what nothing draws any more is freed.
//! - **Render space** is metres from an origin block's south-west corner. It stays put while the
//!   eye travels, until the square's centre is [`REBASE`] blocks from it; then the origin moves to
//!   the centre and everything placed moves with it (instances, rooms and their cut volumes,
//!   lamps, particles, the far terrain and the land field), with nothing loaded again, so no
//!   position is ever more than a few kilometres from the origin and f32 keeps millimetres.
//! - A jump past the square (a teleport, a portal, a dungeon) loads the square round the new
//!   place at once, its blocks planned on every worker; the far trees follow over the next frames.
//!   With [`Stream::staged`] set (portal space covers the screen), the jump is placed a step at a
//!   time instead, nearest block first, and drawn once the whole square is in.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use glam::{Affine3A, Vec2, Vec3};
use pyreal_dat::RetailDatStore;
use pyreal_primitives::DataId;

use crate::evict::{Budget, PoolUse};
use crate::far::FarField;
use crate::land::LandField;
use crate::world::{self, Area, AssetLoader, BlockPlan, PlacedGround, Planner, Prefetch, Region};
use crate::Engine;

/// Blocks the square's centre may be from render space's origin before the origin moves to it.
pub const REBASE: i32 = 8;
/// How far past the centre block's edge the eye goes before the square follows it, metres.
pub const MARGIN: f32 = 24.0;
/// How much nearer than the square's edge the trees reach their cards, metres: the eye stands a
/// little off the body the square follows.
const CARD_SLACK: f32 = 16.0;
/// Scenery objects placed in one step.
const SCENERY_RUN: usize = 48;
/// The least time between evictions asked for by bodies dressed in place.
pub const DRESSED_EVICT_EVERY: std::time::Duration = std::time::Duration::from_secs(2);
/// Worker threads planning blocks.
const WORKERS: usize = 3;

type Block = (i32, i32);
/// A block's scenery: each object and where it stands in the block.
type Scenery = Vec<(DataId, Affine3A)>;
/// The planners' queues (the square's, the far trees') and their wake-up.
type Queues = (Mutex<(VecDeque<Job>, VecDeque<Job>)>, Condvar);

/// What the streaming has done, for the report and the tests.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StreamStats {
    /// Squares loaded at once (the first, a teleport's).
    pub full_loads: u64,
    /// Switches to a new square streamed in.
    pub switches: u64,
    pub rebases: u64,
    pub blocks_placed: u64,
    pub blocks_removed: u64,
    pub tree_blocks: usize,
    /// The last full load and the last switch, ms.
    pub full_load_ms: f64,
    pub switch_ms: f64,
    /// This frame's placing, ms, and the longest single step ever.
    pub frame_ms: f64,
    pub worst_step_ms: f64,
    /// What the longest step was, and its block.
    pub worst_step: (&'static str, (i32, i32)),
    /// What the last step was.
    step: (&'static str, (i32, i32)),
    pub evictions: u64,
    pub evicted: crate::evict::Evicted,
    /// Blocks waiting for their plans or to be placed.
    pub pending: usize,
    /// Far blocks whose trees are asked for and planned, not yet placed.
    pub trees_asked: usize,
    pub trees_planned: usize,
}

enum Job {
    /// A block of the square, its pictures decoded ahead (not for a square loaded at once, which
    /// waits for its plans).
    Block(u64, Block, bool),
    /// A far block's trees, their feet on the far terrain's levels about it.
    Trees(u64, Block, std::ops::RangeInclusive<u32>),
    Far(u64, Area, Block, i32),
    Land(u64, Vec<crate::land::BlockLand>, Block, usize, Vec2, Block),
}

enum Done {
    Block(u64, Box<BlockPlan>),
    Trees(u64, Block, Vec<(DataId, Affine3A)>),
    Far(u64, Block, Box<FarField>),
    Land(u64, Block, Box<LandField>),
}

/// The planning threads: two queues (the square's blocks and fields first, the far trees after),
/// answers by channel.
struct Workers {
    queues: Arc<Queues>,
    stop: Arc<AtomicBool>,
    /// Landscape Smoothing's strength (0 off): the planners work out each block's drawn offset.
    smooth: Arc<AtomicU8>,
    done: mpsc::Receiver<Done>,
    threads: Vec<std::thread::JoinHandle<()>>,
}

impl Workers {
    fn new(store: &Arc<RetailDatStore>, region: &Arc<Region>, prefetch: &Arc<Prefetch>) -> Self {
        let queues = Arc::new((
            Mutex::new((VecDeque::new(), VecDeque::new())),
            Condvar::new(),
        ));
        let stop = Arc::new(AtomicBool::new(false));
        let smooth = Arc::new(AtomicU8::new(0));
        let (tx, done) = mpsc::channel();
        let threads = (0..WORKERS)
            .filter_map(|k| {
                let (queues, stop, tx) = (Arc::clone(&queues), Arc::clone(&stop), tx.clone());
                let (store, region) = (Arc::clone(store), Arc::clone(region));
                let (prefetch, smooth) = (Arc::clone(prefetch), Arc::clone(&smooth));
                std::thread::Builder::new()
                    .name(format!("land planner {k}"))
                    .spawn(move || work(&queues, &stop, &smooth, &tx, &store, &region, &prefetch))
                    .ok()
            })
            .collect();
        Self {
            queues,
            stop,
            smooth,
            done,
            threads,
        }
    }

    fn ask(&self, job: Job, urgent: bool) {
        let (lock, wake) = &*self.queues;
        if let Ok(mut q) = lock.lock() {
            if urgent {
                q.0.push_back(job);
            } else {
                q.1.push_back(job);
            }
            wake.notify_one();
        }
    }

    /// Drops every job not yet started.
    fn clear(&self) {
        if let Ok(mut q) = self.queues.0.lock() {
            q.0.clear();
            q.1.clear();
        }
    }
}

impl Drop for Workers {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.clear();
        self.queues.1.notify_all();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

fn work(
    queues: &Queues,
    stop: &AtomicBool,
    smooth: &AtomicU8,
    tx: &mpsc::Sender<Done>,
    store: &RetailDatStore,
    region: &Region,
    prefetch: &Prefetch,
) {
    let mut planner = Planner::new(region);
    let mut smoothing = 0;
    // ORDER-OK: a memo, looked up only.
    let mut seen = HashSet::new();
    loop {
        let job = {
            let Ok(mut q) = queues.0.lock() else {
                return;
            };
            loop {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                if let Some(j) = q.0.pop_front().or_else(|| q.1.pop_front()) {
                    break j;
                }
                q = match queues.1.wait(q) {
                    Ok(q) => q,
                    Err(_) => return,
                };
            }
        };
        let level = smooth.load(Ordering::Relaxed);
        if level != smoothing {
            smoothing = level;
            planner = Planner::new(region).smoothing(level);
        }
        let done = match job {
            Job::Block(g, b, ahead) => {
                let plan = planner.block(store, region, b);
                // Its pictures decoded here, before it is handed over to be placed.
                if ahead {
                    prefetch.block(store, &plan, &mut seen);
                }
                Done::Block(g, Box::new(plan))
            }
            Job::Trees(g, b, levels) => {
                Done::Trees(g, b, planner.far_trees(store, region, b, levels))
            }
            Job::Far(g, area, centre, radius) => {
                let mut field = world::far_field_round(store, region, area, centre, radius);
                field.smooth = smoothing > 0;
                Done::Far(g, centre, Box::new(field))
            }
            Job::Land(g, lands, first, blocks, origin, centre) => {
                let refs: Vec<&crate::land::BlockLand> = lands.iter().collect();
                Done::Land(
                    g,
                    centre,
                    Box::new(world::land_field(&refs, first, blocks, origin)),
                )
            }
        };
        if tx.send(done).is_err() {
            return;
        }
    }
}

/// One placed block of the square: what to take out again.
#[derive(Debug, Default)]
struct Near {
    rooms: Vec<u32>,
    instances: Vec<u32>,
    /// The runs of object numbers its placements took, `lo..hi` each.
    objects: Vec<(u32, u32)>,
    ground: Option<PlacedGround>,
    /// Its ground's instance, once drawn.
    ground_instance: Option<u32>,
}

/// One far block's trees.
#[derive(Debug, Default)]
struct Trees {
    instances: Vec<u32>,
}

/// A block being placed, a step at a time.
struct Loading {
    block: Block,
    plan: Arc<BlockPlan>,
    stage: Stage,
    near: Near,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Rooms(usize),
    Ground,
    Statics(usize),
    Scenery(usize),
}

/// Rooms and statics placed in one step: each may bring meshes and pictures new to the journey.
const ROOM_RUN: usize = 16;
const STATIC_RUN: usize = 8;

/// What an update changed beyond the land itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Changed {
    /// The square was loaded at once: the engine's world was cleared first (every instance,
    /// the moving objects' too).
    pub loaded: bool,
    /// Render space's origin moved; everything placed moved with it.
    pub rebased: bool,
}

/// The streamed land: see the module header.
pub struct Stream {
    store: Arc<RetailDatStore>,
    region: Arc<Region>,
    /// Blocks of exact land each way round the centre, and of far terrain.
    pub radius: i32,
    pub far: i32,
    /// Milliseconds of placing a frame.
    pub budget_ms: f64,
    /// Whether a jump is placed within the budget over several frames rather than at once.
    pub staged: bool,
    /// When the jump being placed a step at a time began.
    jump_started: Option<Instant>,
    /// How full the pools grow before what nothing draws is freed.
    pub budget: Budget,
    /// Render space's origin block.
    origin: Block,
    /// The square drawn, round this block.
    centre: Option<Block>,
    /// The square being loaded, round this block.
    target: Option<Block>,
    near: HashMap<Block, Near>,
    trees: HashMap<Block, Trees>,
    plans: HashMap<Block, Arc<BlockPlan>>,
    tree_plans: HashMap<Block, Vec<(DataId, Affine3A)>>,
    asked: HashSet<Block>,
    asked_trees: HashSet<Block>,
    loading: Option<Loading>,
    far_field: Option<(Block, FarField)>,
    far_asked: Option<Block>,
    land: Option<(Block, LandField)>,
    land_asked: Option<Block>,
    workers: Workers,
    generation: u64,
    /// The far block being given its trees, a run at a time, and where the next run starts.
    tree_run: Option<(Block, Scenery, usize)>,
    /// What nothing draws is to be freed at the next step.
    evict_due: bool,
    /// The loader's dressed variants counted at the last check, and when it was.
    dressed_seen: (u64, Option<Instant>),
    pub stats: StreamStats,
}

impl std::fmt::Debug for Stream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stream")
            .field("origin", &self.origin)
            .field("centre", &self.centre)
            .field("target", &self.target)
            .field("near", &self.near.len())
            .field("stats", &self.stats)
            .finish_non_exhaustive()
    }
}

fn block_id((x, y): Block) -> u16 {
    u16::try_from(((x & 0xFF) << 8) | (y & 0xFF)).unwrap_or(0)
}

fn in_world((x, y): Block) -> bool {
    (0..=0xFE).contains(&x) && (0..=0xFE).contains(&y)
}

fn chebyshev(a: Block, b: Block) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

impl Stream {
    /// Streams the land of `store`'s `region`, `radius` blocks round the eye and far terrain to
    /// `far`, its pictures decoded ahead into `loader`'s prefetch.
    #[must_use]
    pub fn new(
        store: Arc<RetailDatStore>,
        region: Arc<Region>,
        radius: i32,
        far: i32,
        loader: &AssetLoader,
    ) -> Self {
        let workers = Workers::new(&store, &region, &loader.prefetch);
        Self {
            store,
            region,
            radius,
            far,
            budget_ms: 3.0,
            staged: false,
            jump_started: None,
            budget: Budget::default(),
            origin: (0, 0),
            centre: None,
            target: None,
            near: HashMap::new(),
            trees: HashMap::new(),
            plans: HashMap::new(),
            tree_plans: HashMap::new(),
            asked: HashSet::new(),
            asked_trees: HashSet::new(),
            loading: None,
            far_field: None,
            far_asked: None,
            land: None,
            land_asked: None,
            workers,
            generation: 0,
            tree_run: None,
            evict_due: false,
            dressed_seen: (0, None),
            stats: StreamStats::default(),
        }
    }

    /// Landscape Smoothing at strength `level` (0 off, 1 Low, 2 Medium, 3 High): the ground drawn
    /// at the drawn offset ([`crate::drawn`]), and the scenery moved by it. Set before the first
    /// update; a change takes the blocks planned after it.
    pub fn set_smoothing(&mut self, level: u8) {
        self.workers.smooth.store(level, Ordering::Relaxed);
    }

    /// Landscape Smoothing's strength asked for, 0 off.
    #[must_use]
    pub fn smoothing(&self) -> u8 {
        self.workers.smooth.load(Ordering::Relaxed)
    }

    /// Render space's origin block: its south-west corner is render space's `(0, 0)`.
    #[must_use]
    pub fn origin(&self) -> Block {
        self.origin
    }

    /// The block the drawn square is round.
    #[must_use]
    pub fn centre(&self) -> Option<Block> {
        self.centre
    }

    /// Whether a jump is being placed a step at a time.
    #[must_use]
    pub fn jumping(&self) -> bool {
        self.jump_started.is_some()
    }

    /// Whether the square drawn is the one wanted, wholly placed.
    #[must_use]
    pub fn settled(&self) -> bool {
        self.centre.is_some() && self.target.is_none() && self.loading.is_none() && !self.evict_due
    }

    /// The placed blocks of the square.
    pub fn blocks(&self) -> impl Iterator<Item = Block> + '_ {
        self.near.keys().copied()
    }

    /// The far blocks with their trees placed.
    pub fn tree_blocks(&self) -> impl Iterator<Item = Block> + '_ {
        self.trees.keys().copied()
    }

    /// Render space as an [`Area`]: its centre is the origin block.
    #[must_use]
    pub fn area(&self) -> Area {
        Area {
            center: block_id(self.origin),
            radius: self.radius,
        }
    }

    /// Where block `b`'s south-west corner lies in render space.
    #[must_use]
    pub fn block_origin(&self, b: Block) -> Vec3 {
        self.area().block_origin(b.0, b.1)
    }

    /// The square round `centre`, nearest `near` first.
    fn square(&self, centre: Block, near: Block) -> Vec<Block> {
        let r = self.radius;
        let mut out: Vec<Block> = (centre.0 - r..=centre.0 + r)
            .flat_map(|x| (centre.1 - r..=centre.1 + r).map(move |y| (x, y)))
            .filter(|b| in_world(*b))
            .collect();
        out.sort_by_key(|b| {
            (
                chebyshev(*b, near),
                (b.0 - near.0).abs() + (b.1 - near.1).abs(),
            )
        });
        out
    }

    /// Whether `b` is within the square round `c`.
    fn within(&self, b: Block, c: Block) -> bool {
        chebyshev(b, c) <= self.radius
    }

    /// Forgets everything placed (the engine's world is cleared by the caller), so the next
    /// [`Self::update`] loads the square round the eye at once.
    pub fn reset(&mut self) {
        self.generation += 1;
        self.workers.clear();
        self.centre = None;
        self.jump_started = None;
        self.target = None;
        self.near.clear();
        self.trees.clear();
        self.plans.clear();
        self.tree_plans.clear();
        self.asked.clear();
        self.asked_trees.clear();
        self.loading = None;
        self.far_field = None;
        self.far_asked = None;
        self.land = None;
        self.land_asked = None;
        self.tree_run = None;
        self.evict_due = false;
    }

    /// Brings the land up to date for an eye standing in block `viewer`, `eye` metres into it
    /// from its south-west corner.
    pub fn update(
        &mut self,
        engine: &mut Engine,
        loader: &mut AssetLoader,
        viewer: Block,
        eye: Vec2,
    ) -> Changed {
        let started = Instant::now();
        let mut changed = Changed::default();
        self.receive();
        match self.centre {
            Some(c) if chebyshev(viewer, c) <= self.radius => {
                // Past the centre block by a margin, the square follows the eye's block.
                #[allow(clippy::cast_precision_loss)]
                let p = Vec2::new(
                    ((viewer.0 - c.0) * 192) as f32,
                    ((viewer.1 - c.1) * 192) as f32,
                ) + eye;
                let outside = p.cmplt(Vec2::splat(-MARGIN)).any()
                    || p.cmpgt(Vec2::splat(192.0 + MARGIN)).any();
                let want = if outside { viewer } else { c };
                if want != self.target.unwrap_or(c) {
                    changed.rebased = self.retarget(engine, loader, want, c);
                }
            }
            None if self.jumping()
                && self
                    .target
                    .is_some_and(|t| chebyshev(viewer, t) <= self.radius) =>
            {
                if !self.staged {
                    self.complete_jump(engine, loader, viewer);
                }
            }
            _ => {
                if self.staged {
                    self.begin_jump(engine, loader, viewer);
                } else {
                    self.full_load(engine, loader, viewer);
                }
                changed.loaded = true;
            }
        }
        self.ask_trees(loader);
        // Short of the square's edge every tree is its card, as the far rings draw it.
        let rings = crate::world::far_tree_rings(self.radius, self.far);
        #[allow(clippy::cast_precision_loss)]
        let edge = self.radius as f32 * 192.0 - MARGIN - CARD_SLACK;
        engine.renderer.card_edge = if rings > 0 && loader.pack().is_some() {
            edge.max(0.0)
        } else {
            0.0
        };
        self.note_dressing(loader);
        // Placing, within the budget.
        loop {
            let step = Instant::now();
            if !self.step(engine, loader, viewer) {
                break;
            }
            let ms = step.elapsed().as_secs_f64() * 1e3;
            if ms > self.stats.worst_step_ms {
                self.stats.worst_step_ms = ms;
                self.stats.worst_step = self.stats.step;
            }
            if started.elapsed().as_secs_f64() * 1e3 >= self.budget_ms {
                break;
            }
        }
        self.stats.pending = self.target.map_or(0, |t| {
            self.square(t, t)
                .iter()
                .filter(|b| !self.near.contains_key(b))
                .count()
        });
        self.stats.tree_blocks = self.trees.len();
        self.stats.trees_asked = self.asked_trees.len();
        self.stats.trees_planned = self.tree_plans.len();
        self.stats.frame_ms = started.elapsed().as_secs_f64() * 1e3;
        changed
    }

    /// Whether nothing is left to place round the eye: the square settled and the far trees in.
    #[must_use]
    pub fn idle(&self) -> bool {
        self.settled() && self.asked_trees.is_empty() && self.tree_run.is_none()
    }

    /// [`Self::update`], then waits on the workers and places everything wanted, the far trees
    /// too, with no budget: a headless capture's frames then never depend on the workers' pace.
    pub fn finish(
        &mut self,
        engine: &mut Engine,
        loader: &mut AssetLoader,
        viewer: Block,
        eye: Vec2,
    ) -> Changed {
        let budget = std::mem::replace(&mut self.budget_ms, f64::INFINITY);
        let started = Instant::now();
        let mut changed = Changed::default();
        loop {
            let c = self.update(engine, loader, viewer, eye);
            changed.loaded |= c.loaded;
            changed.rebased |= c.rebased;
            if self.idle() || started.elapsed().as_secs() >= 60 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        self.budget_ms = budget;
        changed
    }

    /// Takes in the workers' answers of this generation.
    fn receive(&mut self) {
        while let Ok(done) = self.workers.done.try_recv() {
            match done {
                Done::Block(g, plan) if g == self.generation => {
                    self.plans.insert(plan.block, Arc::from(plan));
                }
                Done::Trees(g, b, s) if g == self.generation => {
                    self.asked_trees.remove(&b);
                    self.tree_plans.insert(b, s);
                }
                Done::Far(g, c, f) if g == self.generation && self.far_asked == Some(c) => {
                    self.far_field = Some((c, *f));
                }
                Done::Land(g, c, l) if g == self.generation && self.land_asked == Some(c) => {
                    self.land = Some((c, *l));
                }
                _ => {}
            }
        }
    }

    /// The far terrain's first range for this reach.
    fn first_range(&self) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let r = (self.radius as f32 + 0.5) * 192.0 * 1.5 + 384.0;
        r
    }

    /// Starts loading the square round `want` (the drawn one is round `centre`); whether render
    /// space's origin moved first.
    fn retarget(
        &mut self,
        engine: &mut Engine,
        loader: &mut AssetLoader,
        want: Block,
        centre: Block,
    ) -> bool {
        let mut moved = false;
        if want == centre {
            // Back where it was: the square drawn is the one wanted.
            self.target = None;
        } else {
            if chebyshev(want, self.origin) > REBASE {
                self.rebase(engine, loader, want);
                moved = true;
            }
            self.target = Some(want);
            for b in self.square(want, want) {
                if !self.near.contains_key(&b)
                    && !self.plans.contains_key(&b)
                    && self.asked.insert(b)
                {
                    self.workers.ask(Job::Block(self.generation, b, true), true);
                }
            }
            if self.far > self.radius {
                self.far_field = None;
                self.far_asked = Some(want);
                self.workers.ask(
                    Job::Far(
                        self.generation,
                        Area {
                            center: block_id(self.origin),
                            radius: self.radius,
                        },
                        want,
                        self.far,
                    ),
                    true,
                );
            }
            self.land = None;
            self.land_asked = None;
        }
        // Blocks placed for a square no longer wanted are taken out (one half placed with them).
        let keep = self.target.unwrap_or(centre);
        let wanted = |s: &Self, b: Block| s.within(b, keep) || s.within(b, centre);
        if self
            .loading
            .as_ref()
            .is_some_and(|l| !wanted(self, l.block))
        {
            if let Some(l) = self.loading.take() {
                let mut near = l.near;
                near.instances.extend(loader.take_placed());
                if let Some(run) = near.objects.first_mut() {
                    run.1 = loader.object_mark();
                }
                self.unplace(engine, loader, l.block, near);
            }
        }
        let gone: Vec<Block> = self
            .near
            .keys()
            .copied()
            .filter(|b| !wanted(self, *b))
            .collect();
        for b in gone {
            if let Some(n) = self.near.remove(&b) {
                self.unplace(engine, loader, b, n);
            }
        }
        moved
    }

    /// One step of placing; whether there was one to take.
    fn step(&mut self, engine: &mut Engine, loader: &mut AssetLoader, viewer: Block) -> bool {
        if std::mem::take(&mut self.evict_due) {
            self.stats.step = ("evict", viewer);
            self.evict_if_over(engine, loader);
            return true;
        }
        if let Some(l) = self.loading.take() {
            self.advance(engine, loader, l);
            return true;
        }
        let Some(target) = self.target else {
            return self.step_trees(engine, loader);
        };
        // The next block of the wanted square, nearest the eye first.
        let next = self
            .square(target, viewer)
            .into_iter()
            .find(|b| !self.near.contains_key(b) && self.plans.contains_key(b));
        if let Some(b) = next {
            let plan = Arc::clone(&self.plans[&b]);
            self.loading = Some(Loading {
                block: b,
                plan,
                stage: Stage::Rooms(0),
                near: Near {
                    objects: vec![(loader.object_mark(), loader.object_mark())],
                    ..Near::default()
                },
            });
            return true;
        }
        let missing = self
            .square(target, target)
            .into_iter()
            .any(|b| !self.near.contains_key(&b));
        if missing {
            return self.step_trees(engine, loader);
        }
        // The whole square is placed: its land field, then the switch.
        if self.land_asked != Some(target) {
            let r = self.radius;
            let lands: Vec<crate::land::BlockLand> = self
                .square(target, target)
                .iter()
                .filter_map(|b| self.plans.get(b)?.land.clone())
                .collect();
            let first = (target.0 - r, target.1 - r);
            let blocks = usize::try_from(2 * r + 1).unwrap_or(1);
            let origin = self.block_origin(first).truncate();
            self.land_asked = Some(target);
            self.workers.ask(
                Job::Land(self.generation, lands, first, blocks, origin, target),
                true,
            );
            return true;
        }
        let far_ready = self.far <= self.radius || self.far_field.is_some();
        if far_ready && self.land.is_some() {
            self.switch(engine, loader, target);
            return true;
        }
        self.step_trees(engine, loader)
    }

    /// Advances block `l`'s placing by one stage (or one run of scenery).
    fn advance(&mut self, engine: &mut Engine, loader: &mut AssetLoader, mut l: Loading) {
        let origin = self.block_origin(l.block);
        self.stats.step = (
            match l.stage {
                Stage::Rooms(_) => "rooms",
                Stage::Ground => "ground",
                Stage::Statics(_) => "statics",
                Stage::Scenery(_) => "scenery",
            },
            l.block,
        );
        match l.stage {
            Stage::Rooms(from) => {
                let to = (from + ROOM_RUN).min(l.plan.cells.len());
                world::place_rooms_run(
                    engine,
                    loader,
                    &self.store,
                    &l.plan,
                    origin,
                    from..to,
                    &mut l.near.rooms,
                );
                if to < l.plan.cells.len() {
                    l.stage = Stage::Rooms(to);
                } else {
                    // Rooms reaching over the neighbours' ground cut it too.
                    let boxes: Vec<(Vec3, Vec3)> = l
                        .near
                        .rooms
                        .iter()
                        .filter_map(|&s| {
                            engine
                                .cells
                                .cell(s)?
                                .volume
                                .as_ref()
                                .map(|v| (v.min, v.max))
                        })
                        .collect();
                    self.recut_round(engine, loader, l.block, &boxes);
                    l.stage = Stage::Ground;
                }
            }
            Stage::Ground => {
                l.near.ground =
                    world::place_ground(engine, loader, &self.store, &self.region, &l.plan, origin);
                l.stage = Stage::Statics(0);
            }
            Stage::Statics(from) => {
                let to = (from + STATIC_RUN).min(l.plan.statics());
                world::place_statics_run(
                    engine,
                    loader,
                    &self.store,
                    &l.plan,
                    origin,
                    &l.near.rooms,
                    from..to,
                );
                l.stage = if to < l.plan.statics() {
                    Stage::Statics(to)
                } else {
                    Stage::Scenery(0)
                };
            }
            Stage::Scenery(from) => {
                let to = (from + SCENERY_RUN).min(l.plan.scenery.len());
                // A block's own scenery takes the place of its far trees.
                if from == 0 {
                    if let Some(t) = self.trees.remove(&l.block) {
                        for i in t.instances {
                            engine.scene.remove_instance(i);
                        }
                    }
                }
                world::place_scenery(engine, loader, &self.store, &l.plan, origin, from..to);
                if to < l.plan.scenery.len() {
                    l.stage = Stage::Scenery(to);
                } else {
                    l.near.instances.extend(loader.take_placed());
                    if let Some(run) = l.near.objects.first_mut() {
                        run.1 = loader.object_mark();
                    }
                    self.stats.blocks_placed += 1;
                    // Drawn at once only when the square it belongs to is the one drawn.
                    if self.target.is_none() && self.centre.is_some_and(|c| self.within(l.block, c))
                    {
                        if let Some(g) = l.near.ground.as_ref() {
                            l.near.ground_instance = engine.scene.add_instance(g.instance()).ok();
                        }
                    }
                    self.near.insert(l.block, l.near);
                    return;
                }
            }
        }
        l.near.instances.extend(loader.take_placed());
        self.loading = Some(l);
    }

    /// Cuts again the ground of the placed blocks round `block` that any of `boxes` reaches over.
    fn recut_round(
        &mut self,
        engine: &mut Engine,
        loader: &mut AssetLoader,
        block: Block,
        boxes: &[(Vec3, Vec3)],
    ) {
        if boxes.is_empty() {
            return;
        }
        for dx in -1..=1 {
            for dy in -1..=1 {
                let Some(n) = self.near.get_mut(&(block.0 + dx, block.1 + dy)) else {
                    continue;
                };
                let Some(g) = n.ground.as_mut() else {
                    continue;
                };
                let (lo, hi) = g.bounds();
                let over = boxes
                    .iter()
                    .any(|(a, b)| a.cmple(hi + 0.1).all() && b.cmpge(lo - 0.1).all());
                if over {
                    world::cut_ground(engine, loader, g);
                }
            }
        }
    }

    /// Takes block `b`'s placings out.
    fn unplace(&mut self, engine: &mut Engine, loader: &mut AssetLoader, b: Block, near: Near) {
        for i in near.instances.iter().chain(near.ground_instance.iter()) {
            engine.scene.remove_instance(*i);
        }
        if let Some(g) = near.ground.as_ref() {
            g.free(engine);
        }
        let boxes = world::remove_rooms(engine, loader, &near.rooms);
        let runs: Vec<(u32, u32)> = near
            .objects
            .iter()
            .copied()
            .filter(|(lo, hi)| hi > lo)
            .collect();
        if !runs.is_empty() {
            let mine = |o: u32| runs.iter().any(|(lo, hi)| (*lo..*hi).contains(&o));
            engine.lamps.retain(|l| !mine(l.object));
            engine.flames.placed.retain(|f| !mine(f.object));
            engine.particles.hosts.retain(|h| !mine(h.object));
        }
        self.recut_round(engine, loader, b, &boxes);
        self.stats.blocks_removed += 1;
    }

    /// The square round `target` is placed, and its far terrain and land field ready: drawn at
    /// once in place of the old one.
    fn switch(&mut self, engine: &mut Engine, loader: &mut AssetLoader, target: Block) {
        let started = Instant::now();
        self.stats.step = ("switch", target);
        let gone: Vec<Block> = self
            .near
            .keys()
            .copied()
            .filter(|b| !self.within(*b, target))
            .collect();
        for b in gone {
            if let Some(n) = self.near.remove(&b) {
                self.unplace(engine, loader, b, n);
            }
            self.plans.remove(&b);
            self.asked.remove(&b);
        }
        for n in self.near.values_mut() {
            if n.ground_instance.is_none() {
                if let Some(g) = n.ground.as_ref() {
                    n.ground_instance = engine.scene.add_instance(g.instance()).ok();
                }
            }
        }
        if let Some((_, f)) = self.far_field.take() {
            world::set_far(engine, loader, f, self.first_range());
        }
        if let Some((_, l)) = self.land.take() {
            world::set_land(engine, l);
        }
        self.centre = Some(target);
        self.target = None;
        self.far_asked = None;
        self.land_asked = None;
        if let Some(t) = self.jump_started.take() {
            if engine.renderer.cover.is_none() {
                world::load_cover(engine, loader, &self.store, &self.region);
            }
            if let Some(m) = loader.terrain_material(engine) {
                engine.renderer.terrain_material = m;
            }
            self.stats.full_loads += 1;
            self.stats.full_load_ms = t.elapsed().as_secs_f64() * 1e3;
        } else {
            self.stats.switches += 1;
        }
        loader.prefetch.drop_unused();
        // Plans of blocks a block or more past the new square are dropped (asked again if wanted).
        let r = self.radius + 1;
        self.plans.retain(|b, _| chebyshev(*b, target) <= r);
        self.asked.retain(|b| chebyshev(*b, target) <= r);
        // Freeing what the blocks left behind drew is a step of its own.
        self.evict_due = true;
        self.stats.switch_ms = started.elapsed().as_secs_f64() * 1e3;
    }

    /// Frees what nothing draws, when the pools are past the budget.
    pub fn evict_if_over(&mut self, engine: &mut Engine, loader: &mut AssetLoader) {
        if !PoolUse::of(engine).over(engine, &self.budget) {
            return;
        }
        self.evict(engine, loader);
    }

    /// Bodies dressed since the last check (each a mesh, materials and pictures) are let free
    /// what nothing draws when over the budget, at most once each [`DRESSED_EVICT_EVERY`]: a busy
    /// place fills the texture slots without crossing a block.
    fn note_dressing(&mut self, loader: &AssetLoader) {
        let (seen, at) = self.dressed_seen;
        if loader.dressed_made == seen || at.is_some_and(|t| t.elapsed() < DRESSED_EVICT_EVERY) {
            return;
        }
        self.dressed_seen = (loader.dressed_made, Some(Instant::now()));
        self.evict_due = true;
    }

    /// Frees what nothing draws.
    pub fn evict(&mut self, engine: &mut Engine, loader: &mut AssetLoader) {
        let ground = self
            .near
            .values()
            .chain(self.loading.as_ref().map(|l| &l.near))
            .filter_map(|n| n.ground.as_ref());
        self.stats.evicted = crate::evict::evict(engine, loader, ground);
        self.stats.evictions += 1;
    }

    /// The far trees wanted round the drawn square: asked for, and those no longer wanted taken
    /// out.
    fn ask_trees(&mut self, loader: &AssetLoader) {
        let Some(c) = self.centre else {
            return;
        };
        if loader.pack().is_none() {
            return;
        }
        let rings = crate::world::far_tree_rings(self.radius, self.far);
        if rings <= 0 {
            return;
        }
        let outer = self.radius + rings;
        let mut wanted: Vec<Block> = (c.0 - outer..=c.0 + outer)
            .flat_map(|x| (c.1 - outer..=c.1 + outer).map(move |y| (x, y)))
            .filter(|b| in_world(*b) && !self.within(*b, c))
            .collect();
        wanted.sort_by_key(|b| chebyshev(*b, c));
        for b in wanted {
            if !self.trees.contains_key(&b)
                && !self.tree_plans.contains_key(&b)
                && self.asked_trees.insert(b)
            {
                #[allow(clippy::cast_precision_loss)]
                let d = Vec2::new((b.0 - c.0) as f32, (b.1 - c.1) as f32).length() * 192.0;
                // The eye may stand anywhere in the centre block and a cell past it.
                let levels = crate::far::levels_near(self.first_range(), d, 96.0 + MARGIN);
                self.workers
                    .ask(Job::Trees(self.generation, b, levels), false);
            }
        }
    }

    /// One far block's trees placed or taken out; whether there was one.
    fn step_trees(&mut self, engine: &mut Engine, loader: &mut AssetLoader) -> bool {
        let Some(c) = self.centre else {
            return false;
        };
        self.stats.step = ("trees", c);
        if self.tree_run.is_some() {
            self.place_tree_run(engine, loader);
            return true;
        }
        let rings = crate::world::far_tree_rings(self.radius, self.far);
        let outer = self.radius + rings.max(0);
        let (r, target) = (self.radius, self.target);
        let loading = self.loading.as_ref().map(|l| l.block);
        let near = &self.near;
        // A block keeps its far trees while it is in the rings, or in the square being loaded
        // until its own scenery takes their place; never once it is placed.
        let keep = |b: Block| {
            let ring = chebyshev(b, c) > r && chebyshev(b, c) <= outer;
            let awaiting = target.is_some_and(|t| chebyshev(b, t) <= r);
            (ring || awaiting) && !near.contains_key(&b) && loading != Some(b)
        };
        let stale: Vec<Block> = self.trees.keys().copied().filter(|b| !keep(*b)).collect();
        for b in stale {
            if let Some(t) = self.trees.remove(&b) {
                for i in t.instances {
                    engine.scene.remove_instance(i);
                }
            }
            self.asked_trees.remove(&b);
        }
        self.tree_plans.retain(|b, _| chebyshev(*b, c) <= outer + 1);
        self.asked_trees.retain(|b| chebyshev(*b, c) <= outer + 1);
        // New trees only where no block is about to be placed.
        let next = self
            .tree_plans
            .keys()
            .copied()
            .filter(|b| {
                keep(*b)
                    && !self.trees.contains_key(b)
                    && !target.is_some_and(|t| chebyshev(*b, t) <= r)
            })
            .min_by_key(|b| (chebyshev(*b, c), b.0, b.1));
        let Some(b) = next else {
            return false;
        };
        let scenery = self.tree_plans.remove(&b).unwrap_or_default();
        self.trees.insert(b, Trees::default());
        self.tree_run = Some((b, scenery, 0));
        self.place_tree_run(engine, loader);
        true
    }

    /// Places the next run of the far block being given its trees.
    fn place_tree_run(&mut self, engine: &mut Engine, loader: &mut AssetLoader) {
        let Some((b, scenery, from)) = self.tree_run.take() else {
            return;
        };
        let to = (from + SCENERY_RUN * 2).min(scenery.len());
        let ring = self.centre.map_or(0, |c| chebyshev(b, c) - self.radius);
        world::place_far_trees(
            engine,
            loader,
            &scenery[from..to],
            self.block_origin(b),
            world::far_tree_keep(ring),
        );
        let placed = loader.take_placed();
        match self.trees.get_mut(&b) {
            Some(t) => t.instances.extend(placed),
            None => placed
                .into_iter()
                .for_each(|i| engine.scene.remove_instance(i)),
        }
        if to < scenery.len() && self.trees.contains_key(&b) {
            self.tree_run = Some((b, scenery, to));
        }
    }

    /// Starts a jump to the square round `viewer`: the world cleared and every block asked for,
    /// to be placed within the budget and drawn when the whole square is in.
    fn begin_jump(&mut self, engine: &mut Engine, loader: &mut AssetLoader, viewer: Block) {
        engine.clear_world(loader);
        self.reset();
        self.origin = viewer;
        self.target = Some(viewer);
        for b in self.square(viewer, viewer) {
            self.asked.insert(b);
            self.workers.ask(Job::Block(self.generation, b, true), true);
        }
        if self.far > self.radius {
            self.far_asked = Some(viewer);
            self.workers.ask(
                Job::Far(self.generation, self.area(), viewer, self.far),
                true,
            );
        }
        engine.renderer.world_origin = self.area().world_origin();
        self.jump_started = Some(Instant::now());
    }

    /// Places the rest of the jump at once, waiting on the workers.
    fn complete_jump(&mut self, engine: &mut Engine, loader: &mut AssetLoader, viewer: Block) {
        let started = Instant::now();
        while self.jumping() && started.elapsed().as_secs() < 30 {
            self.receive();
            if !self.step(engine, loader, viewer) {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }

    /// Loads the square round `viewer` at once (the first, or after a jump): every block planned
    /// on the workers, then placed; the far trees follow.
    fn full_load(&mut self, engine: &mut Engine, loader: &mut AssetLoader, viewer: Block) {
        let started = Instant::now();
        engine.clear_world(loader);
        self.reset();
        self.origin = viewer;
        let blocks = self.square(viewer, viewer);
        for &b in &blocks {
            self.asked.insert(b);
            self.workers
                .ask(Job::Block(self.generation, b, false), true);
        }
        let want_far = self.far > self.radius;
        if want_far {
            self.far_asked = Some(viewer);
            self.workers.ask(
                Job::Far(self.generation, self.area(), viewer, self.far),
                true,
            );
        }
        // Wait for the plans (and the far field) to come back.
        while blocks.iter().any(|b| !self.plans.contains_key(b))
            || (want_far && self.far_field.is_none())
        {
            match self
                .workers
                .done
                .recv_timeout(std::time::Duration::from_secs(30))
            {
                Ok(done) => match done {
                    Done::Block(g, plan) if g == self.generation => {
                        self.plans.insert(plan.block, Arc::from(plan));
                    }
                    Done::Far(g, c, f) if g == self.generation => {
                        self.far_field = Some((c, *f));
                    }
                    _ => {}
                },
                Err(_) => break,
            }
        }
        // Placed in the order the whole area is: every block's rooms, then each one's ground,
        // statics and scenery.
        let plans: Vec<Arc<BlockPlan>> = blocks
            .iter()
            .filter_map(|b| self.plans.get(b).cloned())
            .collect();
        let mut placed: Vec<(Block, Near)> = Vec::with_capacity(plans.len());
        for p in &plans {
            let first = loader.object_mark();
            let rooms =
                world::place_rooms(engine, loader, &self.store, p, self.block_origin(p.block));
            placed.push((
                p.block,
                Near {
                    rooms,
                    instances: loader.take_placed(),
                    objects: vec![(first, loader.object_mark())],
                    ..Near::default()
                },
            ));
        }
        for (p, (_, near)) in plans.iter().zip(placed.iter_mut()) {
            let origin = self.block_origin(p.block);
            near.ground = world::place_ground(engine, loader, &self.store, &self.region, p, origin);
            if let Some(g) = near.ground.as_ref() {
                near.ground_instance = engine.scene.add_instance(g.instance()).ok();
            }
            let first = loader.object_mark();
            world::place_statics(engine, loader, &self.store, p, origin, &near.rooms);
            world::place_scenery(engine, loader, &self.store, p, origin, 0..p.scenery.len());
            near.instances.extend(loader.take_placed());
            // Its rooms' numbers came with every block's rooms; its own follow here.
            near.objects.push((first, loader.object_mark()));
        }
        for (b, near) in placed {
            self.stats.blocks_placed += 1;
            self.near.insert(b, near);
        }
        engine.renderer.world_origin = self.area().world_origin();
        if engine.renderer.cover.is_none() {
            world::load_cover(engine, loader, &self.store, &self.region);
        }
        let r = self.radius;
        let lands: Vec<&crate::land::BlockLand> =
            plans.iter().filter_map(|p| p.land.as_ref()).collect();
        let first = (viewer.0 - r, viewer.1 - r);
        let land = world::land_field(
            &lands,
            first,
            usize::try_from(2 * r + 1).unwrap_or(1),
            self.block_origin(first).truncate(),
        );
        world::set_land(engine, land);
        if let Some(m) = loader.terrain_material(engine) {
            engine.renderer.terrain_material = m;
        }
        if let Some((_, f)) = self.far_field.take() {
            world::set_far(engine, loader, f, self.first_range());
        }
        self.far_asked = None;
        self.centre = Some(viewer);
        self.stats.full_loads += 1;
        self.evict_if_over(engine, loader);
        self.stats.full_load_ms = started.elapsed().as_secs_f64() * 1e3;
    }

    /// Moves render space's origin to block `to`: everything placed moves the other way.
    fn rebase(&mut self, engine: &mut Engine, loader: &mut AssetLoader, to: Block) {
        #[allow(clippy::cast_precision_loss)]
        let delta = Vec3::new(
            ((self.origin.0 - to.0) * 192) as f32,
            ((self.origin.1 - to.1) * 192) as f32,
            0.0,
        );
        self.origin = to;
        engine.translate(loader, delta);
        for n in self.near.values_mut() {
            if let Some(g) = n.ground.as_mut() {
                g.translate(delta);
            }
        }
        if let Some(l) = self.loading.as_mut() {
            if let Some(g) = l.near.ground.as_mut() {
                g.translate(delta);
            }
        }
        engine.renderer.world_origin = self.area().world_origin();
        // A far field or land field asked for in the old space is asked for again.
        self.far_field = None;
        self.far_asked = None;
        self.land = None;
        self.land_asked = None;
        self.stats.rebases += 1;
    }
}
