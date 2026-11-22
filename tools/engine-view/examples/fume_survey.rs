//! Lists every emitter in the dats that blows laid-over (not added), self-lit puffs that rise and
//! swell, with its picture's tone and colour, its motion and fade, how many placed statics' default
//! scripts play it, and whether Pyreal draws it as a grate's fume ([`pyreal_engine::puffs::fume_of`]).
//!
//! Usage: `fume_survey DAT_DIR`.

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

/// Whether picture `gid` adds its light, and its first surface's luminosity.
fn surfaces(store: &RetailDatStore, gid: DataId) -> (bool, f32) {
    let Some(g) = read::<GfxObj>(store, DbType::GfxObj, gid) else {
        return (false, 0.0);
    };
    let s: Vec<Surface> = g
        .surfaces
        .iter()
        .filter_map(|s| read::<Surface>(store, DbType::Surface, *s))
        .collect();
    (
        s.iter().any(|s| s.surface_type & 0x1_0000 != 0),
        s.first().map_or(0.0, |s| s.luminosity),
    )
}

fn main() {
    use pyreal_animation::data::AnimAssets;
    let a: Vec<String> = std::env::args().collect();
    let dir = std::path::Path::new(&a[1]);
    let store = std::sync::Arc::new(
        if pyreal_dat::holds_modern_dats(dir) {
            RetailDatStore::open_dir(dir)
        } else {
            RetailDatStore::open_classic_dir(dir)
        }
        .expect("dats"),
    );
    let assets = pyreal_world_data::anim_assets::DatAnimAssets::new(std::sync::Arc::clone(&store));
    // How many times each setup is placed, and which emitters its default script plays.
    let mut placed: BTreeMap<u32, u32> = BTreeMap::new();
    let mut outdoors: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    let mut indoors: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for id in store.ids_of(DbType::Cell) {
        if id.0 & 0xFFFF < 0x100 {
            continue;
        }
        if let Some(cell) = read::<EnvCell>(&store, DbType::Cell, id) {
            for o in &cell.static_objects {
                *placed.entry(o.id.0).or_default() += 1;
                let f = &o.frame.origin;
                indoors
                    .entry(o.id.0)
                    .or_default()
                    .push(format!("{:#010X}@{:.1},{:.1},{:.1}", id.0, f.x, f.y, f.z));
            }
        }
    }
    for id in store.ids_of(DbType::Lbi) {
        if let Some(info) = read::<LandblockInfo>(&store, DbType::Lbi, id) {
            for o in &info.objects {
                *placed.entry(o.id.0).or_default() += 1;
                let f = &o.frame.origin;
                outdoors.entry(o.id.0).or_default().push(format!(
                    "{:#06X}@{:.1},{:.1}",
                    id.0 >> 16,
                    f.x,
                    f.y
                ));
            }
        }
    }
    let mut plays: BTreeMap<u32, (u32, Vec<u32>)> = BTreeMap::new();
    for (&id, &n) in &placed {
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
            if let HookData::CreateParticle(cp) = &step.hook.data {
                let e = plays.entry(cp.emitter_info_id.0).or_default();
                e.0 += n;
                e.1.push(id);
            }
        }
    }
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
        let (additive, lum) = surfaces(&store, gid);
        let s = pyreal_engine::puffs::Stream::of(&info);
        let tone = pyreal_engine::lamps::tone(&store, gid);
        let (_, colour, _) = pyreal_engine::puffs::picture(&store, gid);
        let fume = pyreal_engine::puffs::fume_of(tone, additive, colour, &info).is_some();
        if !fume && (additive || !s.endless || s.rise < 0.2 || lum < 0.5) {
            continue;
        }
        let (n, hosts) = plays.get(&id.0).cloned().unwrap_or_default();
        println!(
            "{:#010X} fume {fume:5} placed {n:5} pic {:#010X} lum {lum:.2} {tone:?} colour [{:.3} {:.3} {:.3}] ptype {} max {} life {:.1} scale {:.1}->{:.1} trans {:.2}->{:.2} rise {:.2} hosts {:X?}",
            id.0, gid.0, colour.x, colour.y, colour.z, e.particle_type, e.max_particles,
            e.lifespan, e.start_scale, e.final_scale, e.start_trans, e.final_trans, s.rise,
            &hosts[..hosts.len().min(4)]
        );
        if fume {
            for h in &hosts {
                let at = outdoors.get(h).cloned().unwrap_or_default();
                println!("  {h:#010X} outdoors {}: {}", at.len(), at.join(" "));
                let at = indoors.get(h).cloned().unwrap_or_default();
                println!("  {h:#010X} in cells {}: {}", at.len(), at.join(" "));
            }
        }
    }
}
