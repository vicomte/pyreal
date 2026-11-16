//! Lists every placed setup whose default script throws water: an endless emitter pulled down
//! hard (a waterfall's sheets), or one of the waterfall's pictures (its sheet, its mist), with
//! how many times the landblocks and the dungeons' cells place it, most placed first.
//!
//! Usage: `fall_survey DAT_DIR`, or `fall_survey DAT_DIR SETUP` for every place one setup stands.

use std::collections::HashMap;

use pyreal_assets::hook::HookData;
use pyreal_assets::world::{EnvCell, LandblockInfo, ParticleEmitterInfo};
use pyreal_assets::{Decode, PhysicsScript, Setup};
use pyreal_dat::{DbType, RetailDatStore};
use pyreal_primitives::DataId;

/// The waterfall's pictures: its sheet (both levels) and its mist.
const PICTURES: [u32; 3] = [0x0100_109E, 0x0100_1081, 0x0100_141C];

fn read<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

fn placed(store: &RetailDatStore, mut each: impl FnMut(u32, u32, pyreal_primitives::Vec3)) {
    for id in store.ids_of(DbType::Lbi) {
        if let Some(info) = read::<LandblockInfo>(store, DbType::Lbi, id) {
            for o in &info.objects {
                each(o.id.0, id.0, o.frame.origin);
            }
        }
    }
    for id in store.ids_of(DbType::Cell) {
        if id.0 & 0xFFFF < 0x100 {
            continue;
        }
        if let Some(cell) = read::<EnvCell>(store, DbType::Cell, id) {
            for o in &cell.static_objects {
                each(o.id.0, id.0, o.frame.origin);
            }
        }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let store = RetailDatStore::open_dir(std::path::Path::new(&a[1])).expect("dats");
    if let Some(only) = a.get(2) {
        let only = u32::from_str_radix(only.trim_start_matches("0x"), 16).expect("setup");
        placed(&store, |id, at, o| {
            if id == only {
                println!("{at:#010X} {o:?}");
            }
        });
        return;
    }
    // ORDER-OK: counted, then sorted.
    let mut count: HashMap<u32, (u32, u32)> = HashMap::new();
    placed(&store, |id, at, _| {
        count.entry(id).or_insert((0, at)).0 += 1;
    });
    let mut rows = Vec::new();
    for (&id, &(n, at)) in &count {
        let Some(setup) = read::<Setup>(&store, DbType::Setup, DataId(id)) else {
            continue;
        };
        let Some(script) =
            read::<PhysicsScript>(&store, DbType::PhysicsScript, setup.default_script_id)
        else {
            continue;
        };
        let mut desc = Vec::new();
        for step in &script.script_data {
            let HookData::CreateParticle(c) = &step.hook.data else {
                continue;
            };
            let Some(e) =
                read::<ParticleEmitterInfo>(&store, DbType::ParticleEmitter, c.emitter_info_id)
            else {
                continue;
            };
            let gid = if e.hw_gfxobj_id.0 != 0 {
                e.hw_gfxobj_id
            } else {
                e.gfxobj_id
            };
            let endless = e.total_particles <= 0 && e.total_seconds <= 0.0;
            let pull = e.b.z * e.max_b;
            if !(PICTURES.contains(&gid.0) || endless && pull <= -5.0 && e.particle_type >= 3) {
                continue;
            }
            desc.push(format!(
                "{:#010X} type {} life {:.1} scale {:.1}->{:.1} max {} a {:?}x{:.2} b {:?}x{:.2} pic {:#010X}",
                e.id.0,
                e.particle_type,
                e.lifespan,
                e.start_scale,
                e.final_scale,
                e.max_particles,
                (e.a.x, e.a.y, e.a.z),
                e.max_a,
                (e.b.x, e.b.y, e.b.z),
                e.max_b,
                gid.0
            ));
        }
        if !desc.is_empty() {
            rows.push((n, id, at, desc.join("; ")));
        }
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for (n, id, at, d) in rows {
        println!("{id:#010X} placed {n:5} e.g. {at:#010X}: {d}");
    }
}
