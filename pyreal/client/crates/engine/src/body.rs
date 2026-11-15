//! Animated bodies: a setup's parts posed every frame by the client's own animation code, each
//! part a dynamic instance. AC animates rigid parts, so a pose is one transform
//! per part, and each instance keeps the one it was drawn with last frame for motion vectors.
//!
//! The motion table, the frame player, the part array and its object description are
//! `pyreal-animation`'s [`MotionDriver`], as the client's world and preview spaces hold them: a
//! body is set up from its setup, dressed, told a style and a motion, advanced the way the
//! client's physics advances an object (sub-steps no longer than [`QUANTUM`]), and drawn where
//! the driver places its parts. Root motion is not applied: a body stays where it is put.

use std::sync::Arc;

use glam::{Affine3A, Vec3};
use pyreal_animation::data::AnimAssets;
use pyreal_animation::motion::MovementParameters;
use pyreal_animation::parts::ObjDesc;
use pyreal_animation::{step_animation, MotionCommand, MotionDriver};
use pyreal_dat::RetailDatStore;
use pyreal_primitives::{DataId, Frame, LocalTime};

use crate::asset::object::{frame_affine, near_mesh};
use crate::scene::{Instance, Scene};
use crate::world::{AssetLoader, Dress};
use crate::Engine;

/// The longest sub-step a body is advanced by: the client's physics quantum floor, at which a
/// frame's worth of world time is stepped.
pub const QUANTUM: f64 = 1.0 / 30.0;

/// A posed, animated object.
pub struct Body {
    pub driver: MotionDriver,
    /// Its setup.
    pub setup: DataId,
    /// Where the body stands, in render space.
    pub frame: Frame,
    /// The object number its instances carry.
    pub object: u32,
    /// The cell slot it is seen through.
    pub cell: u32,
    /// Each part's instance, `None` where the part draws nothing.
    instances: Vec<Option<u32>>,
    /// Each part hidden when the body was placed.
    hidden: Vec<bool>,
    /// Whether it was placed as a shadow ([`shadow_parts`]).
    pub shadow: bool,
    clock: f64,
    /// The art pack's body drawn in place of its parts, if any ([`crate::skin`]).
    pub skinned: Option<crate::skin::SkinnedBody>,
    /// Each part the pack's body is drawn in place of.
    skin_parts: Vec<bool>,
    /// Seconds of the last advance.
    step: f64,
}

impl std::fmt::Debug for Body {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Body")
            .field("frame", &self.frame)
            .field("parts", &self.driver.part_array.parts.len())
            .field("clock", &self.clock)
            .finish_non_exhaustive()
    }
}

/// Whether a part draws nothing: hidden (an animation hook's, or the object's), wholly faded, or
/// with no model.
#[must_use]
pub fn part_hidden(p: &pyreal_animation::parts::PhysicsPart) -> bool {
    p.no_draw() || p.material.is_some_and(|m| m.translucency >= 0.999) || p.gfxobj_id.0 == 0
}

/// A part's cell word in `cell`, faded by its translucency as the client's bridge fades it.
#[must_use]
pub fn part_cell(p: &pyreal_animation::parts::PhysicsPart, cell: u32) -> u32 {
    crate::scene::faded(cell, p.material.map_or(0.0, |m| m.translucency))
}

/// The described translucencies a shadow's is among.
pub const SHADOW_TRANSLUCENCY: std::ops::RangeInclusive<f32> = 0.3..=0.9;
/// The mean linear luminance the pictures of a shadow's own body stay under.
pub const SHADOW_LUMA: f32 = 0.03;

/// Which of `driver`'s parts draw as a shadow's smoke under the enhanced lighting; `None` for no
/// shadow (see-through as described, its own body dark).
pub fn shadow_parts(
    engine: &Engine,
    loader: &mut AssetLoader,
    store: &RetailDatStore,
    driver: &MotionDriver,
) -> Option<Vec<bool>> {
    if engine.renderer.calibration != crate::renderer::Calibration::Physical
        || !SHADOW_TRANSLUCENCY.contains(&driver.translucency_original)
    {
        return None;
    }
    let parts = &driver.part_array.parts;
    let body: Vec<bool> = (0..parts.len())
        .map(|k| {
            driver.fades_as_described(k)
                && !part_hidden(&parts[k])
                && parts[k].material.is_some_and(|m| m.translucency > 0.0)
        })
        .collect();
    let (mut sum, mut weight) = (0.0, 0.0);
    for (p, _) in parts.iter().zip(&body).filter(|(_, b)| **b) {
        let dress = Dress::of(p.surface_overrides.as_ref());
        if let Some((l, w)) = loader.dress_luma(store, p.gfxobj_id, &dress) {
            sum += l * w;
            weight += w;
        }
    }
    (weight > 0.0 && sum / weight < SHADOW_LUMA).then_some(body)
}

/// A part's frame and mesh scale as a transform.
#[must_use]
pub fn part_affine(pos: &Frame, scale: pyreal_primitives::Vec3) -> Affine3A {
    frame_affine(pos) * Affine3A::from_scale(Vec3::new(scale.x, scale.y, scale.z))
}

impl Body {
    /// Setup `setup` with its motion table in its default state, dressed in `objdesc`, standing
    /// at `frame`. `None` when the setup or one of its parts will not load.
    #[must_use]
    pub fn new(
        assets: Arc<dyn AnimAssets>,
        setup: DataId,
        objdesc: Option<&ObjDesc>,
        frame: Frame,
    ) -> Option<Self> {
        let data = assets.setup(setup)?;
        let mut driver = MotionDriver::new(assets);
        if !driver.set_setup(data) {
            return None;
        }
        if let Some(od) = objdesc {
            let _ = driver.do_obj_desc_changes(od);
        }
        Some(Self {
            driver,
            setup,
            frame,
            object: u32::MAX,
            cell: crate::cells::OUTSIDE,
            instances: Vec::new(),
            hidden: Vec::new(),
            shadow: false,
            clock: 0.0,
            skinned: None,
            skin_parts: Vec::new(),
            step: 0.0,
        })
    }

    /// Issues `command` (a style, such as `HandCombat`, or a motion, such as `WalkForward`) as
    /// the client issues an interpreted motion; the movement layer's error code, 0 when taken.
    pub fn command(&mut self, command: MotionCommand) -> u32 {
        self.driver
            .do_interpreted_motion(command, &MovementParameters::default())
    }

    /// Advances the animation `seconds`, in sub-steps of at most [`QUANTUM`].
    pub fn advance(&mut self, seconds: f64) {
        self.step = seconds;
        let mut left = seconds;
        while left > 1e-9 {
            let q = left.min(QUANTUM);
            self.clock += q;
            step_animation(&mut self.driver, q, LocalTime(self.clock));
            left -= q;
        }
        // The ramps the hooks start (a part fading out), once a frame as the client steps them.
        self.driver.cur_time = pyreal_primitives::ServerTime(self.clock);
        self.driver.update_fp_hooks();
    }

    /// Seconds advanced since the body was made.
    #[must_use]
    pub fn clock(&self) -> f64 {
        self.clock
    }

    /// Places every part from the current animation frame: each part's transform in render
    /// space.
    pub fn pose(&mut self) -> Vec<Affine3A> {
        self.driver.update_parts(&self.frame);
        self.driver
            .part_array
            .parts
            .iter()
            .map(|p| part_affine(&p.pos, p.gfxobj_scale))
            .collect()
    }

    /// Puts the body's parts into the scene as dynamic instances, each at its near level's mesh
    /// dressed as the part is; how many it placed.
    pub fn place(
        &mut self,
        engine: &mut Engine,
        loader: &mut AssetLoader,
        store: &RetailDatStore,
    ) -> usize {
        self.remove(&mut engine.scene);
        if self.object == u32::MAX {
            self.object = loader.next_object();
        }
        let pose = self.pose();
        let mut n = 0;
        let cell = self.cell;
        self.take_skin(engine, loader, store);
        let smoke = shadow_parts(engine, loader, store, &self.driver);
        self.shadow = smoke.is_some();
        let smoke = smoke.unwrap_or_default();
        let parts: Vec<(DataId, Dress, u32)> = self
            .driver
            .part_array
            .parts
            .iter()
            .enumerate()
            .map(|(k, p)| {
                let mut dress = Dress::of(p.surface_overrides.as_ref());
                dress.smoke = smoke.get(k).copied().unwrap_or(false);
                (p.gfxobj_id, dress, part_cell(p, cell))
            })
            .collect();
        self.hidden = self
            .driver
            .part_array
            .parts
            .iter()
            .enumerate()
            .map(|(k, p)| part_hidden(p) || self.skin_parts.get(k).copied().unwrap_or(false))
            .collect();
        for (((gfxobj, dress, word), at), &hidden) in parts.iter().zip(pose).zip(&self.hidden) {
            if hidden {
                self.instances.push(None);
                continue;
            }
            let mesh = near_mesh(store, *gfxobj)
                .and_then(|g| Some((g, loader.mesh_dressed(engine, store, g, dress)?)));
            let instance = mesh.and_then(|(g, mesh)| {
                loader.link_degrades_dressed(engine, store, mesh, g, false, dress);
                engine
                    .scene
                    .add_dynamic(Instance::at(mesh, at, self.object, *word))
                    .ok()
            });
            n += usize::from(instance.is_some());
            self.instances.push(instance);
        }
        n
    }

    /// Moves the body's instances to the current pose, taking out the parts hidden since; whether
    /// a part hidden when it was placed shows now (it shows once the body is placed again).
    pub fn update(&mut self, scene: &mut Scene) -> bool {
        let pose = self.pose();
        let mut shown = false;
        for (k, (instance, at)) in self.instances.iter_mut().zip(pose).enumerate() {
            let part = self.driver.part_array.parts.get(k);
            let hidden =
                part.is_none_or(part_hidden) || self.skin_parts.get(k).copied().unwrap_or(false);
            if hidden {
                if let Some(i) = instance.take() {
                    scene.remove_instance(i);
                }
            } else if let (Some(i), Some(p)) = (instance, part) {
                scene.set_transform(*i, at);
                scene.set_cell(*i, part_cell(p, self.cell));
            }
            shown |= !hidden && self.hidden.get(k).copied().unwrap_or(false);
            if let Some(h) = self.hidden.get_mut(k) {
                *h |= hidden;
            }
        }
        shown
    }

    /// Makes or lets go of the pack's body for its setup and look.
    fn take_skin(&mut self, engine: &mut Engine, loader: &mut AssetLoader, store: &RetailDatStore) {
        let palettes = crate::skin::palettes_of(&self.driver);
        let want = loader
            .skin_for(Some(store), self.setup, &palettes)
            .map(|(k, _)| k);
        if self.skinned.as_ref().map(|b| b.skin) != want {
            if let Some(old) = self.skinned.take() {
                old.remove(engine);
            }
            self.skinned = want.and_then(|k| crate::skin::SkinnedBody::new(engine, loader, k));
        }
        let parts = self
            .skinned
            .as_ref()
            .and_then(|b| loader.pack()?.pack.skins.get(b.skin))
            .map(|k| k.model.rig.parts())
            .unwrap_or_default();
        self.skin_parts = (0..self.driver.part_array.parts.len())
            .map(|k| u16::try_from(k).is_ok_and(|k| parts.contains(&k)))
            .collect();
    }

    /// Poses and draws the pack's body, if it has one, for the time last advanced.
    pub fn draw_skin(&mut self, engine: &mut Engine, loader: &AssetLoader) {
        let Some(body) = self.skinned.as_mut() else {
            return;
        };
        let Some(skin) = loader.pack().and_then(|p| p.pack.skins.get(body.skin)) else {
            return;
        };
        self.driver.update_parts(&self.frame);
        let s = self.driver.scale.max(1e-4);
        let parts = crate::skin::part_frames(&self.driver);
        let root = frame_affine(&self.frame) * Affine3A::from_scale(Vec3::splat(s));
        #[allow(clippy::cast_possible_truncation)]
        let dt = self.step as f32;
        body.update(
            engine,
            skin,
            root,
            &parts,
            &self.driver,
            dt,
            self.object,
            self.cell,
        );
    }

    /// The instances of the body's parts, `None` where a part draws nothing.
    #[must_use]
    pub fn instances(&self) -> &[Option<u32>] {
        &self.instances
    }

    /// Takes the body's parts out of the scene.
    pub fn remove(&mut self, scene: &mut Scene) {
        for i in self.instances.drain(..).flatten() {
            scene.remove_instance(i);
        }
    }

    /// Takes the body out of the engine, its pack body and that body's mesh too.
    pub fn remove_all(&mut self, engine: &mut Engine) {
        self.remove(&mut engine.scene);
        if let Some(b) = self.skinned.take() {
            b.remove(engine);
        }
    }
}
