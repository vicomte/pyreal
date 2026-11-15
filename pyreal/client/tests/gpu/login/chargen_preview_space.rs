//! The `CreatureMode` preview space as the teleport tunnel's portal swirl uses it. The portal ids
//! resolve as enum keys `0x10000001` and `0x10000002` through group **7, `UIASSET`**
//! (`portalspace_background`, `portalspace_animation`); the sequence frame counter is driven by
//! elapsed seconds, so the same duration reaches the same frame at any frame rate; and a login
//! builds the portal space the way portal initialization describes it.
//! Fixture: the retail dats, bare preview spaces on a WARP renderer, and an offline headless `App`
//! in Holtburg; no link.

#![cfg(gpu)]

use crate::common::client_dir;

use std::sync::Arc;

use pyreal_client::app::App;
use pyreal_client_contract::teleport::{in_exit_window, portal_space, timing};
use pyreal_client_runtime::config::Config;
use pyreal_client_shell::gpu::PreviewId;
use pyreal_primitives::{AssetSource as _, DataId};
use {
    pyreal_scene::preview::ENUM_PORTALSPACE_ANIMATION,
    pyreal_scene::preview::ENUM_PORTALSPACE_BACKGROUND, pyreal_scene::preview::UIASSET_GROUP,
};

/// The portal update's frame rate, as the preview takes it.
fn portal_framerate() -> f32 {
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: forty frames a second, exact in f32
    let rate = timing::PORTAL_FRAMERATE as f32;
    rate
}

/// Every fixture path is an `expect`: a missing dat or device fails the test.
fn store() -> Arc<pyreal_dat::RetailDatStore> {
    let d = client_dir();
    assert!(
        pyreal_dat::testing::have_dats(),
        "the retail dats are required at {} -- set PYREAL_TEST_DAT_DIR",
        d.display()
    );
    Arc::new(pyreal_client_runtime::assets::open_data_files(&d).expect("the retail dats open"))
}

// =================================================================================================
// 1. The two ids
// =================================================================================================

/// Oracle: the two enum-key lookups portal-preview initialization makes, and the shipped
/// `DidMapper` chain `0x25000000` -> group 7 -> `0x25000010`.
///
/// The values are asserted because they pin the *group*: 7 is the only one of the thirteen whose
/// `0x10000001` is a setup record (`0x02` id space) and whose `0x10000002` is an animation
/// (`0x03`), which is what object creation and sequence-animation setup consume. Group 5 answers
/// `0x21000000`, a `LayoutDesc`; group 9 answers a `Font`. Getting the group wrong makes the
/// portal space quietly draw nothing.
#[test]
fn the_portal_space_ids_resolve_through_the_uiasset_group() {
    let store = store();
    let assets: &dyn pyreal_primitives::AssetSource = &*store;

    let obj =
        pyreal_client_runtime::assets::enum_did(assets, UIASSET_GROUP, ENUM_PORTALSPACE_BACKGROUND)
            .expect("UIASSET 0x10000001 (portalspace_background) resolves");
    let anim =
        pyreal_client_runtime::assets::enum_did(assets, UIASSET_GROUP, ENUM_PORTALSPACE_ANIMATION)
            .expect("UIASSET 0x10000002 (portalspace_animation) resolves");

    assert_eq!(
        obj,
        DataId(0x0200_0306),
        "portalspace_background is a setup record"
    );
    assert_eq!(
        anim,
        DataId(0x0300_05AC),
        "portalspace_animation is an Animation"
    );
    assert_eq!(obj.0 >> 24, 0x02, "object creation wants a setup record");
    assert_eq!(
        anim.0 >> 24,
        0x03,
        "set_sequence_animation wants an Animation"
    );
    assert!(
        store.exists(obj) && store.exists(anim),
        "both are in this dat build"
    );

    // The paper-doll and char-gen animation enums resolve too, and to the right id spaces.
    for (name, e, space) in [
        (
            "PaperDollAnimation",
            pyreal_scene::preview::ENUM_PAPERDOLL_ANIMATION,
            0x03,
        ),
        (
            "CharGenAnimation",
            pyreal_scene::preview::ENUM_CHARGEN_ANIMATION,
            0x03,
        ),
    ] {
        let id = pyreal_client_runtime::assets::enum_did(assets, UIASSET_GROUP, e)
            .unwrap_or_else(|| panic!("UIASSET {e:#010X} ({name}) resolves"));
        assert_eq!(id.0 >> 24, space, "{name} is in the {space:#04X} id space");
    }
}

/// Oracle: portal initialization's camera-position and light-direction arguments, with the same
/// camera reapplied by its per-frame update.
///
/// The z is asserted as its **bit pattern** as well as its value: the update stores `0x3F6147AE`,
/// which is 0.88, not 0.87.
#[test]
fn the_portal_camera_constants_are_the_floats_in_post_init() {
    assert_eq!(portal_space::CAMERA_POSITION, (0.24, -2.7, 0.88));
    assert_eq!(
        portal_space::CAMERA_POSITION.2.to_bits(),
        0x3F61_47AE,
        "the camera-height bit pattern is 0x3F6147AE, which is 0.88 and not 0.87"
    );
    assert_eq!(portal_space::LIGHT_DIRECTION, (0.3, -1.9, 0.65));
    assert_eq!(portal_space::LIGHT_INTENSITY, 2.0);
    // The light added is `(DISTANT_LIGHT, 2.0)`; DISTANT_LIGHT is 1 in its enum.
    assert_eq!(
        pyreal_world_render::lighting::LightType::Directional as u32,
        1
    );
}

// =================================================================================================
// 2. The frame counter
// =================================================================================================

/// A preview space holding the portal object with its sequence started, exactly as portal
/// initialization and its per-frame update build it.
fn portal_renderer(store: &Arc<pyreal_dat::RetailDatStore>) -> pyreal_client_shell::gpu::Renderer {
    let mut r =
        pyreal_client_shell::gpu::Renderer::new(None, 64, 64).expect("a WARP device comes up");
    let assets = Arc::new(pyreal_world_data::anim_assets::DatAnimAssets::new(
        Arc::clone(store),
    ));
    assert!(
        r.ensure_preview(PreviewId::Portal, &assets),
        "the space is created once"
    );
    let a: &dyn pyreal_primitives::AssetSource = &**store;
    let obj =
        pyreal_client_runtime::assets::enum_did(a, UIASSET_GROUP, ENUM_PORTALSPACE_BACKGROUND)
            .expect("portalspace_background resolves");
    let i = r
        .add_preview_object(PreviewId::Portal, store, obj)
        .expect("the object is built")
        .expect("the setup loads");
    assert_eq!(i, 0, "the first appended preview object has index zero");
    r
}

///
/// Oracle: the portal update starts the resolved `0x10000002` animation on object zero at
/// low frame 1, clearing prior animations and running at 40 frames per second; the current frame
/// reader supplies the observation.
///
/// The assertion is against **elapsed seconds**, not against a frame count: the sequence update
/// advances `frame_number` by `framerate * dt`.
#[test]
fn the_portal_sequence_advances_at_forty_frames_a_second_of_elapsed_time() {
    let store = store();
    let mut r = portal_renderer(&store);
    let a: &dyn pyreal_primitives::AssetSource = &*store;
    let anim =
        pyreal_client_runtime::assets::enum_did(a, UIASSET_GROUP, ENUM_PORTALSPACE_ANIMATION)
            .expect("portalspace_animation resolves");

    let space = r
        .preview_mut(PreviewId::Portal)
        .expect("the space is there");
    // `low_frame = 1`, `framerate = 40.0`, `clear = 1` -- the portal update's arguments.
    assert!(
        space.set_sequence_animation(0, anim, true, 1, portal_framerate()),
        "the sequence really started -- an animation the dat does not hold is dropped silently"
    );
    assert_eq!(
        space.curr_frame_number(0),
        1,
        "set_sequence_animation starts at low_frame"
    );

    // Half a second of elapsed time is twenty frames at 40 fps, from frame 1.
    for _ in 0..30 {
        space.use_time(0.5 / 30.0);
    }
    let after_half = space.curr_frame_number(0);
    assert_eq!(
        after_half, 21,
        "1 + 0.5 s * 40 fps = frame 21, got {after_half}"
    );
}

/// **Frame-rate independence.**
///
/// The same wall-clock duration, stepped at 30 Hz and at 144 Hz, must reach the same animation
/// frame. A player that accumulated a per-frame increment instead would drift.
#[test]
fn the_same_duration_at_two_frame_rates_reaches_the_same_frame() {
    let store = store();
    let a: &dyn pyreal_primitives::AssetSource = &*store;
    let anim =
        pyreal_client_runtime::assets::enum_did(a, UIASSET_GROUP, ENUM_PORTALSPACE_ANIMATION)
            .expect("portalspace_animation resolves");

    let run = |steps: u32, seconds: f64| {
        let mut r = portal_renderer(&store);
        let space = r
            .preview_mut(PreviewId::Portal)
            .expect("the space is there");
        assert!(space.set_sequence_animation(0, anim, true, 1, portal_framerate()));
        let dt = seconds / f64::from(steps);
        for _ in 0..steps {
            space.use_time(dt);
        }
        space.curr_frame_number(0)
    };

    // 2.5 seconds is a hundred frames at 40 fps -- past the animation's own length, so the wrap is
    // exercised as well as the advance.
    let slow = run(75, 2.5); // 30 Hz
    let fast = run(360, 2.5); // 144 Hz
    assert_eq!(
        slow, fast,
        "2.5 s of elapsed time is 2.5 s of animation at any frame rate"
    );
    // ... and it really moved, so an accumulator stuck at its start frame cannot pass.
    assert!(slow > 1, "the sequence advanced past its low frame: {slow}");
}

/// Oracle: the portal update's tunnel-continue exit opens when
/// `(0x78 - current_animation_frame) / 40.0` is in `(1.1, 1.3)`.
///
/// **This is the test that fails when the mechanism under it is removed.** A space whose sequence
/// animation was never started reports frame 0 for ever, `(120 - 0)/40 = 3.0` is outside the
/// window, and the tunnel can only ever leave on its five-second cap. Delete the
/// `set_sequence_animation` call below and the second half of this test fails.
#[test]
fn the_exit_window_is_reachable_only_once_the_sequence_animation_has_started() {
    let store = store();
    let a: &dyn pyreal_primitives::AssetSource = &*store;
    let anim =
        pyreal_client_runtime::assets::enum_did(a, UIASSET_GROUP, ENUM_PORTALSPACE_ANIMATION)
            .expect("portalspace_animation resolves");

    // Without the sequence: the counter never leaves 0 and the window is never open.
    let mut r = portal_renderer(&store);
    {
        let space = r
            .preview_mut(PreviewId::Portal)
            .expect("the space is there");
        space.clear_sequence_anims(0);
        let mut opened = false;
        for _ in 0..400 {
            space.use_time(0.025);
            opened |= in_exit_window(space.curr_frame_number(0));
        }
        assert_eq!(space.curr_frame_number(0), 0, "no animation, no frames");
        assert!(
            !opened,
            "the exit window never opens without a sequence animation"
        );
    }

    // With it: the window opens, and it opens where `(120 - frame)/40` says it should.
    let space = r
        .preview_mut(PreviewId::Portal)
        .expect("the space is there");
    assert!(space.set_sequence_animation(0, anim, true, 1, portal_framerate()));
    let mut opened_at = Vec::new();
    for _ in 0..400 {
        space.use_time(0.025);
        let f = space.curr_frame_number(0);
        if in_exit_window(f) {
            opened_at.push(f);
        }
    }
    assert!(
        !opened_at.is_empty(),
        "the exit window opens once the sequence is running"
    );
    for f in &opened_at {
        let remaining = (timing::PORTAL_EXIT_FRAME - f) as f32 / portal_framerate();
        assert!(
            remaining > timing::PORTAL_EXIT_WINDOW.0 && remaining < timing::PORTAL_EXIT_WINDOW.1,
            "frame {f} claims to be in the window at {remaining} s remaining"
        );
    }
}

// =================================================================================================
// 3. The portal swirl -- the teleport tunnel's consumer of the preview mechanism
// =================================================================================================

/// The application with Holtburg loaded and the game screen up, which is the state a player is in
/// when a portal fires.
fn app_in_world() -> App {
    let cfg = Config {
        headless: true,
        sound: false,
        ui: true,
        dat_dir: client_dir(),
        ..Config::default()
    };
    let mut app = App::new(cfg).expect("the application comes up");
    app.start_shell().expect("the UI comes up");
    let s = pyreal_client_runtime::scene::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..pyreal_client_runtime::scene::SceneConfig::default()
    };
    app.load_static_scene(s).expect("Holtburg loads");
    crate::common::app::game_screen(&mut app);
    for _ in 0..4 {
        app.frame();
    }
    app
}

/// The portal space's own constants reach it, and it is the space portal initialization describes
/// rather than the char-gen one.
#[test]
fn the_portal_space_is_built_the_way_post_init_builds_it() {
    let mut app = app_in_world();
    app.probe_mut().teleport_mut().apply_events(&[
        pyreal_client_net::client_session::SessionEvent::PlayerCreated(
            pyreal_primitives::ObjectId(0x5000_0001),
        ),
    ]);
    for _ in 0..20 {
        app.frame();
    }
    let space = app
        .renderer_mut()
        .preview(PreviewId::Portal)
        .expect("the portal space exists");
    assert_eq!(space.object_count(), 1, "one object: the portal object");
    assert!(
        space.mode.use_world_fov,
        "the world field of view is what makes the tunnel's projection collapse reach the swirl"
    );
    assert!(
        !space.mode.use_sharp_mode,
        "sharp mode belongs to the character-generation 3D view, not this preview"
    );
    assert_eq!(space.mode.lights.len(), 1);
    let l = space.mode.lights[0];
    assert_eq!(l.intensity, portal_space::LIGHT_INTENSITY);
    assert_eq!(
        (l.direction.x, l.direction.y, l.direction.z),
        portal_space::LIGHT_DIRECTION,
        "the portal light points at negative y; the character-generation 3D view's points at positive y"
    );
    let o = space.mode.view_frame.origin;
    assert_eq!((o.x, o.y, o.z), portal_space::CAMERA_POSITION);
    // The two spaces are distinct: one *mechanism*, one instance per viewport widget.
    assert!(
        app.renderer_mut().preview(PreviewId::CharGen).is_none(),
        "the char-gen space is not built by a login"
    );
    app.shutdown();
}
