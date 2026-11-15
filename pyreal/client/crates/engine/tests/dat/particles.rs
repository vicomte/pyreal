//! A scripted object's particles step as the client's scene steps them.

use std::sync::Arc;

use pyreal_animation::data::AnimAssets;
use pyreal_animation::MotionDriver;
use pyreal_dat::DbType;
use pyreal_engine::cells::OUTSIDE;
use pyreal_engine::particles::{script_sprites, Particles};
use pyreal_primitives::{CellId, DataId, Frame, Position, Quat, ServerTime, Vec3};
use pyreal_world_data::anim_assets::DatAnimAssets;

/// The first setup (by id) whose default script starts emitters that never stop: a torch's, a
/// brazier's flame.
fn scripted_setup(assets: &dyn AnimAssets, store: &pyreal_dat::RetailDatStore) -> DataId {
    store
        .ids_of(DbType::Setup)
        .into_iter()
        .find(|&id| {
            assets
                .setup(id)
                .and_then(|s| s.default_script)
                .is_some_and(|s| !script_sprites(assets, s).is_empty())
        })
        .expect("a setup whose default script starts emitters")
}

/// A scripted static, stepped by the engine's particles over three seconds of frames, holds the
/// same particles (place, scale and translucency, slot by slot) as a motion driver stepped the
/// way the client's scene steps a scripted static (`EmitterHost`: parts placed, scripts run,
/// particles updated, ramps advanced); and its particles change from frame to frame.
#[test]
fn a_scripted_objects_particles_step_as_the_clients_do() {
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let assets: Arc<dyn AnimAssets> = Arc::new(DatAnimAssets::new(Arc::clone(&store)));
    let setup = scripted_setup(&*assets, &store);
    let frame = Frame::new(Vec3::new(30.0, 40.0, 12.0), Quat::IDENTITY);

    let mut engine = Particles {
        assets: Some(Arc::clone(&assets)),
        ..Particles::default()
    };
    assert!(engine.spawn(setup, frame, 1.0, OUTSIDE, 7), "{setup}");

    let mut client = MotionDriver::new(Arc::clone(&assets));
    assert!(client.set_setup(assets.setup(setup).expect("the setup")));
    client.env.in_cell = true;
    client.env.position = Position::new(CellId(0), frame);
    client.update_parts(&frame);

    let eye = glam::Vec3::new(31.0, 41.0, 13.0);
    let mut seen: Vec<Vec<(f32, f32, f32, f32)>> = Vec::new();
    for k in 0..90 {
        let now = 1000.0 + f64::from(k) / 30.0;
        engine.step(now, eye);
        client.cur_time = ServerTime(now);
        client.update_parts(&frame);
        client.update_scripts();
        client.update_particles(true);
        client.update_fp_hooks();

        let host = &engine.hosts[0].driver.particles;
        assert_eq!(host.len(), client.particles.len(), "emitters at frame {k}");
        for (a, b) in host.iter().zip(client.particles.iter()) {
            assert_eq!(a.particles, b.particles, "particles at frame {k}");
        }
        seen.push(
            client
                .particles
                .iter()
                .flat_map(|e| e.live())
                .map(|p| {
                    (
                        p.frame.origin.x,
                        p.frame.origin.y,
                        p.frame.origin.z,
                        p.scale,
                    )
                })
                .collect(),
        );
    }
    let live: usize = client.particles.iter().map(|e| e.live().count()).sum();
    assert!(live > 0, "{setup}: no live particles");
    assert!(
        seen.windows(2).filter(|w| w[0] != w[1]).count() > 30,
        "{setup}: its particles hardly changed"
    );
}

/// A server's portal (setup 0x020001B3, Holtburg's) is its default script's swirl, its one part's
/// picture wholly clipped away: stepped as a moving object's particles are, the swirl lives.
#[test]
fn a_portals_swirl_lives_when_stepped_as_a_moving_object() {
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let assets: Arc<dyn AnimAssets> = Arc::new(DatAnimAssets::new(Arc::clone(&store)));
    let engine = Particles {
        assets: Some(Arc::clone(&assets)),
        ..Particles::default()
    };
    let frame = Frame::new(Vec3::new(30.0, 40.0, 12.0), Quat::IDENTITY);
    let mut portal = MotionDriver::new(Arc::clone(&assets));
    assert!(portal.set_setup(assets.setup(DataId(0x0200_01B3)).expect("the portal")));
    portal.env.in_cell = true;
    portal.env.position = Position::new(CellId(0), frame);
    let at = glam::Vec3::new(30.0, 40.0, 12.0);
    let eye = glam::Vec3::new(30.0, 30.0, 13.0);
    for k in 0..90 {
        portal.cur_time = ServerTime(1000.0 + f64::from(k) / 30.0);
        portal.update_parts(&frame);
        portal.update_scripts();
        engine.step_object(&mut portal, at, eye);
    }
    let live: usize = portal.particles.iter().map(|e| e.live().count()).sum();
    assert!(live >= 6, "the swirl's particles: {live}");
}

/// A brazier's and a tower top's beds of fire (retail's billowing fire puffs) are drawn as their
/// puffs, not as flame cards, while a street torch keeps its drawn flame; the puff picture is
/// fire and a lamp's soft glow picture is not, so a glow's emitters keep their retail look.
#[test]
fn beds_of_fire_burn_as_puffs_and_torches_as_flames() {
    use pyreal_engine::flames::examine;
    let store = pyreal_dat::testing::open_store_or_fail();
    for bed in [0x0200_0624, 0x0200_0485, 0x0200_05AE] {
        let flames = examine(&store, DataId(bed));
        assert!(!flames.is_empty(), "{bed:#X} burns");
        assert!(
            flames.iter().all(|f| !f.kind.carded()),
            "{bed:#X}: {flames:?}"
        );
    }
    let torch = examine(&store, DataId(0x0200_05D8));
    assert!(
        !torch.is_empty() && torch.iter().all(|f| f.kind.carded()),
        "{torch:?}"
    );
    let fire = |g: u32| pyreal_engine::body_fire::fire_picture(&store, DataId(g));
    assert!(fire(0x0100_0FF4).is_some(), "the fire puff is fire");
    assert!(fire(0x0100_16FD).is_none(), "a lamp's glow is not fire");
}
