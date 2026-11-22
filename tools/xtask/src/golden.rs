//! `cargo xtask golden [--update] [--only SLUG,...]`: the drawn world compared with retail, tier 2's
//! retail comparisons.
//!
//! Each [`SCENES`] entry is a place in the report library, drawn by engine-view twice from the same
//! camera at the same time of day: with retail's look (the screenshot library's retail flags) and
//! with Pyreal's over the base pack. Two checks read them:
//!
//! - **Retail golden images.** The retail-look picture must match a stored golden one within
//!   [`PIXEL_TOLERANCE`], [`MAX_DIFFERING`] and [`MAX_MEAN_ERROR`]. The goldens are drawn from the
//!   retail dats, so they never enter the repository: they live under `PYREAL_GOLDEN_DIR`, or a
//!   `Golden` folder beside the retail dats' folder, one folder per graphics adapter. `--update`
//!   draws them again.
//! - **Look drift.** Pyreal's picture is measured by coarse measures in CIE L\*a\*b\*: the frame's
//!   mean lightness, each of a 3 by 3 grid's, the colour cast and the lightness histogram. Against
//!   the retail picture each must stay under [`CEILING`], which only a broken picture reaches.
//!   Against the look accepted at the last `--update`, kept with the goldens, each must stay within
//!   [`BAND`]: a change that makes the world much darker, brighter or bluer fails until it is looked
//!   at beside retail and accepted.
//!
//! Each run's pictures, and a difference picture for a golden that failed, are left in
//! `<target>/golden/` to look at.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use crate::util::{
    retail_data_available, target_dir, workspace_root, NotRun, Outcome, Profile, Report,
};

/// One place drawn by both checks.
pub struct Scene {
    pub slug: &'static str,
    place: Place,
    /// Outdoors the retail side draws no lamps, as the screenshot library's does: engine-view has
    /// no copy of the original's lamp light on the land.
    outdoors: bool,
    /// More engine-view flags for both sides.
    flags: &'static [&'static str],
}

/// Where a scene's camera is.
enum Place {
    /// A place in the report library, by name.
    Library(&'static str),
    /// A report line of its own.
    Report(&'static str),
}

/// The scenes.
pub const SCENES: &[Scene] = &[
    Scene {
        slug: "holtburg-day",
        place: Place::Library("clip-holtburg-1"),
        outdoors: true,
        flags: &["--time", "0.5"],
    },
    Scene {
        slug: "dungeon-hall",
        place: Place::Library("dungeon-bricks-1"),
        outdoors: false,
        flags: &[],
    },
    Scene {
        slug: "house-room",
        place: Place::Report(
            "dereth report 1; loc 0xA9B40108 [78.0929 137.54202 66.005005] 1 0 0 0; camera \
             0xA9B4 [78.09 138.6 67.6] yaw 3.14159 pitch -0.12 cell 0xA9B40108; fov 90 vertical \
             58.343605 aspect 1; time 0.556 Midsong, set here; clock 307749578.947332; weather clear",
        ),
        outdoors: false,
        flags: &[],
    },
    Scene {
        slug: "forest",
        place: Place::Library("trees-unconverted-1"),
        outdoors: true,
        flags: &["--time", "0.45"],
    },
    Scene {
        slug: "sea-shore",
        place: Place::Library("playtest-beach-water"),
        outdoors: true,
        flags: &[],
    },
    Scene {
        slug: "night-lamp",
        place: Place::Library("playtest2-night-lamp"),
        outdoors: true,
        flags: &[],
    },
    Scene {
        slug: "sunlit-court",
        place: Place::Library("clip-f418-sunlight"),
        outdoors: true,
        flags: &[],
    },
    Scene {
        slug: "character",
        place: Place::Report(
            "dereth report 1; loc 0xA9B4002A [140.149353 24.384995 94.005005] 1 0 0 0; camera \
             0xA9B4 [143.5 21.4 95.7] yaw -5.43745 pitch -0.05 cell 0xA9B4002A; fov 90 vertical \
             58.343605 aspect 1; time 0.485 Midsong, set here; clock 307749578.947332; weather clear",
        ),
        outdoors: true,
        flags: &["--character", "male@142.4,22.3;facing=131.5"],
    },
];

/// The report library the scenes name.
const LIBRARY: &str = "pyreal/client/tests/gpu/report-library.txt";

/// The retail look: no pack, the client's own light and every Pyreal addition off, as
/// `tools/screenshots/capture.py` draws its retail side.
const RETAIL: &[&str] = &[
    "--retail",
    "--no-surfaces",
    "--no-shadows",
    "--no-ao",
    "--no-bounce",
    "--no-cover",
    "--no-lamp-rays",
    "--no-water",
    "--clouds",
    "0",
    "--no-volumes",
    "--no-webs",
    "--no-face-detail",
    "--no-weather",
    "--lod-bias",
    "0",
];

/// Pyreal's look: the viewer's own settings over the base pack, the eye adapting as the client's
/// does, every model at its finest level so a still does not catch two levels crossfading.
const PYREAL: &[&str] = &["--lod-bias", "0"];

/// Both sides' picture size and frames.
const SIZE: &str = "640x360";
const FRAMES: &str = "30";

/// A pixel differs from its golden when a channel is further off than this, of 255.
pub const PIXEL_TOLERANCE: u8 = 16;
/// The share of a picture's pixels that may differ from the golden.
pub const MAX_DIFFERING: f64 = 0.002;
/// The mean difference from the golden, of 255, over every channel of every pixel.
pub const MAX_MEAN_ERROR: f64 = 0.25;

/// How far Pyreal's look may stray from retail's, in CIE L\*a\*b\* units (L\* runs 0 to 100).
#[derive(Debug, Clone, Copy)]
pub struct DriftLimits {
    /// The frame's mean lightness.
    pub frame_lightness: f64,
    /// Any ninth of the frame's mean lightness.
    pub region_lightness: f64,
    /// The frame's mean colour, as the distance between the two means in a\* and b\*.
    pub cast: f64,
    /// The lightness histograms' earth mover's distance.
    pub histogram: f64,
}

/// How far Pyreal's look may ever be from retail's: only a broken picture is this far.
pub const CEILING: DriftLimits = DriftLimits {
    frame_lightness: 30.0,
    region_lightness: 50.0,
    cast: 15.0,
    histogram: 35.0,
};

/// How far Pyreal's look may move from the one accepted with `--update`.
pub const BAND: DriftLimits = DriftLimits {
    frame_lightness: 4.0,
    region_lightness: 10.0,
    cast: 4.0,
    histogram: 5.0,
};

/// Where the goldens are kept: `PYREAL_GOLDEN_DIR`, else `Golden` beside the retail dats' folder.
pub fn golden_root(dats: &Path) -> PathBuf {
    match std::env::var_os("PYREAL_GOLDEN_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => dats.parent().unwrap_or(dats).join("Golden"),
    }
}

/// An adapter's name as a folder name.
fn adapter_folder(adapter: &str) -> String {
    let s: String = adapter
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    s.split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// The engine-view flags of one side of a scene, without the dats and the output.
fn side_flags(scene: &Scene, retail: bool, base_pack: &Path) -> Vec<String> {
    let place: &[&str] = match &scene.place {
        Place::Library(name) => &["--library", LIBRARY, "--name", name],
        Place::Report(line) => &["--report", line],
    };
    let mut v: Vec<String> = place
        .iter()
        .chain(&["--size", SIZE, "--frames", FRAMES])
        .map(|s| (*s).to_owned())
        .collect();
    v.extend(scene.flags.iter().map(|s| (*s).to_owned()));
    if retail {
        v.extend(RETAIL.iter().map(|s| (*s).to_owned()));
        if scene.outdoors {
            v.push("--no-lamps".to_owned());
        }
    } else {
        v.push("--base-pack".to_owned());
        v.push(base_pack.display().to_string());
        v.extend(PYREAL.iter().map(|s| (*s).to_owned()));
    }
    v
}

/// The golden's record of how it was drawn: the retail side's flags.
fn retail_key(scene: &Scene) -> String {
    side_flags(scene, true, Path::new("")).join(" ")
}

/// An RGBA picture.
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub fn read_png(path: &Path) -> Result<Picture, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if info.bit_depth != png::BitDepth::Eight || info.color_type != png::ColorType::Rgba {
        return Err(format!("{}: not 8-bit RGBA", path.display()));
    }
    buf.truncate(info.buffer_size());
    Ok(Picture {
        width: info.width,
        height: info.height,
        rgba: buf,
    })
}

fn write_png(path: &Path, p: &Picture) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), p.width, p.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&p.rgba).map_err(|e| e.to_string())
}

/// How far a picture is from its golden.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GoldenDiff {
    /// The share of pixels with a channel more than [`PIXEL_TOLERANCE`] off.
    pub differing: f64,
    /// The mean channel difference, of 255.
    pub mean: f64,
}

impl GoldenDiff {
    pub fn passes(self) -> bool {
        self.differing <= MAX_DIFFERING && self.mean <= MAX_MEAN_ERROR
    }
}

/// `drawn` against `golden`, and the difference as a picture (four times as bright).
pub fn golden_diff(drawn: &Picture, golden: &Picture) -> Result<(GoldenDiff, Picture), String> {
    if (drawn.width, drawn.height) != (golden.width, golden.height) {
        return Err(format!(
            "drawn {}x{}, golden {}x{}",
            drawn.width, drawn.height, golden.width, golden.height
        ));
    }
    let mut differing = 0usize;
    let mut total = 0u64;
    let mut out = Vec::with_capacity(drawn.rgba.len());
    for (a, b) in drawn
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(golden.rgba.as_chunks::<4>().0)
    {
        let d = [0, 1, 2].map(|c| a[c].abs_diff(b[c]));
        total += d.iter().map(|&x| u64::from(x)).sum::<u64>();
        if d.iter().any(|&x| x > PIXEL_TOLERANCE) {
            differing += 1;
        }
        out.extend(d.map(|x| x.saturating_mul(4)));
        out.push(255);
    }
    let pixels = (drawn.rgba.len() / 4).max(1);
    #[allow(clippy::cast_precision_loss)]
    let diff = GoldenDiff {
        differing: differing as f64 / pixels as f64,
        mean: total as f64 / (pixels * 3) as f64,
    };
    let picture = Picture {
        width: drawn.width,
        height: drawn.height,
        rgba: out,
    };
    Ok((diff, picture))
}

/// The coarse measures of one picture, in CIE L\*a\*b\* with sRGB's white.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub lightness: f64,
    pub a: f64,
    pub b: f64,
    /// Each ninth of the frame's mean lightness, row by row from the top left.
    pub regions: [f64; 9],
    /// The lightness histogram, a share of the pixels per unit of L\*.
    pub histogram: [f64; 100],
}

/// sRGB's 8-bit values made linear.
#[allow(clippy::disallowed_methods)] // a measure of a picture, not game state
fn linear(v: u8) -> f64 {
    let c = f64::from(v) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// CIE L\*a\*b\* of linear sRGB, with its white.
#[allow(clippy::disallowed_methods)] // a measure of a picture, not game state
fn lab(r: f64, g: f64, b: f64) -> [f64; 3] {
    let x = (0.412_456_4 * r + 0.357_576_1 * g + 0.180_437_5 * b) / 0.950_47;
    let y = 0.212_672_9 * r + 0.715_152_2 * g + 0.072_175 * b;
    let z = (0.019_333_9 * r + 0.119_192 * g + 0.950_304_1 * b) / 1.088_83;
    let f = |t: f64| {
        if t > 216.0 / 24389.0 {
            t.cbrt()
        } else {
            (24389.0 / 27.0 * t + 16.0) / 116.0
        }
    };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

pub fn look(p: &Picture) -> Look {
    let table: Vec<f64> = (0..=255u8).map(linear).collect();
    let (w, h) = (p.width as usize, p.height as usize);
    let mut sum = [0.0f64; 3];
    let mut regions = [0.0f64; 9];
    let mut counts = [0usize; 9];
    let mut histogram = [0.0f64; 100];
    for (i, px) in p.rgba.as_chunks::<4>().0.iter().enumerate() {
        let [l, a, b] = lab(
            table[usize::from(px[0])],
            table[usize::from(px[1])],
            table[usize::from(px[2])],
        );
        sum[0] += l;
        sum[1] += a;
        sum[2] += b;
        let (x, y) = (i % w.max(1), i / w.max(1));
        let r = (y * 3 / h.max(1)).min(2) * 3 + (x * 3 / w.max(1)).min(2);
        regions[r] += l;
        counts[r] += 1;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let bin = (l.clamp(0.0, 99.999) as usize).min(99);
        histogram[bin] += 1.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let n = (w * h).max(1) as f64;
    for (r, c) in regions.iter_mut().zip(counts) {
        #[allow(clippy::cast_precision_loss)]
        let c = c.max(1) as f64;
        *r /= c;
    }
    for v in &mut histogram {
        *v /= n;
    }
    Look {
        lightness: sum[0] / n,
        a: sum[1] / n,
        b: sum[2] / n,
        regions,
        histogram,
    }
}

impl Look {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "lightness": self.lightness,
            "a": self.a,
            "b": self.b,
            "regions": self.regions.as_slice(),
            "histogram": self.histogram.as_slice(),
        })
    }

    fn from_json(v: &serde_json::Value) -> Option<Self> {
        let floats = |key: &str| -> Option<Vec<f64>> {
            v[key]
                .as_array()?
                .iter()
                .map(serde_json::Value::as_f64)
                .collect()
        };
        Some(Self {
            lightness: v["lightness"].as_f64()?,
            a: v["a"].as_f64()?,
            b: v["b"].as_f64()?,
            regions: floats("regions")?.try_into().ok()?,
            histogram: floats("histogram")?.try_into().ok()?,
        })
    }
}

/// How one look stands against another: Pyreal's against retail's, or against the one accepted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drift {
    /// The mean lightness less the other's.
    pub frame_lightness: f64,
    /// The ninth whose lightness moved furthest, signed.
    pub region_lightness: f64,
    /// The mean a\* and b\* less the other's.
    pub a: f64,
    pub b: f64,
    /// The lightness histograms' earth mover's distance, in L\*.
    pub histogram: f64,
}

impl Drift {
    pub fn cast(self) -> f64 {
        (self.a * self.a + self.b * self.b).sqrt()
    }

    /// The measures over `limits`, named.
    pub fn over(self, limits: DriftLimits) -> Vec<String> {
        let mut v = Vec::new();
        if self.frame_lightness.abs() > limits.frame_lightness {
            v.push(format!("lightness {:+.1}", self.frame_lightness));
        }
        if self.region_lightness.abs() > limits.region_lightness {
            v.push(format!("a ninth's lightness {:+.1}", self.region_lightness));
        }
        if self.cast() > limits.cast {
            v.push(format!("colour cast a {:+.1} b {:+.1}", self.a, self.b));
        }
        if self.histogram > limits.histogram {
            v.push(format!("lightness histogram {:.1}", self.histogram));
        }
        v
    }
}

pub fn drift(look: &Look, against: &Look) -> Drift {
    let (pyreal, retail) = (look, against);
    let region_lightness = pyreal
        .regions
        .iter()
        .zip(&retail.regions)
        .map(|(p, r)| p - r)
        .fold(0.0f64, |m, d| if d.abs() > m.abs() { d } else { m });
    let mut cdf = 0.0;
    let mut histogram = 0.0;
    for (p, r) in pyreal.histogram.iter().zip(&retail.histogram) {
        cdf += p - r;
        histogram += f64::abs(cdf);
    }
    Drift {
        frame_lightness: pyreal.lightness - retail.lightness,
        region_lightness,
        a: pyreal.a - retail.a,
        b: pyreal.b - retail.b,
        histogram,
    }
}

/// What `cargo xtask golden` was asked.
struct Request {
    update: bool,
    only: Option<Vec<String>>,
}

fn request(args: &[String]) -> Result<Request, String> {
    let mut r = Request {
        update: false,
        only: None,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--update" => r.update = true,
            "--only" => {
                let list = it.next().ok_or("--only names scenes")?;
                let names: Vec<String> = list.split(',').map(str::to_owned).collect();
                if let Some(bad) = names.iter().find(|n| !SCENES.iter().any(|s| s.slug == *n)) {
                    return Err(format!("no scene {bad}; scenes: {}", slugs()));
                }
                r.only = Some(names);
            }
            "--debug" | "--on-battery" => {}
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(r)
}

fn slugs() -> String {
    SCENES.iter().map(|s| s.slug).collect::<Vec<_>>().join(", ")
}

pub const USAGE: &str = "usage: cargo xtask golden [--update] [--only SLUG,...] [--debug]";

/// `cargo xtask golden`: the comparisons' table, or with `--update` the goldens drawn again first.
pub fn golden(args: &[String], profile: Profile) -> i32 {
    let req = match request(args) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("golden: {e}\n{USAGE}");
            return 2;
        }
    };
    let rows = comparisons(profile, &req);
    crate::util::print_table("Retail comparisons", &rows)
}

/// Tier 2's rows: every scene compared, and the physics traces still to build.
pub fn tier2_rows(profile: Profile) -> Vec<Report> {
    comparisons(
        profile,
        &Request {
            update: false,
            only: None,
        },
    )
}

/// The physics traces' row: named in tier 2 beside the pictures, not built yet.
pub fn physics_traces_row() -> Report {
    Report::new(
        "retail physics traces",
        Outcome::NotRun(NotRun::NotBuilt),
        "long physics traces against retail are not built yet: no retail recordings of \
         pyreal_physics::trace::REQUIRED_SCENARIOS exist (DERETH_TEST_PHYSICS_TRACES)",
    )
}

const GOLDEN_ROW: &str = "retail golden images";
const DRIFT_ROW: &str = "look drift from retail";

fn both(outcome: Outcome, detail: &str) -> Vec<Report> {
    vec![
        Report::new(GOLDEN_ROW, outcome, detail),
        Report::new(DRIFT_ROW, outcome, detail),
    ]
}

/// One scene's results.
struct Measured {
    slug: &'static str,
    golden: Result<GoldenDiff, String>,
    drift: Result<Drifts, String>,
}

/// Pyreal's look against retail's, and against the one accepted when there is one.
#[derive(Debug, Clone, Copy)]
struct Drifts {
    retail: Drift,
    accepted: Option<Drift>,
}

impl Drifts {
    fn over(self) -> Vec<String> {
        let mut v = self.retail.over(CEILING);
        if let Some(a) = self.accepted {
            v.extend(
                a.over(BAND)
                    .into_iter()
                    .map(|m| format!("{m} since accepted")),
            );
        }
        v
    }
}

fn comparisons(profile: Profile, req: &Request) -> Vec<Report> {
    let started = Instant::now();
    let Some(dats) = retail_data_available() else {
        return both(
            Outcome::NotRun(NotRun::OracleAbsent),
            "the retail dats are not found; set PYREAL_TEST_DAT_DIR",
        );
    };
    let base_pack = match crate::base_pack::pinned() {
        Ok(p) => Some(p),
        Err(e) => {
            println!("golden: no base pack, so no drift check: {e}");
            None
        }
    };
    let viewer = match build_viewer(profile) {
        Ok(v) => v,
        Err(e) => return both(Outcome::Fail, &e),
    };
    let out = target_dir().join("golden");
    if let Err(e) = std::fs::create_dir_all(&out) {
        return both(Outcome::Fail, &format!("{}: {e}", out.display()));
    }
    let root = golden_root(&dats);
    let scenes: Vec<&Scene> = SCENES
        .iter()
        .filter(|s| {
            req.only
                .as_ref()
                .is_none_or(|o| o.iter().any(|n| n == s.slug))
        })
        .collect();
    let mut adapter = None;
    let mut manifest = None;
    let mut measured = Vec::new();
    for scene in &scenes {
        println!("--- golden: {}", scene.slug);
        for side in ["diff", "golden"] {
            let _ = std::fs::remove_file(out.join(format!("{}-{side}.png", scene.slug)));
        }
        let drawn = out.join(format!("{}-retail.png", scene.slug));
        let retail = draw(
            &viewer,
            &dats,
            &side_flags(scene, true, Path::new("")),
            &drawn,
        );
        let retail = match retail {
            Ok(name) => {
                adapter.get_or_insert(name);
                read_png(&drawn)
            }
            Err(e) => Err(e),
        };
        let pyreal = match (&retail, &base_pack) {
            (Err(e), _) => Err(e.clone()),
            (_, None) => Err("no base pack".to_owned()),
            (Ok(_), Some(pack)) => {
                let path = out.join(format!("{}-pyreal.png", scene.slug));
                draw(&viewer, &dats, &side_flags(scene, false, pack), &path)
                    .and_then(|_| read_png(&path))
                    .map(|p| look(&p))
            }
        };
        let folder = root.join(adapter_folder(adapter.as_deref().unwrap_or("unknown")));
        let manifest = manifest.get_or_insert_with(|| Manifest::read(&folder));
        let golden = match &retail {
            Err(e) => Err(e.clone()),
            Ok(_) if req.update => {
                store_golden(&drawn, &folder, scene, manifest).map(|()| GoldenDiff {
                    differing: 0.0,
                    mean: 0.0,
                })
            }
            Ok(p) => compare_golden(p, &folder, scene, manifest, &out),
        };
        if let (true, Ok(l)) = (req.update, &pyreal) {
            manifest.accept(scene, l);
        }
        let drift = match (&retail, &pyreal) {
            (Ok(r), Ok(p)) => Ok(Drifts {
                retail: drift(p, &look(r)),
                accepted: manifest.accepted(scene).map(|a| drift(p, &a)),
            }),
            (Err(e), _) | (_, Err(e)) => Err(e.clone()),
        };
        measured.push(Measured {
            slug: scene.slug,
            golden,
            drift,
        });
    }
    let folder = root.join(adapter_folder(adapter.as_deref().unwrap_or("unknown")));
    if req.update {
        if let Some(m) = &mut manifest {
            m.adapter.clone_from(&adapter);
            if let Err(e) = m.write(&folder) {
                return both(Outcome::Fail, &e);
            }
            println!("goldens written to {}", folder.display());
        }
    }
    print_measures(&measured);
    let secs = started.elapsed().as_secs();
    let took = format!(
        "{} scenes in {}m {:02}s",
        scenes.len(),
        secs / 60,
        secs % 60
    );
    vec![
        golden_row(&measured, &folder, &took),
        drift_row(&measured, base_pack.is_some(), &took),
    ]
}

/// Builds engine-view in `profile` and returns its path.
fn build_viewer(profile: Profile) -> Result<PathBuf, String> {
    let mut argv = vec!["build"];
    argv.extend(profile.cargo_args());
    argv.extend(["-p", "pyreal-engine-view"]);
    if !crate::util::run(&workspace_root(), "cargo", &argv) {
        return Err("engine-view did not build".to_owned());
    }
    let dir = match profile {
        Profile::TestRelease => "test-release",
        Profile::Dev => "debug",
    };
    Ok(target_dir()
        .join(dir)
        .join(format!("engine-view{}", std::env::consts::EXE_SUFFIX)))
}

/// Draws one picture to `out`; the adapter's name it was drawn on.
fn draw(viewer: &Path, dats: &Path, flags: &[String], out: &Path) -> Result<String, String> {
    let _ = std::fs::remove_file(out);
    let output = Command::new(viewer)
        .current_dir(workspace_root())
        .arg("--dat-dir")
        .arg(dats)
        .args(flags)
        .arg("--out")
        .arg(out)
        .output()
        .map_err(|e| format!("{}: {e}", viewer.display()))?;
    let log = String::from_utf8_lossy(&output.stdout).into_owned()
        + &String::from_utf8_lossy(&output.stderr);
    if !output.status.success() || !out.is_file() {
        let tail: Vec<&str> = log.lines().rev().take(6).collect();
        return Err(format!(
            "engine-view failed: {}",
            tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
        ));
    }
    Ok(log
        .lines()
        .find_map(|l| l.strip_prefix("adapter: "))
        .unwrap_or("unknown")
        .trim()
        .to_owned())
}

/// The goldens' record: what drew them and with what flags.
#[derive(Debug, Default)]
struct Manifest {
    adapter: Option<String>,
    scenes: serde_json::Map<String, serde_json::Value>,
}

const MANIFEST: &str = "golden.json";

impl Manifest {
    fn read(folder: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(folder.join(MANIFEST)) else {
            return Self::default();
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            return Self::default();
        };
        Self {
            adapter: v["adapter"].as_str().map(str::to_owned),
            scenes: v["scenes"].as_object().cloned().unwrap_or_default(),
        }
    }

    fn write(&self, folder: &Path) -> Result<(), String> {
        let (_, head) = crate::util::run_captured(&workspace_root(), "git", &["rev-parse", "HEAD"]);
        let v = serde_json::json!({
            "format": 1,
            "adapter": self.adapter,
            "commit": head.trim(),
            "scenes": self.scenes,
        });
        let text = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())? + "\n";
        let path = folder.join(MANIFEST);
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

impl Manifest {
    fn entry(&mut self, scene: &Scene) -> &mut serde_json::Map<String, serde_json::Value> {
        let e = self
            .scenes
            .entry(scene.slug)
            .or_insert_with(|| serde_json::json!({}));
        if !e.is_object() {
            *e = serde_json::json!({});
        }
        e.as_object_mut().expect("made an object above")
    }

    /// Records `look` as the scene's accepted Pyreal look.
    fn accept(&mut self, scene: &Scene, look: &Look) {
        let key = pyreal_key(scene);
        self.entry(scene).insert(
            "pyreal".to_owned(),
            serde_json::json!({ "flags": key, "look": look.to_json() }),
        );
    }

    /// The scene's accepted Pyreal look, when it was drawn with the flags it has now.
    fn accepted(&self, scene: &Scene) -> Option<Look> {
        let p = &self.scenes.get(scene.slug)?["pyreal"];
        (p["flags"].as_str() == Some(pyreal_key(scene).as_str()))
            .then(|| Look::from_json(&p["look"]))
            .flatten()
    }
}

/// The accepted look's record of how it was drawn: Pyreal's side's flags, the pack left out so a
/// new pack is measured against the old one's look.
fn pyreal_key(scene: &Scene) -> String {
    side_flags(scene, false, Path::new("base-pack.dat")).join(" ")
}

fn store_golden(
    drawn: &Path,
    folder: &Path,
    scene: &Scene,
    m: &mut Manifest,
) -> Result<(), String> {
    std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let to = folder.join(format!("{}.png", scene.slug));
    std::fs::copy(drawn, &to).map_err(|e| format!("{}: {e}", to.display()))?;
    m.entry(scene)
        .insert("flags".to_owned(), serde_json::json!(retail_key(scene)));
    Ok(())
}

/// An `Err` starting with [`NO_GOLDEN`] is an absent oracle, not a failure.
const NO_GOLDEN: &str = "no golden";

fn compare_golden(
    drawn: &Picture,
    folder: &Path,
    scene: &Scene,
    m: &Manifest,
    out: &Path,
) -> Result<GoldenDiff, String> {
    let path = folder.join(format!("{}.png", scene.slug));
    if !path.is_file() {
        return Err(format!("{NO_GOLDEN} at {}", path.display()));
    }
    if m.scenes.get(scene.slug).and_then(|s| s["flags"].as_str())
        != Some(retail_key(scene).as_str())
    {
        return Err(format!(
            "{NO_GOLDEN} for this scene's flags: it changed since the golden was drawn"
        ));
    }
    let (diff, picture) = golden_diff(drawn, &read_png(&path)?)?;
    if !diff.passes() {
        let d = out.join(format!("{}-diff.png", scene.slug));
        write_png(&d, &picture)?;
        let g = out.join(format!("{}-golden.png", scene.slug));
        let _ = std::fs::copy(&path, g);
    }
    Ok(diff)
}

fn print_measures(measured: &[Measured]) {
    println!(
        "\n  {:<14} {:>18}   {:>34}   {:>27}",
        "", "golden", "Pyreal against retail", "Pyreal since accepted"
    );
    println!(
        "  {:<14} {:>9} {:>8}   {:>6} {:>6} {:>6} {:>6} {:>6}   {:>6} {:>6} {:>6} {:>6}",
        "scene",
        "differing",
        "mean",
        "L*",
        "ninth",
        "a*",
        "b*",
        "hist",
        "L*",
        "ninth",
        "cast",
        "hist"
    );
    let dash = |n: usize| {
        vec!["-"; n]
            .iter()
            .map(|d| format!("{d:>6}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    for m in measured {
        let g = match &m.golden {
            Ok(d) => format!("{:>8.3}% {:>8.3}", d.differing * 100.0, d.mean),
            Err(_) => format!("{:>9} {:>8}", "-", "-"),
        };
        let (r, a) = match &m.drift {
            Ok(d) => (
                format!(
                    "{:>+6.1} {:>+6.1} {:>+6.1} {:>+6.1} {:>6.1}",
                    d.retail.frame_lightness,
                    d.retail.region_lightness,
                    d.retail.a,
                    d.retail.b,
                    d.retail.histogram
                ),
                d.accepted.map_or_else(
                    || dash(4),
                    |a| {
                        format!(
                            "{:>+6.1} {:>+6.1} {:>6.1} {:>6.1}",
                            a.frame_lightness,
                            a.region_lightness,
                            a.cast(),
                            a.histogram
                        )
                    },
                ),
            ),
            Err(_) => (dash(5), dash(4)),
        };
        println!("  {:<14} {g}   {r}   {a}", m.slug);
    }
}

fn golden_row(measured: &[Measured], folder: &Path, took: &str) -> Report {
    let mut failed = Vec::new();
    let mut absent = Vec::new();
    for m in measured {
        match &m.golden {
            Ok(d) if d.passes() => {}
            Ok(d) => failed.push(format!(
                "{} off its golden ({:.2}% of pixels, mean {:.2})",
                m.slug,
                d.differing * 100.0,
                d.mean
            )),
            Err(e) if e.starts_with(NO_GOLDEN) => absent.push(m.slug),
            Err(e) => failed.push(format!("{}: {e}", m.slug)),
        }
    }
    let worst = measured
        .iter()
        .filter_map(|m| m.golden.as_ref().ok())
        .map(|d| d.differing)
        .fold(0.0f64, f64::max);
    if !failed.is_empty() {
        return Report::new(
            GOLDEN_ROW,
            Outcome::Fail,
            format!(
                "{}; pictures and differences in {}; {took}",
                failed.join(", "),
                target_dir().join("golden").display()
            ),
        );
    }
    if !absent.is_empty() {
        return Report::new(
            GOLDEN_ROW,
            Outcome::NotRun(NotRun::OracleAbsent),
            format!(
                "no golden for {} under {}; cargo xtask golden --update draws them",
                absent.join(", "),
                folder.display()
            ),
        );
    }
    Report::new(
        GOLDEN_ROW,
        Outcome::Pass,
        format!(
            "{} scenes match, at most {:.3}% of pixels off; {took}",
            measured.len(),
            worst * 100.0
        ),
    )
}

fn drift_row(measured: &[Measured], have_pack: bool, took: &str) -> Report {
    if !have_pack {
        return Report::new(
            DRIFT_ROW,
            Outcome::NotRun(NotRun::OracleAbsent),
            "the pinned base pack is not built and cannot be; see `cargo xtask base-pack build`",
        );
    }
    let mut failed = Vec::new();
    let mut unaccepted = Vec::new();
    for m in measured {
        match &m.drift {
            Ok(d) => {
                let over = d.over();
                if !over.is_empty() {
                    failed.push(format!("{} ({})", m.slug, over.join(", ")));
                }
                if d.accepted.is_none() {
                    unaccepted.push(m.slug);
                }
            }
            Err(e) => failed.push(format!("{}: {e}", m.slug)),
        }
    }
    if !failed.is_empty() {
        return Report::new(
            DRIFT_ROW,
            Outcome::Fail,
            format!(
                "{} strayed; pictures in {}; {took}",
                failed.join(", "),
                target_dir().join("golden").display()
            ),
        );
    }
    if !unaccepted.is_empty() {
        return Report::new(
            DRIFT_ROW,
            Outcome::NotRun(NotRun::OracleAbsent),
            format!(
                "within the limits of retail's look, but no accepted look for {}; cargo xtask \
                 golden --update records it",
                unaccepted.join(", ")
            ),
        );
    }
    Report::new(
        DRIFT_ROW,
        Outcome::Pass,
        format!(
            "{} scenes near retail's look and the accepted one; {took}",
            measured.len()
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(rgb: [u8; 3], w: u32, h: u32) -> Picture {
        Picture {
            width: w,
            height: h,
            rgba: (0..w * h)
                .flat_map(|_| [rgb[0], rgb[1], rgb[2], 255])
                .collect(),
        }
    }

    #[test]
    fn a_picture_matches_itself_and_not_a_brighter_one() {
        let a = flat([90, 120, 60], 8, 8);
        let (same, _) = golden_diff(&a, &a).unwrap();
        assert!(same.passes() && same.differing == 0.0 && same.mean == 0.0);
        let (off, _) = golden_diff(&flat([130, 160, 100], 8, 8), &a).unwrap();
        assert!(!off.passes(), "{off:?}");
        assert!(golden_diff(&a, &flat([90, 120, 60], 4, 4)).is_err());
    }

    #[test]
    fn white_and_grey_have_their_lightness() {
        let white = look(&flat([255, 255, 255], 3, 3));
        assert!((white.lightness - 100.0).abs() < 0.05, "{white:?}");
        assert!(white.a.abs() < 0.05 && white.b.abs() < 0.05);
        let grey = look(&flat([119, 119, 119], 3, 3));
        assert!((grey.lightness - 50.0).abs() < 0.5, "{}", grey.lightness);
    }

    #[test]
    fn a_darker_or_bluer_world_drifts_and_the_same_one_does_not() {
        let retail = look(&flat([110, 120, 90], 9, 9));
        let same = drift(&retail, &retail);
        assert!(same.over(BAND).is_empty(), "{same:?}");
        let dark = drift(&look(&flat([40, 44, 32], 9, 9)), &retail);
        assert!(dark.frame_lightness < -BAND.frame_lightness, "{dark:?}");
        assert!(!dark.over(BAND).is_empty());
        let blue = drift(&look(&flat([80, 110, 160], 9, 9)), &retail);
        assert!(blue.b < -BAND.cast, "{blue:?}");
        assert!(!blue.over(BAND).is_empty());
    }

    #[test]
    fn the_histogram_distance_is_the_lightness_moved() {
        let a = look(&flat([119, 119, 119], 3, 3));
        let b = look(&flat([255, 255, 255], 3, 3));
        let d = drift(&b, &a);
        assert!(
            (d.histogram - (b.lightness - a.lightness)).abs() < 1.5,
            "{d:?}"
        );
    }

    #[test]
    fn every_scene_has_its_own_slug_and_a_camera() {
        let library = std::fs::read_to_string(workspace_root().join(LIBRARY)).unwrap();
        for (i, s) in SCENES.iter().enumerate() {
            assert!(
                SCENES[..i].iter().all(|o| o.slug != s.slug),
                "{} twice",
                s.slug
            );
            match s.place {
                Place::Library(name) => assert!(
                    library.lines().any(|l| l.starts_with(&format!("{name} |"))),
                    "{}: no place {name} in the library",
                    s.slug
                ),
                Place::Report(line) => assert!(line.contains("; camera "), "{}", s.slug),
            }
        }
    }

    #[test]
    fn an_accepted_look_reads_back_and_a_broken_one_does_not() {
        let l = look(&flat([110, 120, 90], 9, 9));
        assert_eq!(Look::from_json(&l.to_json()), Some(l.clone()));
        let mut v = l.to_json();
        v["regions"] = serde_json::json!([1.0, 2.0]);
        assert_eq!(Look::from_json(&v), None);
    }

    #[test]
    fn a_black_frame_is_over_the_ceiling() {
        let retail = look(&flat([110, 120, 90], 9, 9));
        let black = drift(&look(&flat([0, 0, 0], 9, 9)), &retail);
        assert!(!black.over(CEILING).is_empty(), "{black:?}");
        let near = drift(&look(&flat([120, 130, 98], 9, 9)), &retail);
        assert!(near.over(CEILING).is_empty(), "{near:?}");
    }

    #[test]
    fn an_adapter_name_is_a_folder_name() {
        assert_eq!(adapter_folder("Apple M4 Max"), "apple-m4-max");
        assert_eq!(
            adapter_folder("NVIDIA GeForce RTX 4090 (D3D12)"),
            "nvidia-geforce-rtx-4090-d3d12"
        );
    }
}
