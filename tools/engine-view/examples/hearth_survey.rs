//! Lists every bed of fire placed inside another placed object (a forge, a cauldron's tripod, a
//! fireplace built apart from its fire): the two setups, how often, where the fire stands in the
//! other's bounds, and how many burn as a forge's or a fire pit's fire
//! ([`pyreal_engine::flames::firebox_of`], in rooms only).
//!
//! Usage: `hearth_survey DAT_DIR` (End of Retail's files, or an older portal.dat and cell.dat)

use std::collections::BTreeMap;

use glam::{Affine3A, Quat, Vec3};
use pyreal_assets::world::{EnvCell, LandblockInfo};
use pyreal_assets::Decode;
use pyreal_dat::{DbType, RetailDatStore};
use pyreal_engine::flames::{examine, firebox_of, setup_bounds, FlameKind, FlameSource};
use pyreal_primitives::DataId;

fn read<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

fn affine(f: &pyreal_primitives::Frame) -> Affine3A {
    let r = f.rotation;
    Affine3A::from_rotation_translation(
        Quat::from_xyzw(r.x, r.y, r.z, r.w),
        Vec3::new(f.origin.x, f.origin.y, f.origin.z),
    )
}

#[derive(Default)]
struct Row {
    n: u32,
    example: u32,
    at: Vec3,
    facing: Vec3,
    /// Least margin inside the sides, height over the floor, hood over the fire.
    sides: (f32, f32),
    floor: (f32, f32),
    hood: (f32, f32),
    /// Placements that burn as a forge's or a fire pit's fire.
    forged: u32,
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(&a[1]);
    let store = if pyreal_dat::holds_modern_dats(dir) {
        RetailDatStore::open_dir(dir)
    } else {
        RetailDatStore::open_classic_dir(dir)
    }
    .expect("dats");
    let mut beds: BTreeMap<u32, Vec<FlameSource>> = BTreeMap::new();
    let mut bounds: BTreeMap<u32, Option<(Vec3, Vec3)>> = BTreeMap::new();
    let mut rows: BTreeMap<(u32, u32), Row> = BTreeMap::new();
    let mut look = |where_: u32, room: bool, objects: &[(DataId, Affine3A)]| {
        for (id, at) in objects {
            let fires = beds.entry(id.0).or_insert_with(|| {
                examine(&store, *id)
                    .iter()
                    .filter(|f| f.kind == FlameKind::Brazier)
                    .copied()
                    .collect()
            });
            for bed in fires.iter() {
                let fire = at.transform_point3(bed.offset);
                for (other, oat) in objects {
                    if other == id {
                        continue;
                    }
                    let Some((lo, hi)) = *bounds
                        .entry(other.0)
                        .or_insert_with(|| setup_bounds(&store, *other))
                    else {
                        continue;
                    };
                    let p = oat.inverse().transform_point3(fire);
                    let sides = (p.x - lo.x).min(hi.x - p.x).min(p.y - lo.y).min(hi.y - p.y);
                    let forged = room && firebox_of(bed, at, &[(other.0, *oat)]).is_some();
                    if !forged && (sides <= 0.0 || hi.z < p.z + 0.5 || p.z < lo.z - 0.5) {
                        continue;
                    }
                    let r = rows.entry((id.0, other.0)).or_insert(Row {
                        example: where_,
                        at: fire,
                        facing: oat.transform_point3((lo + hi) * 0.5),
                        sides: (f32::MAX, f32::MIN),
                        floor: (f32::MAX, f32::MIN),
                        hood: (f32::MAX, f32::MIN),
                        ..Row::default()
                    });
                    r.n += 1;
                    r.forged += u32::from(forged);
                    let span = |s: &mut (f32, f32), v: f32| *s = (s.0.min(v), s.1.max(v));
                    span(&mut r.sides, sides);
                    span(&mut r.floor, p.z - lo.z);
                    span(&mut r.hood, hi.z - p.z);
                }
            }
        }
    };
    for id in store.ids_of(DbType::Cell) {
        if id.0 & 0xFFFF < 0x100 {
            continue;
        }
        if let Some(cell) = read::<EnvCell>(&store, DbType::Cell, id) {
            let objects: Vec<_> = cell
                .static_objects
                .iter()
                .map(|o| (o.id, affine(&o.frame)))
                .collect();
            look(id.0, true, &objects);
        }
    }
    for id in store.ids_of(DbType::Lbi) {
        if let Some(info) = read::<LandblockInfo>(&store, DbType::Lbi, id) {
            let objects: Vec<_> = info
                .objects
                .iter()
                .map(|o| (o.id, affine(&o.frame)))
                .chain(info.buildings.iter().map(|b| (b.id, affine(&b.frame))))
                .collect();
            look(id.0, false, &objects);
        }
    }
    for ((fire, other), r) in rows {
        println!(
            "fire {fire:#010X} in {other:#010X} x{:5} e.g. {:#010X} at {:.2} {:.2} {:.2} middle {:.2} {:.2} {:.2}: sides {:.2}..{:.2} floor {:.2}..{:.2} hood {:.2}..{:.2} forged {}",
            r.n, r.example, r.at.x, r.at.y, r.at.z, r.facing.x, r.facing.y, r.facing.z, r.sides.0, r.sides.1, r.floor.0, r.floor.1, r.hood.0, r.hood.1, r.forged
        );
    }
}
