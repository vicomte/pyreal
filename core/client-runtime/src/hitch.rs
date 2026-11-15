//! Hitch reports: the last [`KEPT_SECONDS`](crate::hitch::KEPT_SECONDS) of frames kept always, and
//! written to a small text file when a frame takes longer than the threshold, or when the player
//! asks.
//!
//! Each frame keeps its time, the frame loop's steps, what the presentation did besides drawing
//! ([`PresentWork`](crate::report::PresentWork): the land's streaming and its parts, uploads, ray
//! tracing, pipelines, waiting on the device, presenting) and where the camera was and how fast it
//! went. A frame costs one small record and no allocation beyond the ring's.
//!
//! A slow frame's report is written [`AFTER_SECONDS`](crate::hitch::AFTER_SECONDS) later, so it
//! holds the frames after it as well as the [`BEFORE_SECONDS`](crate::hitch::BEFORE_SECONDS)
//! before. Automatic reports are at least [`QUIET_SECONDS`](crate::hitch::QUIET_SECONDS) apart (the
//! hitches between are counted in the next), and none is written in the
//! [`SETTLE_SECONDS`](crate::hitch::SETTLE_SECONDS) after a world is made, whose first frames are
//! its loading (a slow frame among them puts the end off until that long after it). A report asked for holds every frame kept. Reports go to `logs/hitches` in the
//! settings folder, named for the local time.

use std::collections::VecDeque;
use std::fmt::Write as _;

use crate::frame::{FrameStep, STEPS};
use crate::perf_capture::STEP_COLUMNS;
use crate::report::PresentWork;

/// Seconds of frames kept.
pub const KEPT_SECONDS: f64 = 30.0;
/// The most frames kept, whatever the frame rate.
const KEPT_FRAMES: usize = 8000;
/// A frame longer than this, milliseconds, is a hitch unless the threshold is set otherwise.
pub const DEFAULT_THRESHOLD_MS: f32 = 100.0;
/// Seconds of frames before a hitch, and after it, in its report.
pub const BEFORE_SECONDS: f64 = 5.0;
pub const AFTER_SECONDS: f64 = 1.0;
/// The least time between automatic reports, seconds.
pub const QUIET_SECONDS: f64 = 10.0;
/// Seconds after a world is made before a slow frame is reported.
pub const SETTLE_SECONDS: f64 = 3.0;
/// The most reports kept in the folder; the oldest go first.
pub const REPORTS_KEPT: usize = 50;
/// The folder under the settings folder the reports go to.
pub const FOLDER: [&str; 2] = ["logs", "hitches"];

/// Where the camera was in a frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct View {
    /// The landblock it was over, and where in it, metres.
    pub block: (i32, i32),
    pub at: [f32; 3],
    /// How fast it went over the ground since the frame before, and how fast it rose (falling
    /// below nought), metres a second: explore keeps the camera over the ground, so it climbs
    /// hills on top of its speed.
    pub speed: f32,
    pub climb: f32,
    /// Explore mode's speed asked for, metres a second.
    pub flight: Option<f32>,
}

impl View {
    /// Where it was in metres from the world's south-west corner.
    #[must_use]
    pub fn global(&self) -> [f32; 3] {
        #[allow(clippy::cast_precision_loss)]
        let (x, y) = (self.block.0 as f32 * 192.0, self.block.1 as f32 * 192.0);
        [x + self.at[0], y + self.at[1], self.at[2]]
    }
}

/// One frame kept.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    /// The frame loop's clock when the frame began, seconds.
    pub at: f64,
    /// From its start to the next frame's, milliseconds.
    pub ms: f32,
    /// The frame loop's steps, milliseconds, in [`FrameStep::ORDER`].
    pub steps: [f32; STEPS],
    pub work: Option<PresentWork>,
    pub view: Option<View>,
}

/// Why a report is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A frame went over the threshold.
    Hitch,
    /// The player asked.
    Asked,
}

/// A report due to be written now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Due {
    pub kind: Kind,
    /// The slow frame's start, for a hitch.
    pub at: f64,
    /// Hitches since the last report that none was written for.
    pub skipped: u32,
}

/// The frames kept and the reports waiting. See the module header.
#[derive(Debug, Clone)]
pub struct HitchLog {
    frames: VecDeque<Frame>,
    /// A frame over this many milliseconds is a hitch; nought writes no report by itself.
    pub threshold_ms: f32,
    waiting: Option<Due>,
    last_written: Option<f64>,
    skipped: u32,
    quiet_until: f64,
}

impl HitchLog {
    #[must_use]
    pub fn new(threshold_ms: f32) -> Self {
        Self {
            frames: VecDeque::new(),
            threshold_ms,
            waiting: None,
            last_written: None,
            skipped: 0,
            quiet_until: SETTLE_SECONDS,
        }
    }

    /// The frames kept, oldest first.
    pub fn frames(&self) -> impl DoubleEndedIterator<Item = &Frame> {
        self.frames.iter()
    }

    /// The last frame kept.
    #[must_use]
    pub fn last(&self) -> Option<&Frame> {
        self.frames.back()
    }

    /// A world was made at `now`: its loading is not a hitch.
    pub fn settle(&mut self, now: f64) {
        self.quiet_until = now + SETTLE_SECONDS;
    }

    /// The player asked for a report: the next [`Self::record`] hands it over.
    pub fn ask(&mut self, now: f64) {
        self.waiting = Some(Due {
            kind: Kind::Asked,
            at: now,
            skipped: 0,
        });
    }

    /// Keep `frame`, and say whether a report is due now.
    pub fn record(&mut self, frame: Frame) -> Option<Due> {
        let now = frame.at;
        self.frames.push_back(frame);
        while self.frames.len() > KEPT_FRAMES
            || self
                .frames
                .front()
                .is_some_and(|f| now - f.at > KEPT_SECONDS)
        {
            self.frames.pop_front();
        }
        let slow = self.threshold_ms > 0.0 && frame.ms > self.threshold_ms;
        // A slow frame while a world settles (its loading) settles it again from its end.
        if slow && now < self.quiet_until {
            let end = now + f64::from(frame.ms) / 1000.0;
            self.quiet_until = self.quiet_until.max(end + SETTLE_SECONDS);
        }
        if slow && now >= self.quiet_until {
            let quiet = self.last_written.is_some_and(|t| now - t < QUIET_SECONDS);
            if self.waiting.is_some() {
                // In the waiting report's frames already.
            } else if quiet {
                self.skipped += 1;
            } else {
                self.waiting = Some(Due {
                    kind: Kind::Hitch,
                    at: now,
                    skipped: 0,
                });
            }
        }
        let due = self.waiting?;
        let ready = match due.kind {
            Kind::Asked => true,
            Kind::Hitch => now - due.at >= AFTER_SECONDS,
        };
        if !ready {
            return None;
        }
        self.waiting = None;
        if due.kind == Kind::Hitch {
            self.last_written = Some(now);
        }
        let skipped = std::mem::take(&mut self.skipped);
        Some(Due { skipped, ..due })
    }

    /// The frames a report holds: a hitch's from [`BEFORE_SECONDS`] before it, every one kept
    /// for one asked for.
    fn span(&self, due: &Due) -> Vec<&Frame> {
        match due.kind {
            Kind::Asked => self.frames.iter().collect(),
            Kind::Hitch => self
                .frames
                .iter()
                .filter(|f| {
                    f.at >= due.at - BEFORE_SECONDS && f.at <= due.at + AFTER_SECONDS + 1e-6
                })
                .collect(),
        }
    }

    /// The report's text: what it is, `context` (`name: value` lines), a summary, the slowest
    /// frames taken apart, and a table of every frame it holds.
    #[must_use]
    pub fn report(&self, due: &Due, context: &[(&str, String)]) -> String {
        let frames = self.span(due);
        let mut out = String::from("# Pyreal hitch report\n");
        let why = match due.kind {
            Kind::Hitch => format!("a frame took longer than {:.0} ms", self.threshold_ms),
            Kind::Asked => "saved on request".to_owned(),
        };
        let _ = writeln!(out, "# why: {why}");
        for (name, value) in context {
            let _ = writeln!(out, "# {name}: {}", value.replace(['\r', '\n'], " "));
        }
        if due.skipped > 0 {
            let _ = writeln!(
                out,
                "# hitches since the last report with none of their own: {}",
                due.skipped
            );
        }
        let mut ms: Vec<f32> = frames.iter().map(|f| f.ms).collect();
        ms.sort_by(f32::total_cmp);
        let pick = |p: f32| {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss
            )]
            let i = ((ms.len().saturating_sub(1)) as f32 * p).round() as usize;
            ms.get(i).copied().unwrap_or(0.0)
        };
        let seconds = match (frames.first(), frames.last()) {
            (Some(a), Some(b)) => b.at - a.at,
            _ => 0.0,
        };
        let over = |t: f32| ms.iter().filter(|m| **m > t).count();
        let _ = writeln!(
            out,
            "# frames: {} over {seconds:.1} s; median {:.1} ms, 95% under {:.1} ms, worst {:.1} ms; \
             over 33 ms {}, over 100 ms {}",
            frames.len(),
            pick(0.5),
            pick(0.95),
            ms.last().copied().unwrap_or(0.0),
            over(33.3),
            over(100.0),
        );
        // The slowest frames, taken apart, the trigger first.
        let trigger = frames
            .iter()
            .position(|f| due.kind == Kind::Hitch && (f.at - due.at).abs() < 1e-9);
        let mut slow: Vec<usize> = (0..frames.len()).collect();
        slow.sort_by(|a, b| frames[*b].ms.total_cmp(&frames[*a].ms));
        slow.truncate(5);
        if let Some(t) = trigger {
            slow.retain(|i| *i != t);
            slow.insert(0, t);
        }
        out.push_str("\n## Slowest frames\n");
        for i in slow {
            let f = frames[i];
            let prev = i.checked_sub(1).map(|p| frames[p]);
            out.push('\n');
            out.push_str(&describe(f, prev, frames.first().map_or(f.at, |a| a.at)));
        }
        out.push_str("\n## Every frame\n\n");
        out.push_str(&table_header());
        out.push('\n');
        let start = frames.first().map_or(0.0, |f| f.at);
        for (i, f) in frames.iter().enumerate() {
            let prev = i.checked_sub(1).map(|p| frames[p]);
            out.push_str(&row(f, prev, start));
            out.push('\n');
        }
        out
    }
}

/// The frame loop's steps of `f` worth naming, as `name ms`, longest first.
fn steps_text(f: &Frame) -> String {
    let mut steps: Vec<(&str, f32)> = FrameStep::ORDER
        .iter()
        .map(|s| (STEP_COLUMNS[s.index()], f.steps[s.index()]))
        .filter(|(_, ms)| *ms >= 0.5)
        .collect();
    steps.sort_by(|a, b| b.1.total_cmp(&a.1));
    let parts: Vec<String> = steps.iter().map(|(n, ms)| format!("{n} {ms:.1}")).collect();
    if parts.is_empty() {
        "none over 0.5 ms".to_owned()
    } else {
        parts.join(", ")
    }
}

/// One slow frame taken apart, as a few lines; `start` is the report's first frame's start.
fn describe(f: &Frame, prev: Option<&Frame>, start: f64) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Frame at {:+.3} s: {:.1} ms", f.at - start, f.ms);
    let _ = writeln!(out, "  frame loop steps, ms: {}", steps_text(f));
    if let Some(w) = &f.work {
        let _ = writeln!(
            out,
            "  presentation, ms: streaming {:.1} (choosing {:.1}, loading at once {:.1} of which waiting \
             on the planners {:.1}, placing {:.1}, switching {:.1}, freeing {:.1}), objects {:.1}, \
             uploads {:.1}, rays {:.1}, pipelines {:.1}, recording {:.1}, waiting on the device {:.1}, \
             presenting {:.1}",
            w.stream_ms,
            w.retarget_ms,
            w.load_ms,
            w.load_wait_ms,
            w.place_ms,
            w.switch_ms,
            w.evict_ms,
            w.sync_ms,
            w.upload_ms,
            w.rays_ms,
            w.pipeline_ms,
            w.render_ms,
            w.gpu_wait_ms,
            w.present_ms,
        );
        let _ =
            writeln!(
            out,
            "  made: {} pictures ({} KB), {} meshes, {} pipelines or shaders, {} BLAS, {} TLAS, \
             {} bindless groups; {} placing steps (last {}, longest ever {:.1} ms)",
            w.textures,
            w.texture_kb,
            w.meshes,
            w.pipelines,
            w.blas,
            w.tlas,
            w.bind_groups,
            w.steps,
            if w.last_step.is_empty() { "none" } else { w.last_step },
            w.worst_step_ms,
        );
        let d = |now: u64, was: Option<u64>| now.saturating_sub(was.unwrap_or(now));
        let p = prev.and_then(|p| p.work);
        let _ = writeln!(
            out,
            "  streaming: square round {} placing {}, far terrain round {} (one being made round {}); {} blocks placed, {} to place, {} asked and \
             not back, {} planned and not placed, {} jobs queued, {} far blocks' trees asked; this \
             frame {} loads at once, {} switches, {} rebases, {} evictions, {} blocks placed, {} \
             removed (totals {} loads, {} switches)",
            block_text(w.centre),
            block_text(w.target),
            block_text(w.far_centre),
            block_text(w.far_asked),
            w.blocks,
            w.pending,
            w.asked,
            w.planned,
            w.queued,
            w.trees_asked,
            d(w.full_loads, p.map(|p| p.full_loads)),
            d(w.switches, p.map(|p| p.switches)),
            d(w.rebases, p.map(|p| p.rebases)),
            d(w.evictions, p.map(|p| p.evictions)),
            d(w.blocks_placed, p.map(|p| p.blocks_placed)),
            d(w.blocks_removed, p.map(|p| p.blocks_removed)),
            w.full_loads,
            w.switches,
        );
    }
    if let Some(v) = &f.view {
        let _ = writeln!(
            out,
            "  camera: block {:#04X}{:02X} at ({:.1}, {:.1}, {:.1}), {:.0} m/s over the ground, \
             climbing {:.0} m/s{}",
            v.block.0,
            v.block.1,
            v.at[0],
            v.at[1],
            v.at[2],
            v.speed,
            v.climb,
            v.flight
                .map_or_else(String::new, |s| format!(", explore speed {s:.0} m/s")),
        );
    }
    out
}

fn block_text(b: Option<(i32, i32)>) -> String {
    b.map_or_else(|| "none".to_owned(), |(x, y)| format!("{x:#04X}{y:02X}"))
}

/// The table's columns.
const COLUMNS: &[&str] = &[
    "t_s",
    "frame_ms",
    "loop_world",
    "loop_draw",
    "loop_present",
    "loop_other",
    "stream",
    "choose",
    "load",
    "load_wait",
    "place",
    "switch",
    "evict",
    "objects",
    "upload",
    "rays",
    "pipelines",
    "record",
    "device_wait",
    "present",
    "pictures",
    "picture_kb",
    "meshes",
    "pipelines_made",
    "blas",
    "tlas",
    "steps",
    "centre",
    "target",
    "blocks",
    "to_place",
    "asked",
    "queued",
    "loads",
    "switches",
    "evictions",
    "block",
    "x",
    "y",
    "z",
    "speed",
    "climb",
];

fn table_header() -> String {
    COLUMNS.join(",")
}

/// One frame as a row of the table.
fn row(f: &Frame, prev: Option<&Frame>, start: f64) -> String {
    let step = |s: FrameStep| f.steps[s.index()];
    let world = step(FrameStep::WorldViewStep);
    let draw = step(FrameStep::DrawWorld);
    let present = step(FrameStep::PresentFrame);
    let other: f32 = f.steps.iter().sum::<f32>() - world - draw - present;
    let mut out = format!(
        "{:.3},{:.1},{world:.1},{draw:.1},{present:.1},{other:.1}",
        f.at - start,
        f.ms
    );
    match &f.work {
        Some(w) => {
            let p = prev.and_then(|p| p.work);
            let d = |now: u64, was: Option<u64>| now.saturating_sub(was.unwrap_or(now));
            let _ = write!(
                out,
                ",{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},\
                 {},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                w.stream_ms,
                w.retarget_ms,
                w.load_ms,
                w.load_wait_ms,
                w.place_ms,
                w.switch_ms,
                w.evict_ms,
                w.sync_ms,
                w.upload_ms,
                w.rays_ms,
                w.pipeline_ms,
                w.render_ms,
                w.gpu_wait_ms,
                w.present_ms,
                w.textures,
                w.texture_kb,
                w.meshes,
                w.pipelines,
                w.blas,
                w.tlas,
                w.steps,
                block_text(w.centre),
                block_text(w.target),
                w.blocks,
                w.pending,
                w.asked,
                w.queued,
                d(w.full_loads, p.map(|p| p.full_loads)),
                d(w.switches, p.map(|p| p.switches)),
                d(w.evictions, p.map(|p| p.evictions)),
            );
        }
        None => out.push_str(&",".repeat(30)),
    }
    match &f.view {
        Some(v) => {
            let _ = write!(
                out,
                ",{},{:.1},{:.1},{:.1},{:.0},{:.0}",
                block_text(Some(v.block)),
                v.at[0],
                v.at[1],
                v.at[2],
                v.speed,
                v.climb
            );
        }
        None => out.push_str(",,,,,,"),
    }
    out
}

/// The report's file name for `kind` saved at `unix_secs` in a zone `utc_offset` seconds east:
/// `hitch-YYYYMMDD-HHMMSS.txt` or `frames-YYYYMMDD-HHMMSS.txt`, local time.
#[must_use]
pub fn file_name(kind: Kind, unix_secs: i64, utc_offset: i32) -> String {
    let stamp = crate::perf_capture::file_name(unix_secs, utc_offset);
    let stamp = stamp
        .trim_start_matches("perf-capture-")
        .trim_end_matches(".csv");
    let what = match kind {
        Kind::Hitch => "hitch",
        Kind::Asked => "frames",
    };
    format!("{what}-{stamp}.txt")
}

/// The reports' folder under the settings folder `settings`.
#[must_use]
pub fn folder(settings: &std::path::Path) -> std::path::PathBuf {
    FOLDER.iter().fold(settings.to_path_buf(), |p, n| p.join(n))
}

/// Writes `text` as `name` in `dir`, a second's suffix added if the name is taken, and lets the
/// oldest reports go past [`REPORTS_KEPT`]. The path written.
///
/// # Errors
/// The folder cannot be made or the file written.
pub fn write(dir: &std::path::Path, name: &str, text: &str) -> std::io::Result<std::path::PathBuf> {
    std::fs::create_dir_all(dir)?;
    let mut path = dir.join(name);
    let mut n = 2;
    while path.exists() {
        let stem = name.trim_end_matches(".txt");
        path = dir.join(format!("{stem}-{n}.txt"));
        n += 1;
    }
    std::fs::write(&path, text)?;
    let mut old: Vec<(std::time::SystemTime, std::path::PathBuf)> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "txt"))
        .filter_map(|p| Some((p.metadata().ok()?.modified().ok()?, p)))
        .collect();
    if old.len() > REPORTS_KEPT {
        old.sort();
        for (_, p) in &old[..old.len() - REPORTS_KEPT] {
            let _ = std::fs::remove_file(p);
        }
    }
    Ok(path)
}

/// Opens `dir` in the system's file browser; whether one was started.
pub fn open_folder(dir: &std::path::Path) -> bool {
    let _ = std::fs::create_dir_all(dir);
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program).arg(dir).spawn().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(at: f64, ms: f32) -> Frame {
        Frame {
            at,
            ms,
            steps: [0.0; STEPS],
            work: Some(PresentWork {
                stream_ms: ms * 0.9,
                load_ms: ms * 0.8,
                full_loads: u64::from(ms > 100.0),
                centre: Some((0xA9, 0xB4)),
                ..PresentWork::default()
            }),
            view: Some(View {
                block: (0xA9, 0xB4),
                at: [96.0, 12.0, 250.0],
                speed: 1000.0,
                climb: 0.0,
                flight: Some(1000.0),
            }),
        }
    }

    /// Feeds frames 1/60 s apart from `from` for `seconds`, the one at `slow_at` taking
    /// `slow_ms`; the reports due on the way.
    fn run(log: &mut HitchLog, from: f64, seconds: f64, slow: &[(f64, f32)]) -> Vec<Due> {
        let mut due = Vec::new();
        let mut t = from;
        while t < from + seconds {
            let ms = slow
                .iter()
                .find(|(at, _)| (t - at).abs() < 0.5 / 60.0)
                .map_or(16.7, |s| s.1);
            if let Some(d) = log.record(frame(t, ms)) {
                due.push(d);
            }
            t += 1.0 / 60.0;
        }
        due
    }

    /// Behaviour: none (Pyreal's own: hitch reports)
    #[test]
    fn a_slow_frame_is_reported_a_second_later_and_the_next_ones_wait_their_turn() {
        let mut log = HitchLog::new(100.0);
        // During the settling a slow frame is the world's loading, and the settling ends that
        // long after it: the frame at 3.5 s is still the loading's.
        assert!(run(&mut log, 0.0, 4.0, &[(2.5, 900.0), (3.5, 200.0)]).is_empty());
        let due = run(&mut log, 7.0, 3.0, &[(8.0, 400.0), (8.5, 300.0)]);
        assert_eq!(due.len(), 1, "{due:?}");
        assert_eq!(due[0].kind, Kind::Hitch);
        assert!((due[0].at - 8.0).abs() < 0.02);
        // Inside the quiet the next hitch is counted, and reported with the next one after it.
        let due = run(&mut log, 10.0, 11.0, &[(12.0, 200.0), (19.5, 200.0)]);
        assert_eq!(due.len(), 1, "{due:?}");
        assert_eq!(due[0].skipped, 1, "the one at 12 s");
        // Nought turns them off; asking still writes one.
        log.threshold_ms = 0.0;
        assert!(run(&mut log, 21.0, 2.0, &[(21.5, 900.0)]).is_empty());
        log.ask(23.0);
        let due = run(&mut log, 23.0, 0.1, &[]);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].kind, Kind::Asked);
    }

    /// Behaviour: none (Pyreal's own: hitch reports)
    #[test]
    fn the_log_keeps_thirty_seconds_and_a_report_takes_the_slow_frame_apart() {
        let mut log = HitchLog::new(100.0);
        log.settle(-10.0);
        let due = run(&mut log, 0.0, 40.0, &[(38.0, 432.0)]);
        let first = log.frames().next().unwrap().at;
        assert!(first >= 40.0 - KEPT_SECONDS - 0.1, "{first}");
        let text = log.report(
            &due[0],
            &[("mode", "explore over the End of Retail files".into())],
        );
        assert!(text.starts_with("# Pyreal hitch report\n"));
        assert!(text.contains("# why: a frame took longer than 100 ms"));
        assert!(text.contains("# mode: explore over the End of Retail files"));
        let slowest = text.split("## Slowest frames").nth(1).unwrap();
        assert!(
            slowest
                .trim_start()
                .starts_with("Frame at +5.000 s: 432.0 ms"),
            "{slowest}"
        );
        assert!(slowest.contains("loading at once 345.6"), "{slowest}");
        assert!(slowest.contains("this frame 1 loads at once"), "{slowest}");
        assert!(slowest
            .contains("camera: block 0xA9B4 at (96.0, 12.0, 250.0), 1000 m/s over the ground"));
        let table: Vec<&str> = text
            .split("## Every frame\n\n")
            .nth(1)
            .unwrap()
            .lines()
            .collect();
        assert_eq!(table[0], table_header());
        // Five seconds before and a second after, at sixty a second.
        assert!((355..=365).contains(&(table.len() - 1)), "{}", table.len());
        for r in &table[1..] {
            assert_eq!(r.split(',').count(), COLUMNS.len(), "{r}");
        }
    }

    #[test]
    fn reports_are_named_for_the_local_time_and_what_they_are() {
        assert_eq!(
            file_name(Kind::Hitch, 1_792_160_553, 7200),
            "hitch-20261016-162233.txt"
        );
        assert_eq!(
            file_name(Kind::Asked, 1_792_160_553, 7200),
            "frames-20261016-162233.txt"
        );
    }
}
