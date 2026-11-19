//! Creatures and characters placed for testing: a setup (with its own motion table or one named
//! beside it), or an Aluvian man or woman dressed as character creation dresses one, posed by the
//! engine's bodies mid-motion.

use std::sync::Arc;

use pyreal_animation::data::AnimAssets;
use pyreal_animation::parts::ObjDesc;
use pyreal_animation::MotionCommand;
use pyreal_dat::RetailDatStore;
use pyreal_engine::body::Body;
use pyreal_engine::world::AssetLoader;
use pyreal_engine::Engine;
use pyreal_primitives::{DataId, Frame};

/// What a body is made from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A setup, and the motion table it moves by when not its own.
    Setup(u32, Option<u32>),
    /// An Aluvian dressed by character creation: `true` for a woman.
    Character(bool),
}

/// One body to place: what it is and, block-local in the view's block, where (in front of the
/// camera when not given).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spec {
    pub kind: Kind,
    pub at: Option<(f32, f32, Option<f32>)>,
    /// The object's scale, as a creature's weenie gives it (1 its setup's own size).
    pub scale: f32,
}

fn hex(s: &str) -> Result<u32, String> {
    let t = s.trim().trim_start_matches("0x").trim_start_matches("0X");
    u32::from_str_radix(t, 16).map_err(|e| format!("{s}: {e}"))
}

fn place(s: &str) -> Result<Option<(f32, f32, Option<f32>)>, String> {
    let n = |v: &str| v.trim().parse::<f32>().map_err(|e| format!("{v}: {e}"));
    let p: Vec<&str> = s.split(',').collect();
    match p.as_slice() {
        [x, y] => Ok(Some((n(x)?, n(y)?, None))),
        [x, y, z] => Ok(Some((n(x)?, n(y)?, Some(n(z)?)))),
        _ => Err(format!("{s}: a place is X,Y or X,Y,Z")),
    }
}

/// `SETUP[/MOTION_TABLE][*SCALE][@X,Y[,Z]]`.
pub fn object(spec: &str) -> Result<Spec, String> {
    let (what, at) = spec
        .split_once('@')
        .map_or((spec, None), |(w, a)| (w, Some(a)));
    let (what, scale) = what
        .split_once('*')
        .map_or((what, None), |(w, s)| (w, Some(s)));
    let (setup, table) = what
        .split_once('/')
        .map_or((what, None), |(s, t)| (s, Some(t)));
    let scale = match scale {
        Some(s) => s
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|s| s.is_finite() && *s > 0.0)
            .ok_or(format!("{s}: a scale is a number over 0"))?,
        None => 1.0,
    };
    Ok(Spec {
        kind: Kind::Setup(hex(setup)?, table.map(hex).transpose()?),
        at: at.map(place).transpose()?.flatten(),
        scale,
    })
}

/// `male|female[@X,Y[,Z]]`.
pub fn character(spec: &str) -> Result<Spec, String> {
    let (what, at) = spec
        .split_once('@')
        .map_or((spec, None), |(w, a)| (w, Some(a)));
    let woman = match what.trim() {
        "male" | "man" => false,
        "female" | "woman" => true,
        other => return Err(format!("--character takes male or female, not {other}")),
    };
    Ok(Spec {
        kind: Kind::Character(woman),
        at: at.map(place).transpose()?.flatten(),
        scale: 1.0,
    })
}

/// A setup dressed as the server dresses a creature: clothing base, palette template, shade and
/// base palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dress {
    pub clothing: u32,
    pub template: u32,
    pub shade: f64,
    pub palette: u32,
}

/// `CLOTHING,TEMPLATE,SHADE[,PALETTE]`.
pub fn dress(spec: &str) -> Result<Dress, String> {
    let p: Vec<&str> = spec.split(',').collect();
    let (clothing, template, shade, palette) = match p.as_slice() {
        [c, t, s, p] => (c, t, s, *p),
        [c, t, s] => (c, t, s, "0"),
        _ => {
            return Err(format!(
                "{spec}: a dress is CLOTHING,TEMPLATE,SHADE[,PALETTE]"
            ))
        }
    };
    Ok(Dress {
        clothing: hex(clothing)?,
        template: template
            .trim()
            .parse()
            .map_err(|e| format!("{template}: {e}"))?,
        shade: shade.trim().parse().map_err(|e| format!("{shade}: {e}"))?,
        palette: hex(palette)?,
    })
}

/// A motion command by its name (`WalkForward`, `HandCombat`, …).
pub fn command(name: &str) -> Result<MotionCommand, String> {
    MotionCommand::from_name(name).ok_or(format!("no motion command named {name}"))
}

/// An Aluvian man or woman as character creation dresses one from a fixed roll of its random
/// draws: the setup, its object description and its sex's motion table.
fn dressed(store: &RetailDatStore, woman: bool) -> Option<(DataId, ObjDesc, DataId)> {
    use pyreal_assets::Decode;
    let tables = pyreal_chargen::CreationTables::read(store)?;
    let cg = &tables.chargen;
    let mut state = pyreal_chargen::CharGenState::default();
    state.clothing = std::rc::Rc::clone(&tables.clothing);
    let seed = if woman { 9 } else { 5 };
    state.rng = pyreal_chargen::CharGenRng::new(seed, seed.unsigned_abs());
    state.reset(cg, &tables.skills);
    state.set_heritage_group(cg, &tables.skills, 1);
    state.set_gender(cg, if woman { 2 } else { 1 });
    state.randomize_appearance(cg, false);
    state.randomize_clothing(cg, false);
    let setup = state.get_setup_id(cg);
    let table = state.sex(cg).map_or(DataId(0), |s| s.motion_table);
    let mut palettes = |id: DataId| -> Vec<DataId> {
        let read = || -> Option<Vec<DataId>> {
            let bytes = pyreal_primitives::AssetSource::read(store, id).ok()?;
            Some(
                pyreal_assets::material::PaletteSet::decode_payload_in(
                    store.era_of(id),
                    id,
                    &bytes,
                )
                .ok()?
                .palette_ids,
            )
        };
        read().unwrap_or_default()
    };
    let (od, _) =
        pyreal_chargen::dress::chargen_objdesc(cg, &state, &tables.clothing, setup, &mut palettes);
    Some((setup, od, table))
}

/// Setup `setup`'s object description in `dress`, each layer over the last as the server dresses a
/// creature in what it wears; the first layer's palette is the base palette.
fn dressed_setup(store: &RetailDatStore, setup: DataId, dress: &[Dress]) -> Option<ObjDesc> {
    let mut od = ObjDesc {
        palette_id: DataId(dress.first()?.palette),
        ..ObjDesc::default()
    };
    for d in dress {
        if !dress_layer(store, setup, *d, &mut od) {
            println!("{:#010X}: the dress will not build", d.clothing);
            return None;
        }
    }
    let parts: Vec<String> = od
        .part_changes
        .iter()
        .map(|c| format!("{}={}", c.part_index, c.part_id))
        .collect();
    println!(
        "dress: parts [{}], {} texture changes, {} subpalettes",
        parts.join(" "),
        od.texture_changes.len(),
        od.subpalettes.len()
    );
    Some(od)
}

/// Setup `setup` in the items `worn`, as the server dresses a creature in its equipment: each item
/// over the last, the parts none covers its own (`own`, all but the head); with none covered, `dress`.
fn worn_setup(
    store: &RetailDatStore,
    setup: DataId,
    own: &[DataId],
    dress: &[Dress],
    worn: &[Dress],
) -> Option<ObjDesc> {
    use pyreal_assets::Decode;
    let mut od = ObjDesc {
        palette_id: DataId(dress.first().or(worn.first())?.palette),
        ..ObjDesc::default()
    };
    let mut covered = Vec::new();
    for d in worn {
        let id = DataId(d.clothing);
        let Some(table) = pyreal_primitives::AssetSource::read(store, id)
            .ok()
            .and_then(|b| {
                pyreal_assets::ClothingTable::decode_payload_in(store.era_of(id), id, &b).ok()
            })
        else {
            continue;
        };
        let Some(base) = table.clothing_bases.get(&setup) else {
            continue;
        };
        covered.extend(base.iter().map(|e| e.part_num));
        // A template the table lacks is its first, as the server takes it.
        let template = if table.palette_templates.contains_key(&d.template) {
            d.template
        } else {
            table.palette_templates.keys().next().copied().unwrap_or(0)
        };
        let mut palettes = |id: DataId| -> Vec<DataId> {
            pyreal_primitives::AssetSource::read(store, id)
                .ok()
                .and_then(|b| {
                    pyreal_assets::material::PaletteSet::decode_payload_in(store.era_of(id), id, &b)
                        .ok()
                })
                .map(|s| s.palette_ids)
                .unwrap_or_default()
        };
        pyreal_chargen::dress::build_obj_desc(
            &table,
            setup,
            template,
            d.shade,
            &mut palettes,
            &mut od,
        );
    }
    if covered.is_empty() {
        return if dress.is_empty() {
            Some(od)
        } else {
            dressed_setup(store, setup, dress)
        };
    }
    for (k, part) in (0u32..).zip(own) {
        if !covered.contains(&k) && k != 0x10 {
            od.part_changes
                .push(pyreal_animation::parts::AnimPartChange {
                    part_index: k,
                    part_id: *part,
                });
        }
    }
    println!(
        "worn: {} part changes, {} texture changes, {} subpalettes",
        od.part_changes.len(),
        od.texture_changes.len(),
        od.subpalettes.len()
    );
    Some(od)
}

/// Lays clothing `d` over `od`; with no entry for the setup only its palette template, as the
/// server dresses one.
fn dress_layer(store: &RetailDatStore, setup: DataId, d: Dress, od: &mut ObjDesc) -> bool {
    use pyreal_assets::Decode;
    let id = DataId(d.clothing);
    let Some(mut table) = pyreal_primitives::AssetSource::read(store, id)
        .ok()
        .and_then(|b| {
            pyreal_assets::ClothingTable::decode_payload_in(store.era_of(id), id, &b).ok()
        })
    else {
        return false;
    };
    table.clothing_bases.entry(setup).or_default();
    let mut palettes = |id: DataId| -> Vec<DataId> {
        let read = || -> Option<Vec<DataId>> {
            let bytes = pyreal_primitives::AssetSource::read(store, id).ok()?;
            Some(
                pyreal_assets::material::PaletteSet::decode_payload_in(
                    store.era_of(id),
                    id,
                    &bytes,
                )
                .ok()?
                .palette_ids,
            )
        };
        read().unwrap_or_default()
    };
    pyreal_chargen::dress::build_obj_desc(&table, setup, d.template, d.shade, &mut palettes, od)
}

/// Makes the body `spec` names, standing at `frame` (render space), in `style` doing `motion`.
pub fn make(
    store: &Arc<RetailDatStore>,
    spec: Spec,
    frame: Frame,
    style: Option<MotionCommand>,
    motion: Option<MotionCommand>,
    dress: &[Dress],
    worn: &[Dress],
) -> Result<Body, String> {
    let assets: Arc<dyn AnimAssets> = Arc::new(pyreal_world_data::anim_assets::DatAnimAssets::new(
        Arc::clone(store),
    ));
    let (setup, od, table) = match spec.kind {
        Kind::Setup(s, t) => {
            let od = if !worn.is_empty() {
                let own = assets
                    .setup(DataId(s))
                    .map(|d| d.parts.clone())
                    .unwrap_or_default();
                Some(
                    worn_setup(store, DataId(s), &own, dress, worn)
                        .ok_or("the dress will not build")?,
                )
            } else if dress.is_empty() {
                None
            } else {
                Some(dressed_setup(store, DataId(s), dress).ok_or("the dress will not build")?)
            };
            (DataId(s), od, t.map(DataId))
        }
        Kind::Character(woman) => {
            let (s, od, t) = dressed(store, woman).ok_or("the creation tables will not read")?;
            (s, Some(od), Some(t))
        }
    };
    let scripted = assets
        .setup(setup)
        .is_some_and(|d| d.default_script.is_some());
    let mut body = Body::new(Arc::clone(&assets), setup, od.as_ref(), frame).ok_or(format!(
        "{setup}: the setup or one of its parts will not load"
    ))?;
    if let Some(t) = table.filter(|t| t.0 != 0) {
        if !body.driver.set_motion_table(t) {
            return Err(format!("{t}: the motion table will not load"));
        }
    }
    // The object's scale on the driver and its parts, as the client sets a described one, before
    // the script fills its emitters.
    if (spec.scale - 1.0).abs() > f32::EPSILON {
        body.driver.scale = spec.scale;
        let s = pyreal_primitives::Vec3::new(spec.scale, spec.scale, spec.scale);
        body.driver.part_array.set_scale_internal(s);
    }
    // Its default script (queued by its setup; a water golem's body is its emitters), run in its
    // idle long enough to fill them along the path its parts take.
    if scripted {
        run(&mut body, SCRIPT_PREROLL);
    }
    for c in [style, motion].into_iter().flatten() {
        let err = body.command(c);
        if err != 0 {
            println!(
                "{setup}: {} refused with error {err:#04X}",
                c.name().unwrap_or("the command")
            );
        }
    }
    Ok(body)
}

/// Seconds a body's default script runs before the first frame is drawn.
const SCRIPT_PREROLL: f64 = 3.0;

/// Advances a body `seconds` with its scripts and emitters stepped along the way, so a moving
/// emitter's trail has no gap.
pub fn run(b: &mut Body, seconds: f64) {
    let mut left = seconds;
    while left > 1e-9 {
        let q = left.min(1.0 / 30.0);
        b.advance(q);
        run_scripts(b);
        left -= q;
    }
}

/// One step of a body's scripts and emitters, after its animation.
fn run_scripts(b: &mut Body) {
    b.driver.update_parts(&b.frame);
    b.driver.update_scripts();
    b.driver.update_particles(true);
    let _ = b.driver.take_events();
    let _ = b.driver.take_effects();
}

/// Hands the bodies' live particles to the next frame, their pictures made ready first, and
/// places their lamps where they stand after the first `statics` of the engine's, as the client
/// places a creature's: its setup's, less those its emitters carry. Seen from `eye`.
pub fn gather(
    engine: &mut Engine,
    loader: &mut AssetLoader,
    store: &RetailDatStore,
    bodies: &[Body],
    statics: usize,
    eye: glam::Vec3,
) {
    engine.lamps.truncate(statics);
    for b in bodies {
        if !b.driver.particles.is_empty() {
            for g in engine.particles.missing_sprites(&b.driver) {
                pyreal_engine::particles::prepare_sprite(engine, loader, store, g);
            }
            engine
                .particles
                .gather(&b.driver, b.cell, glam::Vec3::ZERO, eye, b.shadow, false);
        }
        if b.setup.0 != 0 {
            let at = pyreal_engine::asset::object::frame_affine(&b.frame);
            let lamps = loader.lamps_at(store, b.setup, &at, b.cell, b.object, true);
            let lamps = engine.particles.setup_lamps(&b.driver, lamps);
            engine.lamps.extend(lamps);
        }
    }
}

/// The room holding `p`, one of the view's `block` before another block's dungeon laid over it.
fn cell_at(engine: &Engine, p: glam::Vec3, block: u16) -> u32 {
    let cells = &engine.cells;
    cells
        .cells()
        .into_iter()
        .find(|c| {
            c.id >> 16 == u32::from(block) && c.volume.as_ref().is_some_and(|v| v.contains(p, 0.0))
        })
        .and_then(|c| cells.slot_of(c.id))
        .or_else(|| cells.cell_at(p))
        .unwrap_or(pyreal_engine::cells::OUTSIDE)
}

/// Places `bodies` into the engine, each posed at its current time; the first `named` (the ones
/// the command line asked for) are reported one by one, the rest together.
pub fn place_all(
    engine: &mut Engine,
    loader: &mut AssetLoader,
    store: &RetailDatStore,
    bodies: &mut [Body],
    named: usize,
    block: u16,
) {
    let mut rest = (0, 0);
    for (k, b) in bodies.iter_mut().enumerate() {
        let o = b.frame.origin;
        let cell = cell_at(engine, glam::Vec3::new(o.x, o.y, o.z + 0.5), block);
        b.cell = cell;
        let n = b.place(engine, loader, store);
        b.draw_skin(engine, loader);
        if k >= named {
            rest = (rest.0 + 1, rest.1 + n);
            continue;
        }
        let o = b.frame.origin;
        let setup = b
            .driver
            .part_array
            .setup
            .as_ref()
            .map_or(0, |s| s.parts.len());
        println!(
            "body at [{:.1}, {:.1}, {:.1}]: {setup} parts, {n} drawn{}{}",
            o.x,
            o.y,
            o.z,
            if b.shadow { ", a shadow" } else { "" },
            if b.skinned.is_some() {
                ", the art pack's body"
            } else {
                ""
            },
        );
    }
    if rest.0 > 0 {
        println!(
            "{} of the place's objects stand as bodies, {} parts drawn",
            rest.0, rest.1
        );
    }
}

/// Advances every body `seconds` and moves its parts, placing again a body whose hidden part
/// shows.
pub fn advance(
    engine: &mut Engine,
    loader: &mut AssetLoader,
    store: &RetailDatStore,
    bodies: &mut [Body],
    seconds: f64,
) {
    for b in bodies.iter_mut() {
        b.advance(seconds);
        run_scripts(b);
        if b.update(&mut engine.scene) {
            b.place(engine, loader, store);
        }
        b.draw_skin(engine, loader);
    }
    stand(engine, bodies);
}

/// Tells the engine where the bodies stand, for the water, the grass and the cobwebs, and where
/// the shadows among them pool their darkness.
pub fn stand(engine: &mut Engine, bodies: &[Body]) {
    let feet = |b: &Body| glam::Vec3::new(b.frame.origin.x, b.frame.origin.y, b.frame.origin.z);
    engine.bodies = bodies.iter().map(feet).collect();
    engine.umbrae = bodies.iter().filter(|b| b.shadow).map(feet).collect();
}
