//! A preview space draws its bodies' particles: the smoke an Umbraen body's default script makes
//! shows in the space's picture, turned to its camera.

use std::sync::Arc;

use glam::{Mat4, Vec3};
use pyreal_engine::preview::{particle_draws, PreviewPass, PreviewView};
use pyreal_engine::world::AssetLoader;
use pyreal_primitives::{DataId, Frame};

/// The Umbraen male body, whose default script trails smoke.
const SMOKING: DataId = DataId(0x0200_196F);

#[test]
fn a_preview_space_draws_its_bodies_particles() {
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let mut engine = super::engine();
    let mut loader = AssetLoader::default();
    engine.particles.assets = Some(Arc::new(
        pyreal_world_data::anim_assets::DatAnimAssets::new(Arc::clone(&store)),
    ));
    for g in engine.particles.setup_sprites(SMOKING) {
        pyreal_engine::particles::prepare_sprite(&mut engine, &mut loader, &store, g);
    }
    assert!(engine.particles.spawn(SMOKING, Frame::default(), 1.0, 0, 1));
    let eye = Vec3::new(0.0, -4.0, 1.2);
    let mut now = 0.0;
    for _ in 0..120 {
        now += 1.0 / 60.0;
        engine.particles.step(now, eye);
    }
    let draws = particle_draws(&engine.particles, &engine.particles.hosts[0].driver, eye);
    assert!(!draws.is_empty(), "the body gives off particles");
    engine.upload().expect("the pools");
    let mut pass = PreviewPass::new(&engine.gpu, &engine.materials);
    let view = PreviewView {
        view_proj: Mat4::perspective_rh(0.9, 1.0, 0.05, 100.0)
            * Mat4::look_at_rh(eye, Vec3::new(0.0, 0.0, 1.0), Vec3::Z),
        light_dir: Vec3::new(-0.3, -1.0, 0.6).normalize(),
        light: Vec3::splat(0.6),
        ambient: Vec3::splat(0.4),
    };
    let target = engine.gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("preview"),
        size: wgpu::Extent3d {
            width: 128,
            height: 128,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: pyreal_engine::overlay::FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let covered = |engine: &mut pyreal_engine::Engine, pass: &mut PreviewPass, draws: &[_]| {
        pass.draw(
            &engine.gpu,
            &engine.scene,
            &engine.materials,
            &view,
            draws,
            &target,
        );
        pyreal_engine::read_texture(&engine.gpu, &target, 4)
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p.iter().any(|c| *c > 0))
            .count()
    };
    let with = covered(&mut engine, &mut pass, &draws);
    let without = covered(&mut engine, &mut pass, &[]);
    eprintln!("{} particles cover {with} pixels", draws.len());
    assert_eq!(without, 0);
    assert!(with > 20, "the particles show: {with} pixels");
}
