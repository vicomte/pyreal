//! Lists everything a cell's static objects light and emit: each setup's authored lights, its
//! default script's emitters in full (rates, sizes, motion, fade, picture and its surfaces), the
//! look Pyreal draws each with, and the lamps Pyreal makes of it.
//!
//! Usage: `steam_probe DAT_DIR CELL...` (each as 0x7F03010F), or `steam_probe DAT_DIR --volumes`
//! for every placed static (cells and landblocks) whose emitters Pyreal draws as steam or haze,
//! or `steam_probe DAT_DIR --setup SETUP...` for setups' default scripts.

use pyreal_assets::hook::HookData;
use std::collections::BTreeMap;

use pyreal_assets::world::{EnvCell, LandblockInfo, ParticleEmitterInfo};
use pyreal_assets::{Decode, GfxObj, PhysicsScript, Setup, Surface};
use pyreal_dat::{divine_type, DbType, RetailDatStore};
use pyreal_primitives::DataId;

fn read<T: Decode>(store: &RetailDatStore, kind: DbType, id: DataId) -> Option<T> {
    let bytes = store.read_typed(kind, id).ok()?;
    T::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

fn emitter(store: &RetailDatStore, id: DataId, part: u32, at: &pyreal_primitives::Frame) {
    let Some(e) = read::<ParticleEmitterInfo>(store, DbType::ParticleEmitter, id) else {
        println!("      emitter {:#010X}: unreadable", id.0);
        return;
    };
    println!(
        "      emitter {:#010X} part {part} at {:?} etype {} ptype {} rate {:.3} max {} init {} total {}/{:.1} life {:.2}+-{:.2}",
        e.id.0, at.origin, e.emitter_type, e.particle_type, e.birthrate, e.max_particles,
        e.initial_particles, e.total_particles, e.total_seconds, e.lifespan, e.lifespan_rand
    );
    println!(
        "        offset dir {:?} {:.2}..{:.2}  a {:?} {:.2}..{:.2}  b {:?} {:.2}..{:.2}  c {:?} {:.2}..{:.2}",
        e.offset_dir, e.min_offset, e.max_offset, e.a, e.min_a, e.max_a, e.b, e.min_b, e.max_b,
        e.c, e.min_c, e.max_c
    );
    println!(
        "        scale {:.2}->{:.2} rand {:.2}  trans {:.2}->{:.2} rand {:.2}  local {}  gfx {:#010X} hw {:#010X}",
        e.start_scale, e.final_scale, e.scale_rand, e.start_trans, e.final_trans, e.trans_rand,
        e.is_parent_local, e.gfxobj_id.0, e.hw_gfxobj_id.0
    );
    let gid = if e.hw_gfxobj_id.0 != 0 {
        e.hw_gfxobj_id
    } else {
        e.gfxobj_id
    };
    if let Some(g) = read::<GfxObj>(store, DbType::GfxObj, gid) {
        let (lo, hi) = g.vertex_array.vertices.iter().fold(
            ([f32::MAX; 3], [f32::MIN; 3]),
            |(mut lo, mut hi), v| {
                let p = [v.position.x, v.position.y, v.position.z];
                for i in 0..3 {
                    lo[i] = lo[i].min(p[i]);
                    hi[i] = hi[i].max(p[i]);
                }
                (lo, hi)
            },
        );
        println!("        picture {:#010X} bounds {lo:.2?}..{hi:.2?}", gid.0);
        for s in &g.surfaces {
            if let Some(sf) = read::<Surface>(store, DbType::Surface, *s) {
                println!(
                    "          surface {:#010X} type {:#X} tex {:?} colour {:?} trans {:.2} lum {:.2} diffuse {:.2}",
                    s.0,
                    sf.surface_type,
                    sf.orig_texture_id.map(|t| format!("{:#010X}", t.0)),
                    sf.color_value.map(|c| format!("{c:#010X}")),
                    sf.translucency,
                    sf.luminosity,
                    sf.diffuse
                );
            }
        }
        let (half, colour, _) = pyreal_engine::puffs::picture(store, gid);
        println!(
            "          half {half:.2} mean linear colour {colour:.3}  tone {:?}",
            pyreal_engine::lamps::tone(store, gid)
        );
    }
}

/// A script's steps, and the scripts it calls, in full.
fn script(store: &RetailDatStore, id: DataId, depth: usize) {
    let Some(steps) = read::<PhysicsScript>(store, DbType::PhysicsScript, id) else {
        return;
    };
    println!("    script {:#010X}", id.0);
    for step in &steps.script_data {
        match &step.hook.data {
            HookData::CreateParticle(cp) => {
                println!("      at {:.2} create", step.start_time);
                emitter(store, cp.emitter_info_id, cp.part_index, &cp.offset);
            }
            HookData::CallPes(c) if depth < 4 && c.pes != id => {
                println!(
                    "      at {:.2} calls {:#010X} pause {:.2}",
                    step.start_time, c.pes.0, c.pause
                );
                script(store, c.pes, depth + 1);
            }
            other => println!("      at {:.2} hook {other:?}", step.start_time),
        }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let store =
        std::sync::Arc::new(RetailDatStore::open_dir(std::path::Path::new(&a[1])).expect("dats"));
    if a[2] == "--volumes" {
        volumes(&store);
        return;
    }
    if a[2] == "--setup" {
        for c in &a[3..] {
            let id = DataId(u32::from_str_radix(c.trim_start_matches("0x"), 16).expect("setup"));
            let Some(s) = read::<Setup>(&store, DbType::Setup, id) else {
                println!("{id:?}: no setup");
                continue;
            };
            println!(
                "setup {:#010X} script {:#010X} anim {:#010X} parts {:?}",
                id.0, s.default_script_id.0, s.default_anim_id.0, s.parts
            );
            if let Some(anim) =
                read::<pyreal_assets::Animation>(&store, DbType::Anim, s.default_anim_id)
            {
                println!("    {} frames", anim.num_frames);
                for (f, frame) in anim.part_frames.iter().enumerate() {
                    for h in &frame.hooks {
                        match &h.data {
                            HookData::CreateParticle(cp) => {
                                println!("    frame {f} create (dir {})", h.direction);
                                emitter(&store, cp.emitter_info_id, cp.part_index, &cp.offset);
                            }
                            other => println!("    frame {f} hook (dir {}) {other:?}", h.direction),
                        }
                    }
                }
            }
            script(&store, s.default_script_id, 0);
        }
        return;
    }
    for c in &a[2..] {
        let id = DataId(u32::from_str_radix(c.trim_start_matches("0x"), 16).expect("cell"));
        let Some(cell) = read::<EnvCell>(&store, DbType::Cell, id) else {
            println!("{id:?}: no cell");
            continue;
        };
        println!(
            "cell {:#010X} at {:?} env {:#010X} portals to {:?}",
            id.0,
            cell.frame.origin,
            cell.environment.0,
            cell.portals
                .iter()
                .map(|p| format!("{:#06X}", p.other_cell_id))
                .collect::<Vec<_>>()
        );
        for o in &cell.static_objects {
            println!(
                "  object {:#010X} at {:?} rot {:?}",
                o.id.0, o.frame.origin, o.frame.rotation
            );
            if divine_type(o.id) != Some(DbType::Setup) {
                continue;
            }
            let Some(s) = read::<Setup>(&store, DbType::Setup, o.id) else {
                continue;
            };
            let parts: Vec<String> = s.parts.iter().map(|p| format!("{:#010X}", p.0)).collect();
            println!(
                "    parts [{}] script {:#010X}",
                parts.join(" "),
                s.default_script_id.0
            );
            for (k, l) in &s.lights {
                println!(
                    "    light {k} at {:?} argb {:#010X} intensity {:.2} falloff {:.2} cone {:.2}",
                    l.frame.origin, l.color_argb, l.intensity, l.falloff, l.cone_angle
                );
            }
            for l in pyreal_engine::lamps::examine(&store, o.id) {
                println!("    pyreal lamp {l:?}");
            }
            if let Some(script) =
                read::<PhysicsScript>(&store, DbType::PhysicsScript, s.default_script_id)
            {
                for step in &script.script_data {
                    match &step.hook.data {
                        HookData::CreateParticle(cp) => {
                            emitter(&store, cp.emitter_info_id, cp.part_index, &cp.offset);
                        }
                        other => println!("      at {:.2} hook {other:?}", step.start_time),
                    }
                }
            }
        }
    }
}

/// Each placed static setup whose default script has an emitter `puffs::steam_of` makes a volume
/// of: how often it is placed (cells, landblocks), an example place, and the emitter's opacity.
fn volumes(store: &std::sync::Arc<RetailDatStore>) {
    use pyreal_animation::data::AnimAssets;
    let assets = pyreal_world_data::anim_assets::DatAnimAssets::new(std::sync::Arc::clone(store));
    let mut placed: BTreeMap<u32, (u32, u32, u32)> = BTreeMap::new();
    for id in store.ids_of(DbType::Cell) {
        if id.0 & 0xFFFF < 0x100 {
            continue;
        }
        if let Some(cell) = read::<EnvCell>(store, DbType::Cell, id) {
            for o in &cell.static_objects {
                let e = placed.entry(o.id.0).or_insert((0, 0, id.0));
                e.0 += 1;
            }
        }
    }
    for id in store.ids_of(DbType::Lbi) {
        if let Some(info) = read::<LandblockInfo>(store, DbType::Lbi, id) {
            for o in &info.objects {
                let e = placed.entry(o.id.0).or_insert((0, 0, id.0));
                e.1 += 1;
            }
        }
    }
    for (&id, &(cells, blocks, example)) in &placed {
        if divine_type(DataId(id)) != Some(DbType::Setup) {
            continue;
        }
        let Some(s) = read::<Setup>(store, DbType::Setup, DataId(id)) else {
            continue;
        };
        let Some(script) = read::<PhysicsScript>(store, DbType::PhysicsScript, s.default_script_id)
        else {
            continue;
        };
        for step in &script.script_data {
            let HookData::CreateParticle(cp) = &step.hook.data else {
                continue;
            };
            let Some(e) =
                read::<ParticleEmitterInfo>(store, DbType::ParticleEmitter, cp.emitter_info_id)
            else {
                continue;
            };
            let gid = if e.hw_gfxobj_id.0 != 0 {
                e.hw_gfxobj_id
            } else {
                e.gfxobj_id
            };
            let Some(info) = assets.emitter_info(cp.emitter_info_id) else {
                continue;
            };
            if let Some(k) = pyreal_engine::puffs::placed_kind(store, gid, &info) {
                println!(
                    "{id:#010X} cells {cells:5} blocks {blocks:4} e.g. {example:#010X}: kind {k} emitter {:#010X} ptype {} pic {:#010X} max {} life {:.1} scale {:.1}->{:.1} trans {:.2}->{:.2}",
                    e.id.0, e.particle_type, gid.0, e.max_particles, e.lifespan, e.start_scale, e.final_scale,
                    e.start_trans, e.final_trans
                );
            }
        }
    }
}
