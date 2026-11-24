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

/// Broad glows are not flames: the dungeons' warm haze 0x02000694 (an invisible marker whose glow
/// swells to metres as it drifts), the swirling orbs 0x02000BF3 and 0x02000C1B and the floor's
/// sheet of light 0x020007EA burn nowhere, and a glow with no light of its own lights steadily.
/// A wall torch keeps its flame and flickering light, and a fire bed's puffs stay fire.
#[test]
fn broad_glows_burn_nowhere_and_light_steadily() {
    use pyreal_engine::flames;
    use pyreal_engine::lamps::{self, Flicker};
    let store = pyreal_dat::testing::open_store_or_fail();
    for glow in [0x0200_0694, 0x0200_0BF3, 0x0200_0C1B, 0x0200_07EA] {
        assert!(
            flames::examine(&store, DataId(glow)).is_empty(),
            "{glow:#X}"
        );
    }
    let haze = lamps::examine(&store, DataId(0x0200_0694));
    assert!(
        !haze.is_empty() && haze.iter().all(|l| l.flicker == Flicker::Steady),
        "{haze:?}"
    );
    let torch = DataId(0x0200_039C);
    assert!(flames::examine(&store, torch)
        .iter()
        .any(|f| f.kind.carded()));
    let lit = lamps::examine(&store, torch);
    assert!(lit.iter().any(|l| l.flicker == Flicker::Flame), "{lit:?}");
    assert!(
        !flames::examine(&store, DataId(0x0200_16F9)).is_empty(),
        "a fire bed burns"
    );
}

/// A hearth's bed of fire (the arrival hall's fireplace 0x0200034D in the Training Academy, and
/// its twin 0x02000344) burns as a body of fire on its coals: two rows of flames across its bed,
/// the middle ones tallest, flickering with its lamp, which stands for its light. The same emitter
/// on a campfire or tower top (0x02000485, which burns as a forge's fire only placed in a forge's
/// mouth), the braziers, the hanging coal baskets 0x0200034C and 0x02000346, the lava shrine
/// 0x020007CB and the volcano's fire 0x02000840 stay beds of fire puffs.
#[test]
fn hearths_burn_as_bodies_of_fire_and_other_beds_as_puffs() {
    use pyreal_engine::body_fire;
    use pyreal_engine::flames::{examine, FlameKind};
    let store = pyreal_dat::testing::open_store_or_fail();
    for hearth in [0x0200_034D, 0x0200_0344] {
        let flames = examine(&store, DataId(hearth));
        assert!(
            flames.len() == 1 && flames[0].kind == FlameKind::Hearth,
            "{hearth:#X}: {flames:?}"
        );
        let s = flames[0];
        assert!(!s.kind.carded(), "{hearth:#X} has no flame card");
        assert!((s.bed.x - 0.6).abs() < 1e-3, "its bed spreads 0.6 m: {s:?}");
        assert!(s.bed.y > 0.2 && (0.8..1.4).contains(&s.bed.z), "{s:?}");
        assert!(
            s.tint.x >= s.tint.y && s.tint.y >= s.tint.z,
            "orange: {s:?}"
        );
        let fire = body_fire::hearth(&s, &glam::Affine3A::IDENTITY, 3, 7, 42, 12.0);
        assert_eq!(
            (fire.hearth, fire.seed, fire.object, fire.cell),
            (Some(12.0), 42, 7, 3)
        );
        assert_eq!(fire.points.len(), 9);
        for p in &fire.points {
            assert!(p.at.z < s.offset.z, "its flames stand on its coals: {p:?}");
            assert!((p.at - s.offset).truncate().length() <= s.bed.x, "{p:?}");
        }
        let tallest = fire.points.iter().map(|p| p.height).fold(0.0, f32::max);
        let ends = fire.points[0].height.max(fire.points[4].height);
        assert!(tallest > ends, "its middle licks highest");
        // Drawn as a hearth's body, which brings no lamp: its own lamp lights the room.
        let (records, lamps) = body_fire::frame(
            std::slice::from_ref(&fire),
            &[],
            glam::Vec3::new(0.0, -4.0, 1.0),
            |_| true,
            64,
        );
        assert_eq!(records[0].kind, FlameKind::Hearth);
        assert_eq!(
            (records[0].count, records[0].spin, records[0].seed),
            (9, 12, 42)
        );
        assert!(lamps.is_empty(), "{lamps:?}");
    }
    for bed in [
        0x0200_0485,
        0x0200_0624,
        0x0200_05AE,
        0x0200_034C,
        0x0200_0346,
        0x0200_07CB,
        0x0200_0840,
    ] {
        let flames = examine(&store, DataId(bed));
        assert!(
            flames.iter().any(|f| f.kind == FlameKind::Brazier)
                && flames.iter().all(|f| f.kind != FlameKind::Hearth),
            "{bed:#X}: {flames:?}"
        );
    }
}

/// A hearth's fire throws its own warm light from its coals onto its firebox, hearthstones and the
/// floor before it: a share of its lamp's, reaching a few metres and flickering on its seed.
#[test]
fn a_hearths_coals_light_round_it() {
    use pyreal_engine::lamps::{Flicker, LampKind};
    let store = pyreal_dat::testing::open_store_or_fail();
    let s = pyreal_engine::flames::examine(&store, DataId(0x0200_034D))[0];
    let glow = pyreal_engine::body_fire::hearth_lamp(
        &s,
        &glam::Affine3A::IDENTITY,
        3,
        7,
        (42, 12.0, 100.0),
    );
    assert_eq!(
        (glow.kind, glow.flicker, glow.seed),
        (LampKind::Flame, Flicker::Flame, 42)
    );
    assert_eq!((glow.cell, glow.object), (3, 7));
    assert!((glow.range - 4.5).abs() < 1e-3, "{glow:?}");
    assert!(
        (glow.retail.truncate().max_element() - 30.0).abs() < 1e-3,
        "a share of its lamp's: {glow:?}"
    );
    assert!(glow.position.z < s.offset.z && glow.position.z > s.offset.z - s.bed.y);
    assert!(
        glow.colour.x > glow.colour.y && glow.colour.y > glow.colour.z,
        "{glow:?}"
    );
}

/// A torch's flame of another colour than fire's (retail's torch flame recoloured) burns as a
/// flame of its colour: the Training Academy's blue pole torch 0x020009A3, the dungeons' blue
/// torches 0x020011F8 and 0x020005C1, the green 0x020005C0 and the violet 0x02001A7D, each
/// flickering with its lamp. Fire's own torches carry no colour, and a lamp's blue glow and the
/// fire puff are not torch flames.
#[test]
fn coloured_torches_burn_as_flames_of_their_colour() {
    use pyreal_engine::flames::{self, FlameKind};
    use pyreal_engine::lamps::{self, Flicker};
    let store = pyreal_dat::testing::open_store_or_fail();
    let blue = |c: glam::Vec3| c.z > 0.99 && c.x < 0.5 && c.y < 0.6;
    let green = |c: glam::Vec3| c.y > 0.99 && c.x < 0.5 && c.z < 0.6;
    let violet = |c: glam::Vec3| c.z > 0.99 && c.x > c.y;
    let coloured: [(u32, &dyn Fn(glam::Vec3) -> bool); 5] = [
        (0x0200_09A3, &blue),
        (0x0200_11F8, &blue),
        (0x0200_05C1, &blue),
        (0x0200_05C0, &green),
        (0x0200_1A7D, &violet),
    ];
    for (id, hue) in coloured {
        let f = flames::examine(&store, DataId(id));
        assert_eq!(f.len(), 1, "{id:#X}: {f:?}");
        assert_eq!(f[0].kind, FlameKind::Torch, "{id:#X}");
        assert!(hue(f[0].tint), "{id:#X}: {:?}", f[0].tint);
        let lit = lamps::examine(&store, DataId(id));
        assert!(
            lit.iter().any(|l| l.flicker == Flicker::Flame),
            "{id:#X}: {lit:?}"
        );
        let placed = flames::place(&f[0], &glam::Affine3A::IDENTITY, 0, id, &[]);
        assert_ne!(placed.colour, 0, "{id:#X}");
        let bytes = flames::flame_bytes(&placed, true);
        assert_eq!(bytes[28..32], placed.colour.to_le_bytes(), "{id:#X}");
    }
    for id in [
        0x0200_05D8,
        0x0200_026E,
        0x0200_01E5,
        0x0200_0DB6,
        0x0200_039C,
    ] {
        let f = flames::examine(&store, DataId(id));
        assert!(!f.is_empty(), "{id:#X}");
        for s in &f {
            assert_eq!(s.tint, glam::Vec3::ZERO, "{id:#X}: fire's own");
            let placed = flames::place(s, &glam::Affine3A::IDENTITY, 0, id, &[]);
            assert_eq!(placed.colour, 0, "{id:#X}");
        }
    }
    let torch = |g: u32| pyreal_engine::body_fire::torch_picture(&store, DataId(g));
    assert!(torch(0x0100_174C).is_some(), "the blue torch flame");
    assert!(torch(0x0100_1749).is_none(), "a lamp's blue glow");
    assert!(torch(0x0100_0FF4).is_none(), "the fire puff");
}

/// A torch whose flame retail grows larger than its usual torch flame burns taller: the Training
/// Academy's blue pole torch 0x020009A3 (to 5.4 of the usual 3) and the tall torch 0x02000AA0 (to
/// 7.5), less than in proportion since retail's puffs fade as they grow. Other torches, candles
/// and beds of fire stand as they did.
#[test]
fn a_torch_burns_as_tall_as_retail_grows_its_flame() {
    use pyreal_engine::flames;
    let store = pyreal_dat::testing::open_store_or_fail();
    let grow = |id: u32| {
        let f = flames::examine(&store, DataId(id));
        assert!(!f.is_empty(), "{id:#X}");
        f[0].grow
    };
    assert!((grow(0x0200_09A3) - 1.8_f32.sqrt()).abs() < 1e-3);
    assert!((grow(0x0200_0AA0) - 2.5_f32.sqrt()).abs() < 1e-3);
    for id in [
        0x0200_11F8,
        0x0200_05C0,
        0x0200_05D8,
        0x0200_026E,
        0x0200_0DB6,
        0x0200_0B8D,
        0x0200_0624,
        0x0200_034D,
    ] {
        assert!((grow(id) - 1.0).abs() < 1e-6, "{id:#X}");
    }
    let s = flames::examine(&store, DataId(0x0200_09A3))[0];
    let placed = flames::place(&s, &glam::Affine3A::IDENTITY, 0, 0, &[]);
    assert!(
        (placed.height - 0.32 * 1.8_f32.sqrt()).abs() < 1e-3,
        "{placed:?}"
    );
}

/// The objects room `cell` places, each where (its block's space).
fn cell_objects(store: &pyreal_dat::RetailDatStore, cell: u32) -> Vec<(u32, glam::Affine3A)> {
    use pyreal_assets::Decode;
    let id = DataId(cell);
    let bytes = store.read_typed(DbType::Cell, id).expect("the cell");
    let room = pyreal_assets::world::EnvCell::decode_payload_in(store.era_of(id), id, &bytes)
        .expect("a room");
    room.static_objects
        .iter()
        .map(|o| {
            let (r, p) = (o.frame.rotation, o.frame.origin);
            let at = glam::Affine3A::from_rotation_translation(
                glam::Quat::from_xyzw(r.x, r.y, r.z, r.w),
                glam::Vec3::new(p.x, p.y, p.z),
            );
            (o.id.0, at)
        })
        .collect()
}

/// The Training Academy smithy's forge 0x0200033C (cell 0x7F0301F6) burns the bed of fire placed
/// in its mouth (0x02000485, a campfire's) as a body of fire shaped to its firebox: rows of flames
/// across its coals, inside its walls, tallest at its mouth where its roof is highest, flickering
/// with the fire's lamp, and its coals light its firebox and the floor before it. A cauldron's
/// tripod (0x02000345) is no forge, and the growing fire puffs 0x02001681 in a forge stay puffs.
#[test]
fn a_forges_fire_burns_in_its_firebox() {
    use pyreal_engine::body_fire;
    use pyreal_engine::flames::{examine, forge_of, FlameKind, FORGES};
    use pyreal_engine::lamps::{Flicker, LampKind};
    let store = pyreal_dat::testing::open_store_or_fail();
    let objects = cell_objects(&store, 0x7F03_01F6);
    let find = |id: u32| objects.iter().find(|(o, _)| *o == id).expect("placed").1;
    let (fire_at, forge_at) = (find(0x0200_0485), find(0x0200_033C));
    let s = examine(&store, DataId(0x0200_0485))[0];
    assert_eq!(s.kind, FlameKind::Brazier, "a bed of fire puffs on its own");
    let forges = [(0x0200_033C, forge_at)];
    let (forge, there) = forge_of(&s, &fire_at, &forges).expect("it burns in the forge");
    assert_eq!((forge.setup, there), (0x0200_033C, forge_at));
    let fire = body_fire::forge(&s, forge, &there, 3, 7, 42, 12.0);
    assert_eq!(
        (fire.hearth, fire.seed, fire.object, fire.cell),
        (Some(12.0), 42, 7, 3)
    );
    let inside = there.inverse();
    let run = (forge.mouth - forge.back).truncate();
    for p in &fire.points {
        let q = inside.transform_point3(p.at);
        let ahead = (q - forge.back).truncate().dot(run.normalize());
        assert!(
            (0.0..run.length()).contains(&ahead) && q.x.abs() + p.radius < forge.mouth_half,
            "in its firebox: {q:?}"
        );
        assert!((q.z - forge.mouth.z).abs() < 0.1, "on its coals: {q:?}");
        // Up into its roof, which hides their tips, and no further.
        assert!(p.height < forge.mouth_roof * 2.0, "{p:?}");
    }
    let front = fire.points.first().expect("flames").height;
    let back = fire.points.last().expect("flames").height;
    assert!(front > back, "it licks highest at its mouth");
    let (records, lamps) = body_fire::frame(
        std::slice::from_ref(&fire),
        &[],
        there.transform_point3(glam::Vec3::new(0.0, -4.0, 1.5)),
        |_| true,
        64,
    );
    assert_eq!(
        records[0].kind,
        FlameKind::Hearth,
        "drawn as a hearth's fire"
    );
    assert!(lamps.is_empty(), "{lamps:?}");
    let glow = body_fire::forge_lamp(forge, &there, 3, 7, 42);
    assert_eq!(
        (glow.kind, glow.flicker, glow.seed, glow.cell, glow.object),
        (LampKind::Flame, Flicker::Flame, 42, 3, 7)
    );
    let g = inside.transform_point3(glow.position);
    assert!(
        (forge.back.y.min(forge.mouth.y)..forge.back.y.max(forge.mouth.y)).contains(&g.y)
            && g.z > forge.mouth.z,
        "over its coals: {g:?}"
    );
    // A cauldron's tripod is no forge, and puffs that grow are no bed on coals.
    let tripod = cell_objects(&store, 0x0144_0139);
    let near = |id: u32| tripod.iter().find(|(o, _)| *o == id).expect("placed").1;
    assert!(forge_of(&s, &near(0x0200_0485), &[(0x0200_0345, near(0x0200_0345))]).is_none());
    let forge_cell = cell_objects(&store, 0x005F_0185);
    let at = |id: u32| forge_cell.iter().find(|(o, _)| *o == id).expect("placed").1;
    let billows = examine(&store, DataId(0x0200_1681))[0];
    assert!(forge_of(
        &billows,
        &at(0x0200_1681),
        &[(0x0200_033C, at(0x0200_033C))]
    )
    .is_none());
    assert!(FORGES.iter().all(|f| f.setup != 0x0200_0345));
}

/// The Training Academy kitchen's fire pit 0x02000354 (cell 0x7F03032A), a ring of stones round a
/// mound of coals under a cauldron's tripod, burns the bed of fire placed on it (0x02000485) as a
/// body of fire on its coals: a flame in the middle and a ring round it, the middle tallest, drawn
/// as a hearth's, and its coals light round it. So does one standing at a pit's edge (0x01440139).
#[test]
fn a_fire_pits_fire_burns_on_its_coals() {
    use pyreal_engine::body_fire;
    use pyreal_engine::flames::{examine, firebox_of, holds_fire, Firebox, FlameKind};
    use pyreal_engine::lamps::{Flicker, LampKind};
    let store = pyreal_dat::testing::open_store_or_fail();
    let s = examine(&store, DataId(0x0200_0485))[0];
    for cell in [0x7F03_032A, 0x0144_0139] {
        let objects = cell_objects(&store, cell);
        let find = |id: u32| objects.iter().find(|(o, _)| *o == id).expect("placed").1;
        let holders: Vec<_> = objects
            .iter()
            .filter(|(id, _)| holds_fire(*id))
            .copied()
            .collect();
        let pit_at = find(0x0200_0354);
        let (b, there) = firebox_of(&s, &find(0x0200_0485), &holders).expect("it burns on the pit");
        let Firebox::Pit(pit) = b else {
            panic!("{cell:#X}: a pit's, not {b:?}");
        };
        assert_eq!((pit.setup, there), (0x0200_0354, pit_at));
        let (fire, glow) = body_fire::in_firebox(&s, b, &there, 3, 7, (42, 12.0));
        assert_eq!(
            (fire.hearth, fire.seed, fire.object, fire.cell),
            (Some(12.0), 42, 7, 3)
        );
        let inside = there.inverse();
        for p in &fire.points {
            let q = inside.transform_point3(p.at) - pit.coals;
            assert!(
                q.truncate().length() <= pit.radius && p.radius < pit.radius,
                "on its coals: {q:?}"
            );
            assert!(q.z <= 0.0 && q.z > -0.1, "its feet in its coals: {q:?}");
        }
        let middle = fire.points[0].height;
        assert!(
            fire.points[1..].iter().all(|p| p.height < middle),
            "its middle licks highest"
        );
        let (records, _) = body_fire::frame(
            std::slice::from_ref(&fire),
            &[],
            there.transform_point3(glam::Vec3::new(0.0, -3.0, 1.5)),
            |_| true,
            64,
        );
        assert_eq!(records[0].kind, FlameKind::Hearth);
        assert_eq!(
            (glow.kind, glow.flicker, glow.seed),
            (LampKind::Flame, Flicker::Flame, 42)
        );
        assert!(
            inside.transform_point3(glow.position).z > pit.coals.z,
            "over its coals"
        );
    }
    // A cauldron's tripod alone is no firebox.
    assert!(!holds_fire(0x0200_0345));
}

/// A dungeon's steam vent 0x02000AB4 swarms its puffs about a slow rise (retail's swarm swings
/// them by `b` and `c` about `a`'s path), so its steam is steam, not a frost elemental's sinking
/// snow; drawn as thin as retail's puffs, born three quarters clear. Its room's purple haze
/// 0x02000695 is a haze, and a torch's flame is neither.
#[test]
fn a_vents_steam_rises_thinly_and_a_rooms_haze_is_haze() {
    use pyreal_engine::puffs::{placed_kind, vent_strength, HAZE, STEAM};
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let assets = DatAnimAssets::new(Arc::clone(&store));
    let kind = |emitter: u32, picture: u32| {
        let e = assets.emitter_info(DataId(emitter)).expect("the emitter");
        (placed_kind(&store, DataId(picture), &e), e)
    };
    let (steam, vent) = kind(0x3200_01AE, 0x0100_0FC3);
    assert_eq!(steam, Some(STEAM), "the vent's steam");
    let thin = vent_strength(&vent);
    assert!((thin - 0.25).abs() < 1e-3, "{thin}");
    let (haze, _) = kind(0x3200_0406, 0x0100_192E);
    assert_eq!(haze, Some(HAZE), "the purple haze");
    let (torch, _) = kind(0x3200_002F, 0x0100_1689);
    assert_eq!(torch, None, "a torch's flame");
}

/// Every emitter blowing the dungeons' green gas (picture 0x01000FF9: the vents 0x02000AB3 and
/// 0x02000AB4, 0x0200028D's, the gas pits 0x02000994) is drawn as gas, and no other laid-over
/// puffs are: a coal basket's smoke, a brown dust, a waterfall's spray.
#[test]
fn a_vents_green_gas_is_gas_and_other_laid_over_puffs_are_not() {
    use pyreal_engine::puffs::{placed_kind, GAS, STEAM};
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let assets = DatAnimAssets::new(Arc::clone(&store));
    let kind = |emitter: u32, picture: u32| {
        let e = assets.emitter_info(DataId(emitter)).expect("the emitter");
        placed_kind(&store, DataId(picture), &e)
    };
    for e in [
        0x3200_0010,
        0x3200_01AB,
        0x3200_01AC,
        0x3200_01AD,
        0x3200_055A,
        0x3200_074B,
        0x3200_074C,
        0x3200_0760,
    ] {
        assert_eq!(kind(e, 0x0100_0FF9), Some(GAS), "{e:#010X}");
    }
    assert_eq!(
        kind(0x3200_01AE, 0x0100_0FC3),
        Some(STEAM),
        "the vent's steam"
    );
    assert_eq!(
        kind(0x3200_01B0, 0x0100_0FBF),
        None,
        "a coal basket's smoke"
    );
    assert_eq!(kind(0x3200_021D, 0x0100_1318), None, "brown dust");
    assert_eq!(kind(0x3200_04A2, 0x0100_109E), None, "a waterfall's spray");
}

/// A swamp vent's rising pictures (0x02000584 pink, 0x02000585 green, 0x02000586 brown) are its
/// wisps, and no other emitter in the dats is; its puffs at the mouth stay pictures. No vent draws
/// a flame, and the brown one's lamps glow at a tenth of a fire's.
#[test]
fn a_swamp_vents_rising_pictures_are_wisps_and_glow_faintly() {
    use pyreal_engine::puffs::{placed_kind, WISP};
    use pyreal_engine::{flames, lamps};
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let assets = DatAnimAssets::new(Arc::clone(&store));
    let mut wisps = Vec::new();
    for id in store.ids_of(DbType::ParticleEmitter) {
        let Some(e) = assets.emitter_info(id) else {
            continue;
        };
        let gfx = if e.hw_gfxobj_id.0 != 0 {
            e.hw_gfxobj_id
        } else {
            e.gfxobj_id
        };
        if placed_kind(&store, gfx, &e) == Some(WISP) {
            wisps.push(id.0);
        }
    }
    assert_eq!(wisps, [0x3200_02D0, 0x3200_02D1, 0x3200_02D2]);
    for (e, gfx) in [
        (0x3200_02A0, 0x0100_162B),
        (0x3200_02A2, 0x0100_1626),
        (0x3200_02A3, 0x0100_1627),
    ] {
        let info = assets.emitter_info(DataId(e)).expect("the emitter");
        assert_eq!(placed_kind(&store, DataId(gfx), &info), None, "{e:#010X}");
    }
    for vent in [0x0200_0584, 0x0200_0585, 0x0200_0586] {
        let vent = DataId(vent);
        assert!(
            flames::examine(&store, vent).is_empty(),
            "{vent:?}: a flame"
        );
        let lit = lamps::examine(&store, vent);
        if vent.0 == 0x0200_0586 {
            assert_eq!(lit.len(), 2, "the brown vent's two fire-coloured emitters");
            assert!(lit.iter().all(|l| l.kind == lamps::LampKind::Vent
                && (l.intensity - lamps::VENT_LIGHT).abs() < 1e-6));
        } else {
            assert!(lit.is_empty(), "{vent:?}: a lamp");
        }
    }
}

/// A floor grate 0x02000469's brown haze (emitter 0x3200021C, picture 0x01001318) is a fume, not
/// fire: it is drawn as a volume and makes no flame and no lamp. The same picture's dust
/// 0x3200021D and the vents' gas and steam are not fume.
#[test]
fn a_floor_grates_brown_haze_is_fume_not_fire() {
    use pyreal_engine::puffs::{placed_kind, FUME, GAS, STEAM};
    use pyreal_engine::{flames, lamps};
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let assets = DatAnimAssets::new(Arc::clone(&store));
    let kind = |emitter: u32, picture: u32| {
        let e = assets.emitter_info(DataId(emitter)).expect("the emitter");
        placed_kind(&store, DataId(picture), &e)
    };
    assert_eq!(
        kind(0x3200_021C, 0x0100_1318),
        Some(FUME),
        "the grate's haze"
    );
    assert_eq!(kind(0x3200_021D, 0x0100_1318), None, "brown dust");
    assert_eq!(kind(0x3200_01AC, 0x0100_0FF9), Some(GAS), "a vent's gas");
    assert_eq!(
        kind(0x3200_01AE, 0x0100_0FC3),
        Some(STEAM),
        "a vent's steam"
    );
    let grate = DataId(0x0200_0469);
    assert!(flames::examine(&store, grate).is_empty(), "no flame");
    assert!(lamps::examine(&store, grate).is_empty(), "no lamp");
}

/// A gas vent's jet 0x320001AB (and the jets 0x3200074B and 0x3200074C) falls as it swells, so it
/// is heavy gas that pours down and pools; the vent's rising haze 0x320001AC, its kin 0x32000010
/// and 0x320001AD, and the gas pits' clouds 0x3200055A and 0x32000760 do not pool.
#[test]
fn a_vents_falling_gas_pools_and_its_haze_and_a_pits_cloud_do_not() {
    use pyreal_engine::puffs::gas_pools;
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let assets = DatAnimAssets::new(Arc::clone(&store));
    let pools =
        |emitter: u32| gas_pools(&assets.emitter_info(DataId(emitter)).expect("the emitter"));
    for e in [0x3200_01AB, 0x3200_074B, 0x3200_074C] {
        assert!(pools(e), "{e:#010X} pools");
    }
    for e in [
        0x3200_01AC,
        0x3200_0010,
        0x3200_01AD,
        0x3200_055A,
        0x3200_0760,
    ] {
        assert!(!pools(e), "{e:#010X} does not pool");
    }
}

/// A geyser 0x020003CB throws its steam up from its foot (retail's explode type, about nine metres
/// a second), so it is a jet: steam, not snow, whole as it leaves the vent and thinning to a veil
/// as it spreads, its strength falling as the square of retail's fading opacity; its spray round
/// its foot 0x32000012 is drawn in its volume. The vent's slow steam and the lightning pillar
/// 0x02000C27 (retail's other fast type) are not jets, and a vent's strength is as it was.
#[test]
fn a_geysers_steam_is_a_jet_whole_at_its_foot_and_a_veil_above() {
    use pyreal_engine::puffs::{
        is_jet, is_spray, jet_share, placed_alpha, placed_kind, swell, JET_SHARE, STEAM,
    };
    let store = Arc::new(pyreal_dat::testing::open_store_or_fail());
    let assets = DatAnimAssets::new(Arc::clone(&store));
    let emitter = |id: u32| assets.emitter_info(DataId(id)).expect("the emitter");
    let picture = DataId(0x0100_0FC3);
    let jet = emitter(0x3200_0011);
    assert_eq!(
        placed_kind(&store, picture, &jet),
        Some(STEAM),
        "the geyser's steam"
    );
    assert!(is_jet(&jet));
    let spray = emitter(0x3200_0012);
    let tone = pyreal_engine::lamps::tone(&store, picture);
    assert!(!is_jet(&spray) && is_spray(tone, true, &spray), "its spray");
    let vent = emitter(0x3200_01AE);
    assert!(!is_jet(&vent) && !is_jet(&emitter(0x3200_06A8)));

    assert!(
        placed_alpha(&jet, 1.0, 0.03, false) > 0.95,
        "whole as it leaves the vent"
    );
    assert!((placed_alpha(&jet, 0.5, 0.5, false) - 0.25).abs() < 1e-3);
    assert!((jet_share(0.0) - 1.0).abs() < 1e-6 && (jet_share(0.6) - JET_SHARE).abs() < 1e-6);
    let v = placed_alpha(&vent, 0.8, 0.5, false);
    assert!(
        (v - 0.8 * 0.25 * swell(0.5)).abs() < 1e-6,
        "a vent's as it was: {v}"
    );
}

/// Redspire's fire, 0x02000B22 (two stacked on its spire, both eras): retail's fire puffs swarming
/// round the spire 1.5 m out, drawn as one band of fire whirling round it, which lights round it;
/// other beds of fire and hearths are not wreaths.
#[test]
fn redspires_fire_whirls_round_its_spire_as_a_wreath() {
    use pyreal_engine::body_fire;
    use pyreal_engine::flames::{examine, FlameKind};
    for store in [
        pyreal_dat::testing::open_store_or_fail(),
        pyreal_dat::testing::open_classic_store_or_fail(),
    ] {
        let flames = examine(&store, DataId(0x0200_0B22));
        assert!(
            flames.len() == 1 && flames[0].kind == FlameKind::Wreath,
            "{flames:?}"
        );
        let s = flames[0];
        assert!(!s.kind.carded() && s.kind.placed_body(), "{s:?}");
        assert!((s.bed.x - 1.5).abs() < 1e-3, "its ring is 1.5 m out: {s:?}");
        assert!(s.bed.y > 1.0 && (3.0..3.6).contains(&s.bed.z), "{s:?}");
        assert!(
            s.tint.x >= s.tint.y && s.tint.y >= s.tint.z,
            "orange: {s:?}"
        );
        let fire = body_fire::wreath(&s, &glam::Affine3A::IDENTITY, 0, 7, 42);
        assert_eq!(
            (fire.hearth, fire.seed, fire.object, fire.cell),
            (None, 42, 7, 0)
        );
        assert!(fire.spin > 0.0, "it whirls");
        for p in &fire.points {
            let out = (p.at - s.offset).truncate().length();
            assert!((out - s.bed.x).abs() < 1e-3, "on its ring: {p:?}");
            assert!(
                p.at.z < s.offset.z,
                "its band stands round its puffs: {p:?}"
            );
        }
        let (records, lamps) = body_fire::frame(
            std::slice::from_ref(&fire),
            &[],
            glam::Vec3::new(0.0, -8.0, 1.0),
            |_| true,
            64,
        );
        assert_eq!(records[0].kind, FlameKind::Wreath);
        assert!(records[0].spin > 0, "{:?}", records[0]);
        assert_eq!(lamps.len(), 1, "it brings its own light");
        for other in [0x0200_0485, 0x0200_0624, 0x0200_034D, 0x0200_05D8] {
            let flames = examine(&store, DataId(other));
            assert!(
                flames.iter().all(|f| f.kind != FlameKind::Wreath),
                "{other:#X}: {flames:?}"
            );
        }
    }
}
