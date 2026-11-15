//! A press measured against a moved view.

use super::*;
// =============================================================================================
// viewport.* (dat) -- the half that needs the shipped geometry
//
// The three arithmetic rows are in the `cpu` tier's `shell.rs`, and the reasoning is there.
// =============================================================================================

/// The window, throughout. Every press is given in **window** coordinates.
const VIEW_WINDOW: (u32, u32) = (800, 600);
/// The whole window as a view.
const VIEW_WHOLE: pyreal_primitives::viewport::Viewport = pyreal_primitives::viewport::Viewport {
    x: 0,
    y: 0,
    width: VIEW_WINDOW.0,
    height: VIEW_WINDOW.1,
};
/// A moved and resized view, deliberately off-centre.
const VIEW_INSET: pyreal_primitives::viewport::Viewport = pyreal_primitives::viewport::Viewport {
    x: 240,
    y: 90,
    width: 400,
    height: 300,
};
/// That view's own middle, and the window's, which are not the same point.
const VIEW_INSET_MIDDLE: (i32, i32) = (240 + 199, 90 + 149);
const VIEW_WINDOW_MIDDLE: (i32, i32) = (399, 299);

/// The thing to be found, and the shape it is drawn with -- a real body from the shipped data, so
/// the sweep has geometry to sweep.
const VIEW_TARGET: pyreal_primitives::ObjectId = pyreal_primitives::ObjectId(0x8300_0F01);
const VIEW_BODY: pyreal_primitives::DataId = pyreal_primitives::DataId(0x0200_0001);

/// The viewer and the one thing in front of it -- all the sweep reads, and the reason this needs
/// no device.
struct AView {
    viewer: pyreal_primitives::Frame,
    frames: std::collections::BTreeMap<pyreal_primitives::ObjectId, pyreal_primitives::Frame>,
}

impl pyreal_client_runtime::pick::PickScene for AView {
    fn viewer(&self) -> pyreal_primitives::Frame {
        self.viewer
    }
    fn object_frame(&self, id: pyreal_primitives::ObjectId) -> Option<pyreal_primitives::Frame> {
        self.frames.get(&id).copied()
    }
    fn fov_y_rad(&self, _window: (u32, u32)) -> f32 {
        pyreal_client_contract::camera::DEFAULT_FOV_DEGREES
            * pyreal_client_contract::camera::DEG_TO_RAD
    }
}

/// One thing three metres due north of the eye and level with it.
fn one_thing_ahead(
    store: &std::sync::Arc<pyreal_dat::RetailDatStore>,
) -> (pyreal_client_runtime::objects::ObjectStream, AView) {
    let mut objects = pyreal_client_runtime::objects::ObjectStream::with_store(store.clone());
    let payload = pyreal_protocol::objects::ObjectCreatePayload {
        id: VIEW_TARGET,
        objdesc: pyreal_protocol::types::ObjDesc::default(),
        physicsdesc: pyreal_protocol::types::PhysicsDesc {
            bitfield: pyreal_protocol::types::physicsdesc::flags::POSITION
                | pyreal_protocol::types::physicsdesc::flags::SETUP,
            setup_id: Some(VIEW_BODY.0),
            position: Some(pyreal_protocol::types::PositionWire {
                objcell_id: 0x0001_0001,
                frame: pyreal_protocol::types::Frame {
                    origin: pyreal_primitives::Vec3::ZERO.into(),
                    orientation: pyreal_primitives::Quat::new(1.0, 0.0, 0.0, 0.0).into(),
                },
            }),
            ..pyreal_protocol::types::PhysicsDesc::default()
        },
        wdesc: pyreal_protocol::types::PublicWeenieDesc::default(),
    };
    let body = pyreal_protocol::write_body(&pyreal_protocol::objects::ItemCreateObject(payload))
        .expect("an object create encodes");
    objects.apply_event(
        &pyreal_client_net::client_session::SessionEvent::WorldObject {
            opcode: pyreal_protocol::Opcode::ITEM_CREATE_OBJECT,
            body,
        },
        pyreal_primitives::LocalTime(1.0),
    );
    let identity = pyreal_primitives::Quat::new(1.0, 0.0, 0.0, 0.0);
    let view = AView {
        viewer: pyreal_primitives::Frame::new(
            pyreal_primitives::Vec3::new(0.0, -3.0, 0.7),
            identity,
        ),
        frames: [(
            VIEW_TARGET,
            pyreal_primitives::Frame::new(pyreal_primitives::Vec3::new(0.0, 0.0, 0.0), identity),
        )]
        .into_iter()
        .collect(),
    };
    (objects, view)
}

/// One whole look: aim at a window point, sweep, and answer what was found.
fn what_is_found_at(
    store: &pyreal_dat::RetailDatStore,
    objects: &pyreal_client_runtime::objects::ObjectStream,
    view: &AView,
    at: (i32, i32),
    viewport: pyreal_primitives::viewport::Viewport,
) -> Option<pyreal_primitives::ObjectId> {
    let mut pick = pyreal_client_runtime::pick::WorldPicker::new();
    if !pick.find_object(at.0, at.1, viewport) {
        return None;
    }
    pick.draw_no_blit(store, view, objects, VIEW_WINDOW, viewport)
}

// =============================================================================================
// viewport.a-press-in-the-middle-of-a-moved-view-finds-what-is-ahead-of-the-eye
// =============================================================================================

/// The rejecting half is the window's middle: a client that ignored the view's corner would find
/// the thing from there, which is exactly what the reported one did.
pub(super) fn a_press_in_the_middle_of_a_moved_view_finds_what_is_ahead_of_the_eye() {
    let mut c = HeadlessClient::new(ClientSpec::retail());
    let store = c
        .dat_store()
        .expect("the retail data files are open")
        .clone();
    let (objects, view) = one_thing_ahead(&store);

    // The control first, so that a client in which nothing is findable at all is caught here
    // rather than read as a pass below.
    let found_through_the_whole_window =
        what_is_found_at(&store, &objects, &view, VIEW_WINDOW_MIDDLE, VIEW_WHOLE)
            == Some(VIEW_TARGET);

    let found_through_the_moved_view =
        what_is_found_at(&store, &objects, &view, VIEW_INSET_MIDDLE, VIEW_INSET)
            == Some(VIEW_TARGET);

    // ...and the window's middle is not the view's, so aiming there must miss.
    let a_real_difference = VIEW_INSET_MIDDLE != VIEW_WINDOW_MIDDLE;
    let the_windows_middle_misses =
        what_is_found_at(&store, &objects, &view, VIEW_WINDOW_MIDDLE, VIEW_INSET)
            != Some(VIEW_TARGET);

    c.assert_behaviour(
        "viewport.a-press-in-the-middle-of-a-moved-view-finds-what-is-ahead-of-the-eye",
        move |_| {
            found_through_the_whole_window
                && found_through_the_moved_view
                && a_real_difference
                && the_windows_middle_misses
        },
    );
    c.shutdown();
}
