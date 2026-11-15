//! Lists every placed setup whose default script makes steam-like puffs (an endless stream of
//! additive pictures that swell over a second and a half or more): its emitters' particle type,
//! scales, rise and picture, and how many times the landblocks place it, most placed first.
//!
//! Usage: `steam_survey DAT_DIR`, or `steam_survey DAT_DIR SETUP` for every place one setup stands.

use std::collections::HashMap;

use pyreal_assets::hook::HookData;
use pyreal_assets::world::{LandblockInfo, ParticleEmitterInfo};
use pyreal_assets::{Decode, GfxObj, PhysicsScript, Setup, Surface};
use pyreal_dat::{DbType, RetailDatStore};
use pyreal_primitives::DataId;

fn read<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

fn additive(store: &RetailDatStore, gid: DataId) -> bool {
    read::<GfxObj>(store, DbType::GfxObj, gid).is_some_and(|g| {
        g.surfaces.iter().any(|s| {
            read::<Surface>(store, DbType::Surface, *s)
                .is_some_and(|s| s.surface_type & 0x1_0000 != 0)
        })
    })
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let store = RetailDatStore::open_dir(std::path::Path::new(&a[1])).expect("dats");
    if let Some(only) = a.get(2) {
        let only = u32::from_str_radix(only.trim_start_matches("0x"), 16).expect("setup");
        for id in store.ids_of(DbType::Lbi) {
            if let Some(info) = read::<LandblockInfo>(&store, DbType::Lbi, id) {
                for o in info.objects.iter().filter(|o| o.id.0 == only) {
                    println!("{:#06X} {:?}", id.0 >> 16, o.frame.origin);
                }
            }
        }
        return;
    }
    // ORDER-OK: counted, then sorted.
    let mut placed: HashMap<u32, (u32, u32)> = HashMap::new();
    for id in store.ids_of(DbType::Lbi) {
        if let Some(info) = read::<LandblockInfo>(&store, DbType::Lbi, id) {
            for o in &info.objects {
                let e = placed.entry(o.id.0).or_insert((0, id.0 >> 16));
                e.0 += 1;
            }
        }
    }
    let mut rows = Vec::new();
    for (&id, &(n, block)) in &placed {
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
            let swells =
                e.final_scale >= e.start_scale * 1.4 && e.start_scale.max(e.final_scale) >= 0.8;
            if !endless || e.lifespan < 1.5 || !swells || !additive(&store, gid) {
                continue;
            }
            let rise = e.a.z * e.max_a + e.b.z * e.max_b + e.c.z * e.max_c;
            desc.push(format!(
                "{:#010X} type {} life {:.1} scale {:.1}->{:.1} max {} rise {rise:.2} local {} pic {:#010X}",
                e.id.0, e.particle_type, e.lifespan, e.start_scale, e.final_scale, e.max_particles,
                e.is_parent_local, gid.0
            ));
        }
        if !desc.is_empty() {
            rows.push((n, id, block, desc.join("; ")));
        }
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for (n, id, block, d) in rows {
        println!("{id:#010X} placed {n:5} e.g. {block:#06X}: {d}");
    }
}
