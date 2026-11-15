//! `pyreal-art`: surveys the models the world is built from, and packs art packs.
//!
//! ```text
//! pyreal-art survey [--dat-dir <dir>] [--out <dir>] [--spawns <spawns.tsv>] [--tags <tags.tsv>]
//!                   [--landblocks <XXYY,XXYY,...>] [--threads <n>] [--no-thumbnails] [--pixels]
//! pyreal-art pack <folder> [--out <file>] [--layout 1|2] [--check]
//! pyreal-art mesh <id> [--dat-dir <dir>]
//! pyreal-art open <pack>
//! pyreal-art fingerprint <folder> --dat-dir <dir> [--replace]
//! pyreal-art pictures <list> --dat-dir <dir> --out <dir>
//! pyreal-art skeleton <setup> --out <dir> [--dat-dir <dir>] [--sample]
//! ```
//!
//! `survey`: `--dat-dir` defaults to `PYREAL_TEST_DAT_DIR`, `--out` to `target/art-survey`,
//! `--tags` to `tags.tsv` in the output folder when it is there, and `--threads` to the
//! machine's. `--pixels` also writes every retail picture the world draws to `pictures/` there.
//!
//! `pack`: the art pack in `<folder>` packed into one file, `--out` or `<folder>.dat` beside it;
//! with `--check`, whether that file is what the folder packs into now, and which records are
//! not.
//!
//! `mesh`: the triangles the model `<id>` draws, as JSON on the standard output, by group: its
//! surface, and each corner's position, normal and picture coordinates.
//!
//! `fingerprint`: the fingerprint of the retail record each of the pack's models and materials in
//! `<folder>` was made from, read from the files in `--dat-dir` and written into its manifest, for
//! those that name none (or, with `--replace`, every one); see `docs/ERAS.md`.
//!
//! `pictures`: each picture `<list>` names (a line each: its id, and `clip` where it cuts out),
//! read from the files in `--dat-dir` of either era and written to `--out` as `<id>.png` as the
//! engine decodes it, with each one's fingerprint in `fingerprints.tsv` there: the pictures the
//! pack's materials for an older era's files are worked out from (`docs/ERAS.md`).
//!
//! `skeleton`: the setup `<setup>` written to `--out` as a body to rig against (`body.gltf`, one
//! joint `part_NN` for each part at its resting placement and the parts' triangles on them) with
//! a manifest that makes the folder a pack drawing the setup as that body. `--sample` writes a
//! small body of three boxes with two clips instead, to try the path with. `--dat-dir` defaults
//! to `PYREAL_TEST_DAT_DIR`.
//!
//! `open`: the pack (a packed file or a folder) opened as the client opens it, with what it holds
//! and how long it took.

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "pyreal-art survey [--dat-dir <dir>] [--out <dir>] [--spawns <spawns.tsv>] \
[--tags <tags.tsv>] [--landblocks <XXYY,XXYY,...>] [--threads <n>] [--no-thumbnails] [--pixels]
       pyreal-art pack <folder> [--out <file>] [--layout 1|2] [--check]
       pyreal-art mesh <id> [--dat-dir <dir>]
       pyreal-art open <pack>
       pyreal-art fingerprint <folder> --dat-dir <dir> [--replace]
       pyreal-art pictures <list> --dat-dir <dir> --out <dir>
       pyreal-art skeleton <setup> --out <dir> [--dat-dir <dir>] [--sample]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("survey") => match options(&args[1..]) {
            Ok(opts) => survey(&opts),
            Err(e) => {
                eprintln!("pyreal-art survey: {e}\nusage: {USAGE}");
                ExitCode::from(2)
            }
        },
        Some("pack") => match pack_options(&args[1..]) {
            Ok((dir, out, layout, check)) => pack(&dir, &out, layout, check),
            Err(e) => {
                eprintln!("pyreal-art pack: {e}\nusage: {USAGE}");
                ExitCode::from(2)
            }
        },
        Some("mesh") => mesh(&args[1..]),
        Some("fingerprint") => fingerprint(&args[1..]),
        Some("pictures") => pictures(&args[1..]),
        Some("skeleton") => skeleton(&args[1..]),
        Some("open") => match args.get(1) {
            Some(path) => open(std::path::Path::new(path)),
            None => {
                eprintln!("usage: {USAGE}");
                ExitCode::from(2)
            }
        },
        Some("-h" | "--help") => {
            println!("usage: {USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: {USAGE}");
            ExitCode::from(2)
        }
    }
}

fn options(args: &[String]) -> Result<pyreal_art::survey::Options, String> {
    let mut dat_dir = None;
    let mut out = None;
    let mut spawns = None;
    let mut tags = None;
    let mut landblocks = None;
    let mut threads = None;
    let mut thumbnails = true;
    let mut pixels = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value"))
        };
        match a.as_str() {
            "--dat-dir" => dat_dir = Some(PathBuf::from(value()?)),
            "--out" => out = Some(PathBuf::from(value()?)),
            "--spawns" => spawns = Some(PathBuf::from(value()?)),
            "--tags" => tags = Some(PathBuf::from(value()?)),
            "--landblocks" => {
                let v = value()?;
                let ids = v
                    .split(',')
                    .map(|s| {
                        u16::from_str_radix(s.trim().trim_start_matches("0x"), 16)
                            .map_err(|_| format!("--landblocks wants XXYY ids, got {s:?}"))
                    })
                    .collect::<Result<Vec<u16>, String>>()?;
                landblocks = Some(ids);
            }
            "--threads" => {
                let v = value()?;
                threads = Some(
                    v.parse::<usize>()
                        .ok()
                        .filter(|&n| n > 0)
                        .ok_or_else(|| format!("--threads wants a count, got {v:?}"))?,
                );
            }
            "--no-thumbnails" => thumbnails = false,
            "--pixels" => pixels = true,
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    let dat_dir = match dat_dir {
        Some(d) => d,
        None => std::env::var_os("PYREAL_TEST_DAT_DIR")
            .or_else(|| std::env::var_os("DERETH_TEST_DAT_DIR"))
            .map(PathBuf::from)
            .ok_or("no --dat-dir, and PYREAL_TEST_DAT_DIR is not set")?,
    };
    Ok(pyreal_art::survey::Options {
        dat_dir,
        out: out.unwrap_or_else(|| PathBuf::from("target/art-survey")),
        spawns,
        tags,
        landblocks,
        threads: threads.unwrap_or_else(|| {
            std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get)
        }),
        thumbnails,
        pixels,
    })
}

/// `pack`'s folder, its file, its layout (1 unless `--layout` says) and whether it only checks.
fn pack_options(args: &[String]) -> Result<(PathBuf, PathBuf, u32, bool), String> {
    let mut dir = None;
    let mut out = None;
    let mut layout = pyreal_scene::art_pack::packed::PLAIN;
    let mut check = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => {
                out = Some(PathBuf::from(
                    it.next().ok_or_else(|| format!("{a} needs a value"))?,
                ));
            }
            "--layout" => {
                layout = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or_else(|| format!("{a} needs 1 or 2"))?;
            }
            "--check" => check = true,
            other if dir.is_none() && !other.starts_with("--") => dir = Some(PathBuf::from(other)),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    let dir = dir.ok_or("no folder named")?;
    let out = out.unwrap_or_else(|| pyreal_art::pack::default_out(&dir));
    Ok((dir, out, layout, check))
}

fn pack(dir: &std::path::Path, out: &std::path::Path, layout: u32, check: bool) -> ExitCode {
    use pyreal_art::pack::Freshness;
    let started = std::time::Instant::now();
    if check {
        return match pyreal_art::pack::check(dir, out, layout) {
            Ok(Freshness::Current) => {
                println!("{} is what {} packs into", out.display(), dir.display());
                ExitCode::SUCCESS
            }
            Ok(Freshness::Stale(lines)) => {
                eprintln!(
                    "{} is stale; run pyreal-art pack {}",
                    out.display(),
                    dir.display()
                );
                for line in lines {
                    eprintln!("  {line}");
                }
                ExitCode::FAILURE
            }
            Ok(Freshness::Missing) => {
                eprintln!(
                    "{} is not there; run pyreal-art pack {}",
                    out.display(),
                    dir.display()
                );
                ExitCode::FAILURE
            }
            Err(e) => {
                eprintln!("pyreal-art pack: {e}");
                ExitCode::FAILURE
            }
        };
    }
    match pyreal_art::pack::pack(dir, out, layout) {
        Ok(r) => {
            for problem in &r.problems {
                eprintln!("left out: {problem}");
            }
            println!(
                "wrote {}: the {:?} pack, {} records, {} MB, in {:.1} s",
                r.out.display(),
                r.name,
                r.records,
                r.bytes.div_ceil(1 << 20),
                started.elapsed().as_secs_f64()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("pyreal-art pack: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The files in `dir`, of either era.
fn open_either(dir: &std::path::Path) -> Result<pyreal_dat::RetailDatStore, String> {
    if pyreal_dat::holds_modern_dats(dir) {
        pyreal_dat::RetailDatStore::open_dir(dir)
    } else {
        pyreal_dat::RetailDatStore::open_classic_dir(dir)
    }
    .map_err(|e| format!("{}: {e}", dir.display()))
}

fn skeleton(args: &[String]) -> ExitCode {
    let (mut setup, mut dats, mut out, mut sample) = (None, None, None, false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--dat-dir" => dats = it.next().map(PathBuf::from),
            "--out" => out = it.next().map(PathBuf::from),
            "--sample" => sample = true,
            other if setup.is_none() && !other.starts_with("--") => {
                setup = u32::from_str_radix(other.trim_start_matches("0x"), 16).ok();
            }
            other => {
                eprintln!("pyreal-art skeleton: unknown argument {other:?}\nusage: {USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let dats = dats.or_else(|| std::env::var_os("PYREAL_TEST_DAT_DIR").map(PathBuf::from));
    let (Some(setup), Some(out)) = (setup.map(pyreal_primitives::DataId), out) else {
        eprintln!("usage: {USAGE}");
        return ExitCode::from(2);
    };
    let run = if sample {
        let body = pyreal_scene::art_pack::skin::sample_body(0.5, [0.75, 0.3, 0.2, 1.0]);
        let clips = [("idle", "Sway"), ("walk", "Sway"), ("melee_med", "Nod")];
        pyreal_art::skeleton::write(&out, setup, &body, &clips)
    } else {
        dats.ok_or_else(|| "no --dat-dir and no PYREAL_TEST_DAT_DIR".to_owned())
            .and_then(|d| open_either(&d))
            .and_then(|store| pyreal_art::skeleton::body(&store, setup))
            .and_then(|body| pyreal_art::skeleton::write(&out, setup, &body, &[]))
    };
    match run {
        Ok(()) => {
            println!("{}: the body of {:#010X}", out.display(), setup.0);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("pyreal-art skeleton: {e}");
            ExitCode::FAILURE
        }
    }
}

fn pictures(args: &[String]) -> ExitCode {
    let (mut list, mut dats, mut out) = (None, None, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--dat-dir" => dats = it.next().map(PathBuf::from),
            "--out" => out = it.next().map(PathBuf::from),
            other if list.is_none() && !other.starts_with("--") => {
                list = Some(PathBuf::from(other))
            }
            other => {
                eprintln!("pyreal-art pictures: unknown argument {other:?}\nusage: {USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let (Some(list), Some(dats), Some(out)) = (list, dats, out) else {
        eprintln!("usage: {USAGE}");
        return ExitCode::from(2);
    };
    let run = std::fs::read_to_string(&list)
        .map_err(|e| format!("{}: {e}", list.display()))
        .and_then(|text| {
            let wanted = pyreal_art::fingerprint::list(&text);
            let store = open_either(&dats)?;
            let missing = pyreal_art::fingerprint::export(&store, &wanted, &out)?;
            Ok((wanted.len(), missing))
        });
    match run {
        Ok((wanted, missing)) => {
            for id in &missing {
                eprintln!("not in these files: {id:?}");
            }
            println!(
                "{}: {} pictures written, {} not in these files",
                out.display(),
                wanted - missing.len(),
                missing.len()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("pyreal-art pictures: {e}");
            ExitCode::FAILURE
        }
    }
}

fn fingerprint(args: &[String]) -> ExitCode {
    let (mut dir, mut dats, mut replace) = (None, None, false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--dat-dir" => dats = it.next().map(PathBuf::from),
            "--replace" => replace = true,
            other if dir.is_none() && !other.starts_with("--") => dir = Some(PathBuf::from(other)),
            other => {
                eprintln!("pyreal-art fingerprint: unknown argument {other:?}\nusage: {USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let (Some(dir), Some(dats)) = (dir, dats) else {
        eprintln!("usage: {USAGE}");
        return ExitCode::from(2);
    };
    let store = match open_either(&dats) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("pyreal-art fingerprint: {e}");
            return ExitCode::FAILURE;
        }
    };
    let path = dir.join(pyreal_scene::art_pack::MANIFEST_FILE);
    let run = std::fs::read_to_string(&path)
        .map_err(|e| format!("{}: {e}", path.display()))
        .and_then(|text| {
            pyreal_art::fingerprint::fingerprint(
                &text,
                |id| pyreal_scene::art_pack::fingerprint::of_record(&store, id),
                replace,
            )
        })
        .and_then(|(text, report)| {
            std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(report)
        });
    match run {
        Ok(r) => {
            for line in &r.missing {
                eprintln!("not in these files: {line}");
            }
            println!(
                "{}: {} fingerprints written, {} kept, {} not in these files",
                path.display(),
                r.written,
                r.kept,
                r.missing.len()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("pyreal-art fingerprint: {e}");
            ExitCode::FAILURE
        }
    }
}

fn open(path: &std::path::Path) -> ExitCode {
    let started = std::time::Instant::now();
    match pyreal_scene::art_pack::ArtPack::open_path(path) {
        Ok(p) => {
            let size = |i: &pyreal_scene::art_pack::Image| -> usize {
                i.levels.iter().map(|l| l.len()).sum()
            };
            let models: usize = p.images.iter().chain(&p.grits).map(size).sum();
            let materials: usize = p
                .materials
                .iter()
                .map(|m| m.albedo.as_ref().map_or(0, size) + size(&m.normal) + size(&m.surface))
                .sum();
            let grounds: usize = p
                .grounds
                .iter()
                .map(|g| size(&g.albedo) + size(&g.normal) + size(&g.surface))
                .sum();
            println!(
                "{}: the {:?} pack ({}), {} models, {} pictures ({} MB), {} materials ({} MB), {} grounds ({} MB), \
                 {} kinds of cover, {} bodies, in {:.2} s",
                path.display(),
                p.name,
                p.hash,
                p.models.len(),
                p.images.len(),
                models >> 20,
                p.materials.len(),
                materials >> 20,
                p.grounds.len(),
                grounds >> 20,
                p.ground_cover.len(),
                p.skins.len(),
                started.elapsed().as_secs_f64()
            );
            for problem in &p.problems {
                eprintln!("left out: {problem}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("pyreal-art open: {e}");
            ExitCode::FAILURE
        }
    }
}

fn mesh(args: &[String]) -> ExitCode {
    let id = args.first().and_then(|s| {
        u32::from_str_radix(s.trim_start_matches("0x"), 16)
            .ok()
            .map(pyreal_primitives::DataId)
    });
    let dat_dir = match args.get(1..) {
        Some([flag, dir]) if flag == "--dat-dir" => Some(PathBuf::from(dir)),
        _ => std::env::var_os("PYREAL_TEST_DAT_DIR")
            .or_else(|| std::env::var_os("DERETH_TEST_DAT_DIR"))
            .map(PathBuf::from),
    };
    let (Some(id), Some(dat_dir)) = (id, dat_dir) else {
        eprintln!("usage: {USAGE}");
        return ExitCode::from(2);
    };
    let store = match pyreal_art::survey::open_dats(&dat_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("pyreal-art mesh: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut models = pyreal_art::model::Models::new(&store);
    let Some(mesh) = models.mesh(pyreal_art::tally::ModelId::Object(id), &[]) else {
        eprintln!("pyreal-art mesh: {id:?} does not read");
        return ExitCode::FAILURE;
    };
    let groups: Vec<_> = mesh
        .groups
        .iter()
        .map(|g| {
            let corners: Vec<_> = g
                .triangles
                .iter()
                .flatten()
                .map(|v| (v.position, v.normal, v.uv))
                .collect();
            serde_json::json!({
                "surface": g.surface.map(|s| s.0),
                "two_sided": g.two_sided,
                "corners": corners,
            })
        })
        .collect();
    println!("{}", serde_json::Value::Array(groups));
    ExitCode::SUCCESS
}

fn survey(opts: &pyreal_art::survey::Options) -> ExitCode {
    let log = |s: &str| eprintln!("{s}");
    match pyreal_art::survey::run(opts, &log) {
        Ok(s) => {
            println!(
                "surveyed {} landblocks and {} models in {:.0} s; wrote {}",
                s.tally.world.landblocks,
                s.tally.uses.len(),
                s.seconds,
                opts.out.join("report.html").display()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("pyreal-art survey: {e}");
            ExitCode::FAILURE
        }
    }
}
