//! The preview spaces on the engine: the character-creation model, the paper doll, the examined
//! object's portrait and the portal swirl. Each is a [`pyreal_world_render::creature_mode::CreatureMode`] (camera, field of view,
//! light) and its objects, bodies posed by the client's own animation code, drawn by the engine's
//! preview pass ([`pyreal_engine::preview`]) into a picture the overlay lays down.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use pyreal_animation::data::AnimAssets;
use pyreal_client_contract::overlay::PreviewSpace as PreviewId;
use pyreal_dat::RetailDatStore;
use pyreal_engine::preview::{PreviewDraw, PreviewView};
use pyreal_engine::scene::MeshId;
use pyreal_engine::world::{AssetLoader, Dress};
use pyreal_engine::Engine;
use pyreal_primitives::{DataId, Frame, Quat, Vec3};
use pyreal_world_render::creature_mode::{CreatureMode, PreviewProjection};

pub use crate::preview::{preview_body, step_preview_body};

/// One object of a space.
pub struct PreviewObject {
    pub setup: DataId,
    pub body: pyreal_animation::MotionDriver,
    pub frame: Frame,
    /// Each part's mesh and what it was dressed as, made on first draw.
    meshes: Vec<Option<(DataId, Dress, Option<MeshId>)>>,
    /// The engine generation the meshes were made in.
    made_in: u64,
}

impl std::fmt::Debug for PreviewObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreviewObject")
            .field("setup", &self.setup)
            .field("parts", &self.body.part_array.parts.len())
            .finish_non_exhaustive()
    }
}

/// One space: its camera and light, its objects and their clock, and its picture.
pub struct Space {
    pub mode: CreatureMode,
    objects: Vec<PreviewObject>,
    assets: Arc<pyreal_world_data::anim_assets::DatAnimAssets>,
    clock: f64,
    picture: Option<wgpu::Texture>,
}

impl std::fmt::Debug for Space {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Space")
            .field("objects", &self.objects.len())
            .finish_non_exhaustive()
    }
}

impl Space {
    fn new(assets: Arc<pyreal_world_data::anim_assets::DatAnimAssets>) -> Self {
        let mut mode = CreatureMode::new();
        mode.initialize_scene();
        Self {
            mode,
            objects: Vec::new(),
            assets,
            clock: 0.0,
            picture: None,
        }
    }

    /// Adds setup `setup` dressed in `objdesc`; its index, `None` when it will not load.
    pub fn add_object(
        &mut self,
        setup: DataId,
        objdesc: Option<&pyreal_animation::parts::ObjDesc>,
    ) -> Option<usize> {
        let data = if setup.raw() >> 24 == 0x01 {
            Arc::new(pyreal_animation::data::SetupData {
                parts: vec![setup],
                ..pyreal_animation::data::SetupData::default()
            })
        } else {
            self.assets.setup(setup)?
        };
        let assets: Arc<dyn AnimAssets> = Arc::clone(&self.assets) as _;
        let mut body = preview_body(assets, data)?;
        body.part_array.set_placement_frame(0, &mut body.sequence);
        let frame = Frame::new(Vec3::ZERO, Quat::IDENTITY);
        body.update_parts(&frame);
        if let Some(od) = objdesc {
            let _ = body
                .part_array
                .do_obj_desc_changes_from_default_with(od, &*self.assets);
        }
        self.objects.push(PreviewObject {
            setup,
            body,
            frame,
            meshes: Vec::new(),
            made_in: u64::MAX,
        });
        Some(self.objects.len() - 1)
    }

    pub fn remove_all_objects(&mut self) {
        self.objects.clear();
    }

    #[must_use]
    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    pub fn set_sequence_animation(
        &mut self,
        i: usize,
        anim: DataId,
        clear: bool,
        low_frame: i32,
        framerate: f32,
    ) -> bool {
        let Some(o) = self.objects.get_mut(i) else {
            return false;
        };
        if clear {
            o.body.sequence.clear();
        }
        o.body.sequence.append_animation(
            pyreal_animation::data::AnimData {
                anim_id: anim,
                low_frame,
                high_frame: -1,
                framerate,
            },
            &*self.assets,
        );
        o.body.sequence.has_anims()
    }

    #[must_use]
    pub fn has_anims(&self, i: usize) -> bool {
        self.objects
            .get(i)
            .is_some_and(|o| o.body.sequence.has_anims())
    }

    pub fn clear_sequence_anims(&mut self, i: usize) {
        if let Some(o) = self.objects.get_mut(i) {
            o.body.sequence.clear_animations();
        }
    }

    /// The current sequence frame, as the unsigned number the tunnel's exit test reads.
    #[must_use]
    pub fn curr_frame_number(&self, i: usize) -> u32 {
        #[allow(clippy::cast_sign_loss)]
        // LINT-OK: the client's own `int -> ulong`; a negative frame number is the wrap the
        // exit window relies on.
        self.objects
            .get(i)
            .map_or(0, |o| o.body.sequence.curr_frame_number() as u32)
    }

    pub fn set_heading(&mut self, i: usize, degrees: f32) {
        if let Some(o) = self.objects.get_mut(i) {
            pyreal_terrain::math::set_heading(&mut o.frame, degrees);
        }
    }

    pub fn set_scale(&mut self, i: usize, scale: f32) {
        if let Some(o) = self.objects.get_mut(i) {
            let scale = if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                1.0
            };
            o.body
                .part_array
                .set_scale_internal(Vec3::new(scale, scale, scale));
            o.body.update_parts(&o.frame);
        }
    }

    /// Advances every object `dt` seconds on the space's clock.
    pub fn use_time(&mut self, dt: f64) {
        self.clock += dt;
        for o in &mut self.objects {
            step_preview_body(&mut o.body, &o.frame, dt, self.clock);
        }
    }
}

/// Every space this process holds.
#[derive(Debug, Default)]
pub struct Previews {
    // ORDER-OK: a lookup by a fixed key.
    spaces: BTreeMap<PreviewId, Space>,
    /// The engine generation and the meshes the spaces hold in it, pinned against eviction.
    pinned: (u64, BTreeSet<u32>),
}

impl Previews {
    /// Makes space `id` if it is not there (or was built over other files); whether it was made.
    pub fn ensure(
        &mut self,
        id: PreviewId,
        assets: &Arc<pyreal_world_data::anim_assets::DatAnimAssets>,
    ) -> bool {
        if self
            .spaces
            .get(&id)
            .is_some_and(|s| Arc::ptr_eq(&s.assets, assets))
        {
            return false;
        }
        self.spaces.insert(id, Space::new(Arc::clone(assets)));
        true
    }

    #[must_use]
    pub fn get(&self, id: PreviewId) -> Option<&Space> {
        self.spaces.get(&id)
    }

    pub fn get_mut(&mut self, id: PreviewId) -> Option<&mut Space> {
        self.spaces.get_mut(&id)
    }

    /// Draws space `id` into its picture, `size` big, with the world's field of view `world_fov`
    /// and the view-distance override `view_distance`; the picture, `None` with nothing to draw.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        id: PreviewId,
        engine: &mut Engine,
        pass: &mut pyreal_engine::preview::PreviewPass,
        loader: &mut AssetLoader,
        store: &RetailDatStore,
        generation: u64,
        size: (u32, u32),
        world_fov: f32,
        view_distance: Option<f32>,
    ) -> Option<wgpu::TextureView> {
        // Meshes no space holds any more are let go; those made in the drawing are kept.
        self.repin(engine, generation);
        let view = self.draw_space(
            id,
            engine,
            pass,
            loader,
            store,
            generation,
            size,
            world_fov,
            view_distance,
        );
        self.repin(engine, generation);
        view
    }

    /// The meshes every space's objects hold in engine generation `generation`.
    fn held(&self, generation: u64) -> BTreeSet<u32> {
        self.spaces
            .values()
            .flat_map(|s| &s.objects)
            .filter(|o| o.made_in == generation)
            .flat_map(|o| o.meshes.iter().flatten())
            .filter_map(|(_, _, m)| m.map(|m| m.0))
            .collect()
    }

    /// Pins the meshes the spaces hold and unpins those they let go (an engine made again
    /// dropped the last one's pins with it).
    fn repin(&mut self, engine: &mut Engine, generation: u64) {
        let held = self.held(generation);
        if self.pinned.0 != generation {
            self.pinned = (generation, BTreeSet::new());
        }
        for &m in held.difference(&self.pinned.1) {
            engine.scene.pin(MeshId(m));
        }
        for &m in self.pinned.1.difference(&held) {
            engine.scene.unpin(MeshId(m));
        }
        self.pinned.1 = held;
    }

    /// The meshes still pinned for the spaces.
    pub fn pinned(&self) -> impl Iterator<Item = MeshId> + '_ {
        self.pinned.1.iter().map(|&m| MeshId(m))
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_space(
        &mut self,
        id: PreviewId,
        engine: &mut Engine,
        pass: &mut pyreal_engine::preview::PreviewPass,
        loader: &mut AssetLoader,
        store: &RetailDatStore,
        generation: u64,
        size: (u32, u32),
        world_fov: f32,
        view_distance: Option<f32>,
    ) -> Option<wgpu::TextureView> {
        let space = self.spaces.get_mut(&id)?;
        if space.objects.is_empty() || size.0 == 0 || size.1 == 0 {
            return None;
        }
        // The meshes, made once a part.
        let mut draws = Vec::new();
        for o in &mut space.objects {
            if o.made_in != generation {
                o.meshes.clear();
                o.made_in = generation;
            }
            o.meshes.resize(o.body.part_array.parts.len(), None);
            for (k, p) in o.body.part_array.parts.iter().enumerate() {
                if p.no_draw() || p.gfxobj_id.0 == 0 {
                    continue;
                }
                let dress = Dress::of(p.surface_overrides.as_ref());
                let fresh =
                    !matches!(&o.meshes[k], Some((g, d, _)) if *g == p.gfxobj_id && *d == dress);
                if fresh {
                    // A space's meshes are drawn by no instance: pinned while held (`repin`).
                    let own = o
                        .body
                        .part_array
                        .setup
                        .as_ref()
                        .and_then(|s| s.parts.get(k));
                    let mesh = pyreal_engine::asset::object::near_mesh(store, p.gfxobj_id)
                        .and_then(|g| loader.mesh_worn(engine, store, g, own.copied(), &dress));
                    o.meshes[k] = Some((p.gfxobj_id, dress, mesh));
                }
                if let Some((_, _, Some(mesh))) = &o.meshes[k] {
                    // A face shown here is given its detail as it comes.
                    loader.want_faces(engine, store, *mesh);
                    draws.push(PreviewDraw::part(
                        *mesh,
                        pyreal_engine::body::part_affine(&p.pos, p.gfxobj_scale),
                    ));
                }
            }
            // The body's particles (a portal's swirl, a weapon's glow), their pictures made
            // ready the first time.
            for g in engine.particles.missing_sprites(&o.body) {
                pyreal_engine::particles::prepare_sprite(engine, loader, store, g);
                if let Some(Some(s)) = engine.particles.sprites.get(&g.0) {
                    engine.scene.pin(s.mesh);
                }
            }
        }
        let _ = engine.upload();
        // The camera, as the client's preview pass places it.
        #[allow(clippy::cast_precision_loss)]
        let (w, h) = (size.0 as f32, size.1 as f32);
        let display =
            pyreal_client_contract::camera::AspectPreference::Normal.display_aspect_ratio(w, h);
        let aspect =
            pyreal_client_contract::camera::compute_aspect_for_viewport(w, h, display, false);
        let fov = match space.mode.projection(view_distance, world_fov, aspect) {
            PreviewProjection::FovRadians(f) => f,
            PreviewProjection::ViewDistance(d) => pyreal_client_contract::camera::set_vdst(d).0,
        };
        let f = &space.mode.view_frame;
        let q = glam::Quat::from_xyzw(f.rotation.x, f.rotation.y, f.rotation.z, f.rotation.w);
        let q = if q.length_squared() > 0.0 {
            q.normalize()
        } else {
            glam::Quat::IDENTITY
        };
        let eye = glam::Vec3::new(f.origin.x, f.origin.y, f.origin.z);
        // Their particles as the world draws them: fire as fire, creatures' lights as light.
        let (mut fire, mut lamps) = (Vec::new(), Vec::new());
        for o in &space.objects {
            let made = pyreal_engine::preview::particles(&mut engine.particles, &o.body, eye);
            draws.extend(made.draws);
            fire.extend(made.fire);
            lamps.extend(made.lamps);
        }
        let view = glam::Mat4::look_to_rh(eye, q * glam::Vec3::Y, q * glam::Vec3::Z);
        let proj = glam::Mat4::perspective_rh(fov.clamp(0.01, 3.1), w / h, 0.05, 500.0);
        let light = space.mode.lights.first();
        let light_dir = light.map_or(glam::Vec3::new(-0.3, -1.0, 0.6), |l| {
            -glam::Vec3::new(l.direction.x, l.direction.y, l.direction.z)
        });
        let light_dir = light_dir.normalize_or(glam::Vec3::Z);
        let intensity = light.map_or(1.0, |l| l.intensity).clamp(0.0, 4.0);
        let colour = light.map_or([1.0; 3], |l| l.color);
        let a = space.mode.ambient;
        let pview = PreviewView {
            view_proj: proj * view,
            light_dir,
            light: glam::Vec3::from(colour) * intensity * 0.5,
            ambient: glam::Vec3::new(a[0], a[1], a[2]).max(glam::Vec3::splat(0.25)),
            #[allow(clippy::cast_possible_truncation)] // seconds the space has run
            time: space.clock as f32,
            lamps,
        };
        let fits = space
            .picture
            .as_ref()
            .is_some_and(|t| (t.width(), t.height()) == size);
        if !fits {
            space.picture = Some(engine.gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("preview"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: pyreal_engine::overlay::FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            }));
        }
        let picture = space.picture.as_ref()?;
        pass.draw(
            &engine.gpu,
            &engine.scene,
            &engine.materials,
            &pview,
            &draws,
            &fire,
            picture,
        );
        Some(picture.create_view(&Default::default()))
    }
}
