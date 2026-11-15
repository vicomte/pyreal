//! What an object id draws: a `Setup`'s parts at their resting placement and default scale, or a
//! bare `GfxObj` as one part, each part at the mesh its near detail level names.

use glam::{Affine3A, Quat, Vec3};
use pyreal_animation::data::DegradeInfo;
use pyreal_animation::parts::{get_degrade, DegradeSettings};
use pyreal_assets::{Decode, GfxObj, GfxObjDegradeInfo, Setup};
use pyreal_dat::{divine_type, DbType, RetailDatStore};
use pyreal_primitives::{DataId, Frame};

/// The placement every static object rests in; key `0` when the setup has none.
pub const PLACEMENT_RESTING: u32 = 0x65;

/// One drawn part: its mesh and where it sits in the object's own space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Part {
    pub gfxobj: DataId,
    pub transform: Affine3A,
}

/// A dat frame as a transform.
#[must_use]
pub fn frame_affine(f: &Frame) -> Affine3A {
    let q = Quat::from_xyzw(f.rotation.x, f.rotation.y, f.rotation.z, f.rotation.w);
    let q = if q.length_squared() > 0.0 {
        q.normalize()
    } else {
        Quat::IDENTITY
    };
    Affine3A::from_rotation_translation(q, Vec3::new(f.origin.x, f.origin.y, f.origin.z))
}

/// An object placed at `frame` with a uniform `scale`: the scale applies to the parts' offsets and
/// meshes alike.
#[must_use]
pub fn object_affine(frame: &Frame, scale: f32) -> Affine3A {
    frame_affine(frame) * Affine3A::from_scale(Vec3::splat(scale))
}

fn decode<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

/// The mesh a part draws close up: the level its degrade record picks at distance zero, the part
/// itself when it has none, `None` when that level draws nothing.
#[must_use]
pub fn near_mesh(store: &RetailDatStore, gfxobj: DataId) -> Option<DataId> {
    let Some(obj) = decode::<GfxObj>(store, DbType::GfxObj, gfxobj) else {
        return Some(gfxobj);
    };
    let Some(did) = obj.did_degrade else {
        return Some(gfxobj);
    };
    let Some(info) = decode::<GfxObjDegradeInfo>(store, DbType::DegradeInfo, did) else {
        return Some(gfxobj);
    };
    let info = DegradeInfo {
        degrades: info.degrades,
    };
    let (level, _) = get_degrade(&info, 0.0, DegradeSettings::default());
    let chosen = usize::try_from(level)
        .ok()
        .and_then(|i| info.degrades.get(i))
        .map_or(gfxobj, |e| e.gfxobj_id);
    (chosen.0 != 0).then_some(chosen)
}

/// One coarser level of a part: its mesh (`None` draws nothing), the distance from which it is
/// drawn, and how it turns to the eye (the dats' `degrade_mode`: 1 not at all, 2 to 5 a card).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DegradeLevel {
    pub mesh: Option<DataId>,
    pub switch: f32,
    pub mode: u32,
}

/// How the level a part draws close up turns to the eye (its degrade record's mode there, 1 for
/// none).
#[must_use]
pub fn near_mode(store: &RetailDatStore, gfxobj: DataId) -> u32 {
    let Some(info) = decode::<GfxObj>(store, DbType::GfxObj, gfxobj)
        .and_then(|o| o.did_degrade)
        .and_then(|did| decode::<GfxObjDegradeInfo>(store, DbType::DegradeInfo, did))
    else {
        return 1;
    };
    let info = DegradeInfo {
        degrades: info.degrades,
    };
    let (_, mode) = get_degrade(&info, 0.0, DegradeSettings::default());
    u32::try_from(mode).unwrap_or(1)
}

/// What a part draws past its near level, nearest first. Level `i` is the client's pick while
/// `distance - 50` is under its `ideal_dist` (`get_degrade` at no bias), so level `i + 1` takes
/// over at level `i`'s `ideal_dist` plus 50 metres.
#[must_use]
pub fn degrade_chain(store: &RetailDatStore, gfxobj: DataId) -> Vec<DegradeLevel> {
    let Some(info) = decode::<GfxObj>(store, DbType::GfxObj, gfxobj)
        .and_then(|o| o.did_degrade)
        .and_then(|did| decode::<GfxObjDegradeInfo>(store, DbType::DegradeInfo, did))
    else {
        return Vec::new();
    };
    let settings = DegradeSettings::default();
    let degrades = info.degrades;
    let first = {
        let d = DegradeInfo {
            degrades: degrades.clone(),
        };
        usize::try_from(get_degrade(&d, 0.0, settings).0).unwrap_or(0)
    };
    let mut out: Vec<DegradeLevel> = Vec::new();
    let mut current = degrades.get(first).map(|e| e.gfxobj_id);
    for i in first + 1..degrades.len() {
        let switch = degrades[i - 1].ideal_dist + settings.degrade_distance;
        if !switch.is_finite() || switch >= f32::MAX / 2.0 {
            break;
        }
        let mesh = Some(degrades[i].gfxobj_id).filter(|g| g.0 != 0);
        if mesh.map(|g| g.0) == current.map(|g| g.0) {
            continue;
        }
        out.push(DegradeLevel {
            mesh,
            switch,
            mode: u32::try_from(degrades[i].degrade_mode).unwrap_or(1),
        });
        current = mesh;
        if mesh.is_none() {
            break;
        }
    }
    out
}

/// The parts `id` (a setup or a graphics object) draws, at the resting placement.
#[must_use]
pub fn object_parts(store: &RetailDatStore, id: DataId) -> Vec<Part> {
    match divine_type(id) {
        Some(DbType::GfxObj) => near_mesh(store, id)
            .map(|g| Part {
                gfxobj: g,
                transform: Affine3A::IDENTITY,
            })
            .into_iter()
            .collect(),
        Some(DbType::Setup) => {
            let Some(setup) = decode::<Setup>(store, DbType::Setup, id) else {
                return Vec::new();
            };
            setup_parts(&setup)
                .into_iter()
                .filter_map(|p| near_mesh(store, p.gfxobj).map(|g| Part { gfxobj: g, ..p }))
                .collect()
        }
        _ => Vec::new(),
    }
}

/// A setup's parts at the resting placement (key `0` when it has none, the identity when it has
/// neither), each scaled by the setup's default scale.
#[must_use]
pub fn setup_parts(setup: &Setup) -> Vec<Part> {
    let frames = setup
        .placement_frames
        .get(&PLACEMENT_RESTING)
        .or_else(|| setup.placement_frames.get(&0))
        .map(|p| p.frames.as_slice());
    setup
        .parts
        .iter()
        .enumerate()
        .map(|(i, &gfxobj)| {
            let at = frames
                .and_then(|f| f.get(i))
                .map_or(Affine3A::IDENTITY, frame_affine);
            let scale = setup
                .default_scale
                .as_ref()
                .and_then(|d| d.get(i))
                .map_or(Vec3::ONE, |s| Vec3::new(s.x, s.y, s.z));
            Part {
                gfxobj,
                transform: at * Affine3A::from_scale(scale),
            }
        })
        .collect()
}
