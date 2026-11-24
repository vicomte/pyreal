//! A preview space draws its bodies' particles as the world draws them: the smoke an Umbraen body's
//! default script makes shows in the space's picture, turned to its camera; an Undead's flame is a
//! body of fire, added light carrying a lamp; an Empyrean's eyes are creatures' light.

use std::sync::Arc;

use glam::{Mat4, Vec3};
use pyreal_engine::preview::{PreviewPass, PreviewView};
use pyreal_engine::world::AssetLoader;
use pyreal_primitives::{DataId, Frame};

/// The Umbraen male body, whose default script trails smoke.
const SMOKING: DataId = DataId(0x0200_196F);
/// The Undead male body with a flaming skull, whose flame pictures are additive with an alpha.
const BURNING: DataId = DataId(0x0200_1A9C);
/// The Empyrean female body, whose eyes glow.
const GLOWING: DataId = DataId(0x0200_1A10);

#[test]
fn a_preview_space_draws_its_bodies_particles() {
    let (with, without, _) = particle_pictures(SMOKING);
    let covered = |px: &[[u8; 4]]| px.iter().filter(|p| p.iter().any(|c| *c > 0)).count();
    let (with, without) = (covered(&with), covered(&without));
    eprintln!("the particles cover {with} pixels");
    assert_eq!(without, 0);
    assert!(with > 20, "the particles show: {with} pixels");
}

#[test]
fn an_undead_flame_is_a_body_of_fire_in_a_preview() {
    let (with, _, made) = particle_pictures(BURNING);
    assert!(
        !made.fire.is_empty(),
        "the flame is drawn as a body of fire"
    );
    assert!(!made.lamps.is_empty(), "the fire carries its lamp");
    assert_added(&with, "flame", 20);
}

#[test]
fn an_empyreans_eyes_are_creatures_light_in_a_preview() {
    let (with, _, made) = particle_pictures(GLOWING);
    assert!(
        made.draws.iter().all(|d| d.glow.is_some()),
        "the eyes are drawn as light"
    );
    assert_added(&with, "eyes", 2);
}

/// `picture` shows `what` as light added over more than `least` pixels, hiding none behind it.
fn assert_added(picture: &[[u8; 4]], what: &str, least: usize) {
    let lit = picture
        .iter()
        .filter(|p| p[..3].iter().any(|c| *c > 8))
        .count();
    let covering = picture.iter().filter(|p| p[3] > 0).count();
    eprintln!("the {what} light {lit} pixels and cover {covering}");
    assert!(lit > least, "the {what} show: {lit} pixels");
    assert_eq!(covering, 0, "the {what} hide nothing behind them");
}

/// Setup `setup`'s particles two seconds into its default script, drawn by a preview pass; the
/// pass's picture with nothing drawn; and the particles as the preview took them.
fn particle_pictures(
    setup: DataId,
) -> (
    Vec<[u8; 4]>,
    Vec<[u8; 4]>,
    pyreal_engine::preview::PreviewParticles,
) {
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let mut engine = super::engine();
    let mut loader = AssetLoader::default();
    engine.particles.assets = Some(Arc::new(
        pyreal_world_data::anim_assets::DatAnimAssets::new(Arc::clone(&store)),
    ));
    for g in engine.particles.setup_sprites(setup) {
        pyreal_engine::particles::prepare_sprite(&mut engine, &mut loader, &store, g);
    }
    assert!(engine.particles.spawn(setup, Frame::default(), 1.0, 0, 1));
    let eye = Vec3::new(0.0, -4.0, 1.2);
    let mut now = 0.0;
    for _ in 0..120 {
        now += 1.0 / 60.0;
        engine.particles.step(now, eye);
    }
    let hosts = std::mem::take(&mut engine.particles.hosts);
    let made = pyreal_engine::preview::particles(&mut engine.particles, &hosts[0].driver, eye);
    engine.particles.hosts = hosts;
    assert!(
        !made.draws.is_empty() || !made.fire.is_empty(),
        "the body gives off particles"
    );
    engine.upload().expect("the pools");
    let mut pass = PreviewPass::new(&engine.gpu, &engine.materials);
    let view = PreviewView {
        view_proj: Mat4::perspective_rh(0.9, 1.0, 0.05, 100.0)
            * Mat4::look_at_rh(eye, Vec3::new(0.0, 0.0, 1.0), Vec3::Z),
        light_dir: Vec3::new(-0.3, -1.0, 0.6).normalize(),
        light: Vec3::splat(0.6),
        ambient: Vec3::splat(0.4),
        time: 2.0,
        lamps: made.lamps.clone(),
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
    let mut picture = |draws: &[_], fire: &[_]| {
        pass.draw(
            &engine.gpu,
            &engine.scene,
            &engine.materials,
            &view,
            draws,
            fire,
            &target,
        );
        pyreal_engine::read_texture(&engine.gpu, &target, 4)
            .as_chunks::<4>()
            .0
            .to_vec()
    };
    (picture(&made.draws, &made.fire), picture(&[], &[]), made)
}
