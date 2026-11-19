//! Lists every placed static (cells and landblocks) whose default script blows laid-over (not
//! added) puffs that swell: each emitter's picture tone, motion and fade, the volume Pyreal draws
//! it as, and how many times it is placed, most placed first.
//!
//! Usage: `gas_survey DAT_DIR`, or `gas_survey DAT_DIR --all` for every emitter in the dats that
//! Pyreal draws as a vent's gas, placed or not.

use std::collections::BTreeMap;

use pyreal_assets::hook::HookData;
use pyreal_assets::world::{EnvCell, LandblockInfo, ParticleEmitterInfo};
use pyreal_assets::{Decode, GfxObj, PhysicsScript, Setup, Surface};
use pyreal_dat::{divine_type, DbType, RetailDatStore};
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
    use pyreal_animation::data::AnimAssets;
    let a: Vec<String> = std::env::args().collect();
    let store =
        std::sync::Arc::new(RetailDatStore::open_dir(std::path::Path::new(&a[1])).expect("dats"));
    let assets = pyreal_world_data::anim_assets::DatAnimAssets::new(std::sync::Arc::clone(&store));
    if a.get(2).is_some_and(|f| f == "--all") {
        for id in store.ids_of(DbType::ParticleEmitter) {
            let (Some(e), Some(info)) = (
                read::<ParticleEmitterInfo>(&store, DbType::ParticleEmitter, id),
                assets.emitter_info(id),
            ) else {
                continue;
            };
            let gid = if e.hw_gfxobj_id.0 != 0 {
                e.hw_gfxobj_id
            } else {
                e.gfxobj_id
            };
            let tone = pyreal_engine::lamps::tone(&store, gid);
            if pyreal_engine::puffs::gas_of(tone, additive(&store, gid), &info).is_some() {
                println!(
                    "{:#010X} ptype {} pic {:#010X} max {} life {:.1} scale {:.1}->{:.1} trans {:.2}->{:.2} local {}",
                    id.0, e.particle_type, gid.0, e.max_particles, e.lifespan, e.start_scale,
                    e.final_scale, e.start_trans, e.final_trans, e.is_parent_local
                );
            }
        }
        return;
    }
    let mut placed: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
    for id in store.ids_of(DbType::Cell) {
        if id.0 & 0xFFFF < 0x100 {
            continue;
        }
        if let Some(cell) = read::<EnvCell>(&store, DbType::Cell, id) {
            for o in &cell.static_objects {
                placed.entry(o.id.0).or_insert((0, id.0)).0 += 1;
            }
        }
    }
    for id in store.ids_of(DbType::Lbi) {
        if let Some(info) = read::<LandblockInfo>(&store, DbType::Lbi, id) {
            for o in &info.objects {
                placed.entry(o.id.0).or_insert((0, id.0)).0 += 1;
            }
        }
    }
    let mut rows = Vec::new();
    for (&id, &(n, example)) in &placed {
        if divine_type(DataId(id)) != Some(DbType::Setup) {
            continue;
        }
        let Some(s) = read::<Setup>(&store, DbType::Setup, DataId(id)) else {
            continue;
        };
        let Some(script) =
            read::<PhysicsScript>(&store, DbType::PhysicsScript, s.default_script_id)
        else {
            continue;
        };
        for step in &script.script_data {
            let HookData::CreateParticle(cp) = &step.hook.data else {
                continue;
            };
            let Some(e) =
                read::<ParticleEmitterInfo>(&store, DbType::ParticleEmitter, cp.emitter_info_id)
            else {
                continue;
            };
            let gid = if e.hw_gfxobj_id.0 != 0 {
                e.hw_gfxobj_id
            } else {
                e.gfxobj_id
            };
            let endless = e.total_particles <= 0 && e.total_seconds <= 0.0;
            if !endless || e.lifespan < 1.0 || e.max_particles < 3 || additive(&store, gid) {
                continue;
            }
            if e.final_scale < e.start_scale * 1.4 {
                continue;
            }
            let Some(info) = assets.emitter_info(cp.emitter_info_id) else {
                continue;
            };
            let tone = pyreal_engine::lamps::tone(&store, gid);
            let (_, colour, _) = pyreal_engine::puffs::picture(&store, gid);
            let kind = pyreal_engine::puffs::gas_of(tone, false, &info);
            rows.push((
                n,
                format!(
                    "{id:#010X} placed {n:5} e.g. {example:#010X}: emitter {:#010X} ptype {} pic {:#010X} {tone:?} colour [{:.3} {:.3} {:.3}] max {} life {:.1} scale {:.1}->{:.1} trans {:.2}->{:.2} rise {:.2} local {} kind {kind:?}",
                    e.id.0, e.particle_type, gid.0, colour.x, colour.y, colour.z, e.max_particles,
                    e.lifespan, e.start_scale, e.final_scale, e.start_trans, e.final_trans,
                    pyreal_engine::puffs::Stream::of(&info).rise, e.is_parent_local
                ),
            ));
        }
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for (_, r) in rows {
        println!("{r}");
    }
}
