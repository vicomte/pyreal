//! Posed bodies: an art pack's skinned mesh drawn in place of a setup's parts
//! ([`pyreal_art_pack::skin`]).
//!
//! Each object a body is drawn for has a mesh of its own in the pools, pinned, written again each
//! frame with the posed vertices (on the processor) and drawn by one dynamic instance standing at
//! the object's place and scale. Each vertex also carries how far it moved since the last frame
//! ([`crate::scene::SKINNED`]), so its motion is right for the temporal passes, and the ray
//! structures build its bottom level again each frame.
//!
//! **What plays.** The object's motion picks a slot ([`slot_of`]); the body plays its clip for
//! it, or for the first fallback it has. With no clip for it, the joints that name parts follow
//! the parts as the game animates them, and the body's own bones hang from them at rest.

use glam::{Affine3A, Mat4, Vec3};
use pyreal_animation::{MotionCommand, MotionDriver};
use pyreal_art_pack::skin::{Play, Posed, SkinModel, Slot};
use pyreal_art_pack::PackSkin;

use crate::asset::mesh::{MeshData, Sides, SubMesh, Vertex};
use crate::scene::{Instance, MeshId, Scene, SKINNED};
use crate::Engine;

/// How much farther than at rest a body may reach as it moves, for culling.
pub const REACH: f32 = 1.6;

/// The mesh of a body posed as `posed`, one sub-mesh a primitive.
#[must_use]
pub fn rest_mesh(model: &SkinModel, posed: &[Posed]) -> MeshData {
    let mut mesh = MeshData::default();
    let mut at = 0usize;
    for (k, p) in model.primitives.iter().enumerate() {
        let base = u32::try_from(mesh.vertices.len()).unwrap_or(0);
        for v in posed.iter().skip(at).take(p.positions.len()) {
            mesh.vertices.push(Vertex {
                position: v.position.to_array(),
                normal: v.normal.to_array(),
                uv: v.uv,
                extra: [0, 0],
            });
        }
        at += p.positions.len();
        let first = u32::try_from(mesh.indices.len()).unwrap_or(0);
        mesh.indices.extend(p.indices.iter().map(|i| i + base));
        mesh.submeshes.push(SubMesh {
            surface_index: u16::try_from(k).unwrap_or(u16::MAX),
            surface: None,
            sides: if p.material.double_sided {
                Sides::Both
            } else {
                Sides::One
            },
            first_index: first,
            index_count: u32::try_from(p.indices.len()).unwrap_or(0),
        });
    }
    mesh.fit_bounds();
    mesh
}

/// What the object's motion does now, as a body plays it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionNow {
    pub slot: Slot,
    /// Played backwards: a turn or side step to the left, a walk back with the walk's clip.
    pub backwards: bool,
    /// The motion's own speed.
    pub rate: f32,
    /// The one-shot motion playing, if any (its command), so a new one starts its clip again.
    pub action: Option<u32>,
}

/// The slot a stance's still state takes.
fn idle_of(style: MotionCommand) -> Slot {
    match style {
        MotionCommand::NON_COMBAT => Slot::Idle,
        MotionCommand::BOW_COMBAT
        | MotionCommand::CROSSBOW_COMBAT
        | MotionCommand::SLING_COMBAT
        | MotionCommand::THROWN_WEAPON_COMBAT => Slot::IdleMissile,
        MotionCommand::MAGIC => Slot::IdleMagic,
        s if s.0 & 0xFF00_0000 == 0x8000_0000 && s.0 != 0x8000_0000 => Slot::IdleMelee,
        _ => Slot::Idle,
    }
}

/// The slot a one-shot motion (an attack, a hit, an emote) takes; `None` for one a body does not
/// show.
fn action_slot(m: MotionCommand) -> Option<Slot> {
    use MotionCommand as M;
    let id = m.0;
    Some(match m {
        M::CHEER => Slot::Cheer,
        M::JUMP | M::JUMPUP | M::HOP => Slot::Jump,
        M::SHOOT => Slot::Missile,
        M::STAGGER_BACKWARD | M::STAGGER_FORWARD | M::FALL_DOWN => Slot::HitHeavy,
        M::TWITCH1 | M::TWITCH2 | M::TWITCH3 | M::TWITCH4 => Slot::Hit,
        M::THRUST_HIGH | M::SLASH_HIGH | M::BACKHAND_HIGH => Slot::MeleeHigh,
        M::THRUST_MED | M::SLASH_MED | M::BACKHAND_MED => Slot::MeleeMed,
        M::THRUST_LOW | M::SLASH_LOW | M::BACKHAND_LOW => Slot::MeleeLow,
        M::HEAD_THROW | M::FIST_SLAM | M::BREATHE_FLAME_ | M::SPIN_ATTACK => Slot::Special,
        _ if (0x1000_0062..=0x1000_006A).contains(&id) => match (id - 0x1000_0062) % 3 {
            0 => Slot::MeleeHigh,
            1 => Slot::MeleeMed,
            _ => Slot::MeleeLow,
        },
        _ if (0x1000_006F..=0x1000_0078).contains(&id) => Slot::Telegraph,
        _ if (0x4000_002B..=0x4000_0039).contains(&id) => Slot::Cast,
        _ if id >> 24 == 0x13 => Slot::Emote,
        _ => return None,
    })
}

/// What `driver`'s motion does now.
#[must_use]
pub fn slot_of(driver: &MotionDriver) -> MotionNow {
    use MotionCommand as M;
    let state = &driver.motion_table.state;
    let rate = state.substate_mod.abs().max(0.05);
    if let Some(a) = state
        .actions
        .front()
        .and_then(|a| Some((a.motion, action_slot(a.motion)?)))
    {
        return MotionNow {
            slot: a.1,
            backwards: false,
            rate: 1.0,
            action: Some(a.0 .0),
        };
    }
    let turning = state
        .modifiers
        .iter()
        .find(|m| matches!(m.motion, M::TURN_LEFT | M::TURN_RIGHT));
    let (slot, backwards) = match state.substate {
        M::DEAD => (Slot::Die, false),
        M::SLEEPING => (Slot::Sleep, false),
        M::FALLING => (Slot::Fall, false),
        M::RUN_FORWARD => (Slot::Run, state.substate_mod < 0.0),
        M::WALK_FORWARD => (
            if state.substate_mod < 0.0 {
                Slot::WalkBack
            } else {
                Slot::Walk
            },
            false,
        ),
        M::WALK_BACKWARDS => (Slot::WalkBack, false),
        M::SIDE_STEP_RIGHT => (Slot::SideStep, state.substate_mod < 0.0),
        M::SIDE_STEP_LEFT => (Slot::SideStep, state.substate_mod >= 0.0),
        M::TURN_RIGHT => (Slot::Turn, state.substate_mod < 0.0),
        M::TURN_LEFT => (Slot::Turn, state.substate_mod >= 0.0),
        _ => match turning {
            Some(t) => (
                Slot::Turn,
                (t.motion == M::TURN_LEFT) != (t.speed_mod < 0.0),
            ),
            None => (idle_of(state.style), false),
        },
    };
    MotionNow {
        slot,
        backwards,
        rate,
        action: None,
    }
}

/// Where a body is in its clips.
#[derive(Debug, Clone, Default)]
pub struct Animator {
    /// The slot and clip playing, and the one-shot it plays for.
    playing: Option<(Slot, usize, Option<u32>)>,
    /// Seconds into the clip.
    time: f32,
    /// A one-shot that has played through, while the motion still names it.
    done: Option<u32>,
}

impl Animator {
    /// Steps `dt` seconds of `now`, the body having travelled `moved` metres; the clip to pose
    /// and the seconds into it, `None` where the parts lead.
    pub fn step(
        &mut self,
        skin: &PackSkin,
        mut now: MotionNow,
        dt: f32,
        moved: f32,
    ) -> Option<(usize, f32)> {
        if now.action.is_some() && now.action == self.done {
            now = MotionNow {
                slot: Slot::Idle,
                backwards: false,
                rate: 1.0,
                action: None,
            };
        } else if now.action.is_none() {
            self.done = None;
        }
        let mut slot = now.slot;
        let mut clip = skin.clip_for(slot);
        // A walk back with the walk's own clip plays it backwards.
        let mut backwards = now.backwards;
        if slot == Slot::WalkBack && !skin.clips.iter().any(|c| c.0 == Slot::WalkBack) {
            backwards = !backwards;
        }
        if slot == Slot::Die && clip.is_none() {
            slot = Slot::Dead;
            clip = skin.clip_for(slot);
        }
        let Some(clip) = clip else {
            self.playing = None;
            return None;
        };
        let c = skin.model.clips.get(clip)?;
        let duration = c.duration.max(1e-3);
        let key = (slot, clip, now.action);
        if self.playing != Some(key) {
            let keep_phase = self
                .playing
                .is_some_and(|(s, k, _)| k == clip && s.play() == slot.play());
            self.playing = Some(key);
            if !keep_phase {
                self.time = if backwards { duration } else { 0.0 };
            }
        }
        let sign = if backwards { -1.0 } else { 1.0 };
        match slot.play() {
            Play::Loop => {
                self.time = (self.time + sign * dt * now.rate).rem_euclid(duration);
            }
            Play::Stride => {
                let step = match c.stride {
                    Some(stride) => moved / stride * duration,
                    None => dt * now.rate,
                };
                self.time = (self.time + sign * step).rem_euclid(duration);
            }
            Play::Once => {
                self.time += dt;
                if self.time >= duration {
                    self.done = now.action;
                    self.time = duration;
                }
            }
            Play::Hold => self.time = (self.time + dt).min(duration),
        }
        Some((clip, self.time))
    }
}

/// One object's posed body.
#[derive(Debug)]
pub struct SkinnedBody {
    /// The pack body's place in the pack.
    pub skin: usize,
    pub mesh: MeshId,
    pub instance: Option<u32>,
    pub animator: Animator,
    /// Where the body's vertices stood last frame, object space.
    last: Vec<Vec3>,
    posed: Vec<Posed>,
    bytes: Vec<u8>,
    /// Where the body stood last frame, for how far it walked.
    last_at: Option<Vec3>,
}

impl SkinnedBody {
    /// The pack's `skin`th body made for one object, its mesh posed at rest.
    pub fn new(
        engine: &mut Engine,
        loader: &mut crate::world::AssetLoader,
        skin: usize,
    ) -> Option<Self> {
        let mesh = loader.skin_mesh(engine, skin)?;
        engine.renderer.rays.deforming.insert(mesh.0);
        Some(Self {
            skin,
            mesh,
            instance: None,
            animator: Animator::default(),
            last: Vec::new(),
            posed: Vec::new(),
            bytes: Vec::new(),
            last_at: None,
        })
    }

    /// Poses the body for one frame and draws it at `root` (the object's place and scale, engine
    /// space) as object `object` in cell word `cell`. `parts` are the object's parts' transforms
    /// in the same space, for joints that follow them; `driver` says what plays; `dt` is the
    /// frame's seconds.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        engine: &mut Engine,
        skin: &PackSkin,
        root: Affine3A,
        parts: &[Affine3A],
        driver: &MotionDriver,
        dt: f32,
        object: u32,
        cell: u32,
    ) {
        let at = Vec3::from(root.translation);
        let scale = root.matrix3.x_axis.length().max(1e-4);
        let moved = self.last_at.map_or(0.0, |p| p.distance(at)) / scale;
        self.last_at = Some(at);
        let now = slot_of(driver);
        let playing = self.animator.step(skin, now, dt, moved);
        let model = &skin.model;
        let mut locals = model.rig.rest();
        let mut fixed = Vec::new();
        match playing.and_then(|(c, t)| Some((model.clips.get(c)?, t))) {
            Some((clip, t)) => clip.sample(t, &mut locals),
            None => {
                let into = Mat4::from(root.inverse());
                for j in &model.rig.joints {
                    if let Some(p) = j.part.and_then(|p| parts.get(usize::from(p))) {
                        let rig = pyreal_art_pack::skin::part_to_rig(into * Mat4::from(*p));
                        fixed.push((j.node, rig));
                    }
                }
            }
        }
        let globals = model.rig.globals(&locals, &fixed);
        model.pose(&model.rig.joint_matrices(&globals), &mut self.posed);
        let first = self.last.len() != self.posed.len();
        self.bytes.clear();
        for (k, v) in self.posed.iter().enumerate() {
            let was = if first { v.position } else { self.last[k] };
            let d = was - v.position;
            let half = |a: f32, b: f32| half_bits(a) | half_bits(b) << 16;
            Vertex {
                position: v.position.to_array(),
                normal: v.normal.to_array(),
                uv: v.uv,
                extra: [half(d.x, d.y), half(d.z, 0.0)],
            }
            .write(&mut self.bytes);
        }
        self.last.clear();
        self.last.extend(self.posed.iter().map(|v| v.position));
        engine
            .scene
            .write_vertices(&engine.gpu.queue, self.mesh, &self.bytes);
        let mut instance = Instance::at(self.mesh, root, object, cell);
        instance.sway = SKINNED;
        match self.instance {
            Some(i) => {
                engine.scene.set_transform(i, root);
                engine.scene.set_cell(i, cell);
            }
            None => self.instance = engine.scene.add_dynamic(instance).ok(),
        }
    }

    /// Takes the body out of the scene and frees its mesh.
    pub fn remove(self, engine: &mut Engine) {
        Self::free(engine, self.mesh, self.instance);
    }

    fn free(engine: &mut Engine, mesh: MeshId, instance: Option<u32>) {
        if let Some(i) = instance {
            engine.scene.remove_instance(i);
        }
        engine.renderer.rays.forget(mesh);
        engine.scene.unpin(mesh);
        engine.scene.free_mesh(mesh);
    }
}

/// Each of `driver`'s parts' frames at the object's scale, for joints that follow them: the
/// setup's own part scale is the bind pose's.
#[must_use]
pub fn part_frames(driver: &MotionDriver) -> Vec<Affine3A> {
    let s = Affine3A::from_scale(Vec3::splat(driver.scale.max(1e-4)));
    driver
        .part_array
        .parts
        .iter()
        .map(|p| crate::asset::object::frame_affine(&p.pos) * s)
        .collect()
}

/// The palettes `driver`'s look lays over its parts: each part's shift palette and its ranges'.
#[must_use]
pub fn palettes_of(driver: &MotionDriver) -> Vec<pyreal_primitives::DataId> {
    let mut out = Vec::new();
    for o in driver
        .part_array
        .parts
        .iter()
        .filter_map(|p| p.surface_overrides.as_ref())
    {
        out.extend(o.shift_palette);
        out.extend(o.subpalettes.iter().map(|r| r.palette_set));
    }
    out.sort_unstable_by_key(|d| d.0);
    out.dedup();
    out
}

/// A float's half-precision bits, signed (rounded toward zero, tiny values flushed to zero).
#[must_use]
pub fn half_bits(v: f32) -> u32 {
    let sign = if v < 0.0 { 0x8000 } else { 0 };
    let b = v.abs().min(65_000.0).to_bits();
    let exp = i32::try_from((b >> 23) & 0xFF).unwrap_or(0) - 127 + 15;
    if exp <= 0 || !v.is_finite() {
        return 0;
    }
    #[allow(clippy::cast_sign_loss)]
    let out = ((exp as u32) << 10) | ((b >> 13) & 0x3FF);
    out | sign
}

/// Whether `scene` still holds `body`'s mesh (a world cleared under it frees it).
#[must_use]
pub fn alive(scene: &Scene, body: &SkinnedBody) -> bool {
    scene.mesh(body.mesh).is_some_and(|m| m.live)
}
