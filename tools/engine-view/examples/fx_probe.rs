//! Lists a setup's particle hooks: its default script's, its motion table's animations' and its
//! physics script table's, with each emitter's slot id, part and picture.
//!
//! Usage: `fx_probe DAT_DIR SETUP[:PHS_TABLE[:MOTION_TABLE]]...`

use std::sync::Arc;

use pyreal_animation::data::AnimAssets;
use pyreal_animation::hooks::HookKind;
use pyreal_dat::RetailDatStore;
use pyreal_primitives::DataId;

fn hooks(a: &dyn AnimAssets, kind: &HookKind, at: &str, depth: u32) {
    match kind {
        HookKind::CreateParticle(c) | HookKind::CreateBlockingParticle(c) => {
            let e = a.emitter_info(c.emitter_info_id);
            let (spr, info) = e.as_ref().map_or((0, String::new()), |e| {
                (
                    pyreal_engine::particles::sprite_of(e).0,
                    format!(
                        "etype {} rate {:.3} max {} life {:.2} scale {:.2}->{:.2} trans {:.2}->{:.2} local {} total {}/{:.1} type {:?}",
                        e.emitter_type,
                        e.birthrate,
                        e.max_particles,
                        e.lifespan,
                        e.start_scale,
                        e.final_scale,
                        e.start_trans,
                        e.final_trans,
                        e.is_parent_local,
                        e.total_particles,
                        e.total_seconds,
                        e.particle_type
                    ),
                )
            });
            let blocking = matches!(kind, HookKind::CreateBlockingParticle(_));
            println!(
                "{at}  create{} slot {} part {} info {:#010X} sprite {spr:#010X} {info}",
                if blocking { "(blocking)" } else { "" },
                c.emitter_id,
                c.part_index,
                c.emitter_info_id.0
            );
        }
        HookKind::DestroyParticle { emitter_id } => println!("{at}  destroy slot {emitter_id}"),
        HookKind::StopParticle { emitter_id } => println!("{at}  stop slot {emitter_id}"),
        HookKind::DefaultScript => println!("{at}  default script"),
        HookKind::DefaultScriptPart { part_index } => {
            println!("{at}  default script part {part_index}")
        }
        HookKind::CallPes(p) if depth < 3 => {
            println!("{at}  call {:#010X} after {:.2}", p.pes.0, p.pause);
            script(a, p.pes, &format!("{at}>"), depth + 1);
        }
        _ => {}
    }
}

fn script(a: &dyn AnimAssets, id: DataId, at: &str, depth: u32) {
    let Some(s) = a.script(id) else {
        println!("{at}  script {:#010X} will not read", id.0);
        return;
    };
    for step in &s.steps {
        hooks(
            a,
            &step.hook.kind,
            &format!("{at} {:#010X}@{:.2}", id.0, step.start_time),
            depth,
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let store = Arc::new(RetailDatStore::open_dir(std::path::Path::new(&args[1])).expect("dats"));
    let a: Arc<dyn AnimAssets> = Arc::new(pyreal_world_data::anim_assets::DatAnimAssets::new(
        Arc::clone(&store),
    ));
    for s in &args[2..] {
        let hex =
            |t: &str| DataId(u32::from_str_radix(t.trim_start_matches("0x"), 16).expect("hex"));
        let mut it = s.split(':');
        let id = hex(it.next().expect("setup"));
        let phs = it.next().map(hex);
        let table = it.next().map(hex);
        let Some(setup) = a.setup(id) else {
            println!("{s}: no setup");
            continue;
        };
        println!(
            "== {s}: parts {} script {:?} table {:?} phs {:?}",
            setup.parts.len(),
            setup.default_script,
            setup.default_motion_table,
            setup.default_phs_table
        );
        if let Some(d) = setup.default_script {
            script(a.as_ref(), d, "default", 0);
        }
        if let Some(t) = phs
            .or(setup.default_phs_table)
            .and_then(|t| a.script_table(t))
        {
            for (k, list) in &t.scripts {
                for m in list {
                    println!(
                        "phs {k:#X} mod {:.2} -> {:#010X}",
                        m.modifier, m.script_id.0
                    );
                    script(a.as_ref(), m.script_id, &format!("phs {k:#X}"), 0);
                }
            }
        }
        if let Some(t) = table
            .or(setup.default_motion_table)
            .and_then(|t| a.motion_table(t))
        {
            let mut seen = std::collections::BTreeSet::new();
            let mut all: Vec<(String, &pyreal_animation::data::MotionData)> = Vec::new();
            for (k, m) in &t.cycles {
                all.push((format!("cycle {k:#010X}"), m));
            }
            for (k, m) in &t.modifiers {
                all.push((format!("modifier {k:#010X}"), m));
            }
            for (k, l) in &t.links {
                for (to, m) in l {
                    all.push((format!("link {k:#010X}->{to:#010X}"), m));
                }
            }
            for (name, m) in all {
                for ad in &m.anims {
                    let Some(an) = a.animation(ad.anim_id) else {
                        continue;
                    };
                    for (f, fr) in an.part_frames.iter().enumerate() {
                        for h in &fr.hooks {
                            if matches!(
                                h.kind,
                                HookKind::CreateParticle(_)
                                    | HookKind::CreateBlockingParticle(_)
                                    | HookKind::DestroyParticle { .. }
                                    | HookKind::StopParticle { .. }
                                    | HookKind::DefaultScript
                                    | HookKind::DefaultScriptPart { .. }
                                    | HookKind::CallPes(_)
                            ) && seen.insert((ad.anim_id.0, f, format!("{:?}", h.kind)))
                            {
                                hooks(
                                    a.as_ref(),
                                    &h.kind,
                                    &format!("{name} anim {:#010X} f{f}", ad.anim_id.0),
                                    0,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
