//! The workspace task runner: the gate tiers, the clippy policy, the seams and the checks clippy
//! cannot express.
//!
//! **Depends on** two of the workspace's crates: `pyreal-dat`, for the tests' lookup of the retail
//! dats (`pyreal_dat::testing`), and `pyreal-machine-lock`, the heavy runs' hold on the machine;
//! otherwise it runs `cargo` and reads the workspace's files.
//! **Used by**
//! nothing: it is the `cargo xtask` binary (the alias is in `.cargo/config.toml`).
//!
//! **Must never** pass a check it did not run: a skipped row is reported as not run, never as a
//! pass, and every checker is calibrated by tests against planted violations.
//!
//! One command per moment:
//!
//! | when | command | needs |
//! |---|---|---|
//! | while working, before each commit | `cargo xtask check` | nothing |
//! | before pushing or opening a PR | `cargo xtask ci prepush` | nothing: what CI leaves to the Mac |
//! | everything that needs no data | `cargo xtask ci tier0` | nothing: CI runs it in parts |
//! | when a test might lean on data you have | `cargo xtask ci no-data` | nothing: the tests as public CI runs them |
//! | with the retail data | `cargo xtask ci tier1` | the dats and `world.pack` |
//! | on hardware | `cargo xtask ci tier2` | a graphics device and the dats |
//! | after a graphics change | `cargo xtask golden` | the same, and the goldens beside the dats |
//!
//! The expensive comparisons wait until the cheap ones hold: golden images against a renderer built
//! on unproven float precision produce an unbounded stream of false differences. The single checks
//! are listed, grouped, by `cargo xtask` with no arguments.
//!
//! Every command that builds or runs tests does so in the `test-release` profile: optimised, with
//! debug assertions and overflow checks on (see [`util::Profile`]). `--debug` on any of them
//! selects cargo's dev profile instead, for investigating a failure.

mod base_pack;
mod bundle;
mod changelog;
mod check;
mod comment_lint;
mod crate_docs;
mod cross_clippy;
mod gates;
mod golden;
mod guards;
mod licence;
mod line_endings;
mod lint;
mod no_data;
mod output_hygiene;
mod package;
mod prepush;
mod seams;
mod skip_claims;
mod sweep;
mod tier_targets;
mod util;
#[cfg(test)]
mod workflow_tests;

use util::{
    print_table, retail_data_available, run, workspace_root, NotRun, Outcome, Profile, Report,
};

const USAGE: &str = "\
usage: cargo xtask <task> [--debug]

When to run what:
  check                     while working, before each commit: clippy and the tests of the crates
                            changed since the base branch (their dependents linted too), and the
                            fast lints. Needs nothing. --base <ref> (default: origin/HEAD,
                            origin/main or main), -p <crate> to name the crates
  ci prepush                before pushing or opening a PR: tier0, cheapest first (the rules, the
                            changelog, the docs, clippy, clippy for Windows and Linux, the build,
                            the tests). CI runs the tests and clippy on Linux only, so this is
                            where the Mac's run. Runs on battery too. --list: what it would run
  ci tier0                  everything that needs no data: the build, every no-data test, clippy,
                            every rule below and the docs. --only build,test,clippy,docs,rules:
                            those parts alone, in that order (CI runs test and clippy on Linux,
                            clippy on Windows, and the rules and docs once). --list: what it would run
  ci smoke                  the tests CI runs on Windows: the crates with code for each system that
                            build without the graphics stack. --list: what it would run
  install-hook [--force]    a git pre-push hook that runs `ci prepush`, skipped when that already
                            passed on the commit pushed (`git push --no-verify` skips it)
  ci no-data                the workspace's tests as public CI runs them, on any machine: no data
                            (every variable naming it emptied), an empty per-user cache and
                            --no-fail-fast, then a list of everything that failed
  ci tier1                  with the retail data: every crate's data tiers. Needs the dats
                            (PYREAL_TEST_DAT_DIR). --serial: the unsharded client dat tier
  ci tier2                  on hardware: the gpu tier and the retail comparisons. Needs a graphics
                            device and the dats. The client's gpu binary runs sharded
                            (PYREAL_TEST_SHARDS=N); --serial: one process
  golden [--update] [--only SLUG,...]
                            tier2's retail comparisons alone: each scene drawn with retail's look
                            against its golden picture, and Pyreal's look measured against
                            retail's. The goldens live outside the repository (PYREAL_GOLDEN_DIR,
                            else Golden beside the dats' folder); --update draws them again
                            tier0, tier1 and tier2 refuse to run on battery unless --on-battery
                            (or PYREAL_ON_BATTERY=1); tier1, tier2, the gates and golden take
                            a heavy hold on the machine lock
  lock [[--heavy|--quiet] -- <command>]
                            who holds the machine lock, or <command> run holding it. A heavy
                            hold (the tiers, build.py, renders; the default) never waits; --quiet,
                            for timing and profiling, waits until the heavy work before it has
                            finished, then has the machine alone (PYREAL_MACHINE_LOCK=off skips)

Rules (each a row of `check` or `ci tier0`):
  fmt-check                 cargo fmt --all --check
  lint                      the bans clippy cannot express (`as i32`, unannotated HashMap) and the seams
  seams                     the seams alone: contract, ui-logic, client-runtime, client crates
  crate-docs                every crate's //! header and its Cargo description
  comment-lint              comments and messages describe behaviour: no work-item labels,
                            private-document pointers or study-method words
  tier-targets              every dat, corpus, gpu or live test target declares required features
  doc                       a check, not a docs build: rustdoc links and doc comments stay correct
                            (every rustdoc warning an error except links to private items). It writes
                            no pages; plain `cargo doc` does
  licence [ROOT]            every crate is MIT, and no ported (copyleft) source is in the tree
  output-hygiene [ROOT]     no test or tool writes into a tracked path or outside the workspace
  line-endings --tree [ROOT] | --normalise FILE... | FILE...
                            every tracked file has the ending .gitattributes gives it (ROOT: any
                            git checkout, default the workspace)
  skip-claims --tree [ROOT] no test skips when the retail dats are absent, and no comment claims
                            one does
  changelog                 a branch that changes what players get (since its merge base with
                            main) updates CHANGELOG.md's Unreleased section, or says it has nothing
                            for players: a `Changelog: none` commit trailer or PYREAL_CHANGELOG=none
  cross-clippy              clippy -D warnings for Windows and Linux from this machine, their C
                            stood in (needs `rustup target add` for each)

Tests:
  build [ARGS]              cargo build of the workspace (ARGS go to cargo: `cargo xtask build
                            --release`)
  test | clippy             cargo test / clippy -D warnings over the whole workspace (clippy with
                            `retail-dats` on, so the data and device tests are linted too)
  gate <name>               one area's whole suite, data and device tiers included (--serial);
                            names: {GATES}
  gates                     every area's gate, one table
  sweep [SUITE...] [--shard-binary PKG:TIER] [--serial] [--shards N] ...
                            run test targets one by one, or one tier binary sharded across
                            processes, with a verdict that cannot read as silence (--help)

Packaging:
  package pyreal [--base-pack <file>] [--target <triple>]... [--out <dir>] [--allow-dirty]
                 | --gather <dir>
                            Pyreal's release files: builds the client and the plugin tool for this
                            host, with the base art pack when --base-pack names it
                            (Windows zip, macOS Pyreal.app, Linux tarball),
                            checked, scanned and, on macOS, signed with APPLE_SIGNING_IDENTITY
  base-pack build           the base art pack base-pack.toml pins, packed from the pack
                            repository's source at its commit into <target>/base-pack
  base-pack update [<rev>]  pack the pack repository at <rev> (its HEAD by default) and pin it
  bundle-macos              the client as a macOS `Pyreal.app` (--dat-dir <dir> links the retail
                            dats in; --base-pack <file> copies the base pack in; --debug bundles the
                            dev profile)
";

/// The usage text, with the gate names wrapped under their column.
fn usage() -> String {
    let indent = " ".repeat(28);
    let mut lines = vec![String::from("names:")];
    for name in gates::names().split(' ') {
        let last = lines.last_mut().expect("one line at least");
        if last.len() + 1 + name.len() > 72 {
            lines.push(name.to_owned());
        } else {
            last.push(' ');
            last.push_str(name);
        }
    }
    USAGE.replace("names: {GATES}", &lines.join(&format!("\n{indent}")))
}

fn main() {
    if let Some(rustdoc) = std::env::var_os(RUSTDOC_STAND_IN) {
        std::process::exit(rustdoc_stand_in(&rustdoc));
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let profile = Profile::from_args(&args);
    let code = match args.first().map(String::as_str) {
        Some("check") => check::check(&args[1..]),
        Some("build") => {
            let mut argv = vec!["build", "--workspace"];
            argv.extend(args[1..].iter().map(String::as_str));
            i32::from(!run(&workspace_root(), "cargo", &argv))
        }
        Some("test") => {
            let mut argv = vec!["test", "--workspace"];
            argv.extend_from_slice(profile.cargo_args());
            i32::from(!run(&workspace_root(), "cargo", &argv))
        }
        Some("clippy") => i32::from(!run(&workspace_root(), "cargo", CLIPPY_ARGS)),
        Some("lint") => lint::lint(),
        Some("seams") => seams::seams(),
        Some("crate-docs") => crate_docs::crate_docs(),
        Some("comment-lint") => comment_lint::comment_lint(),
        Some("tier-targets") => tier_targets::tier_targets(),
        Some("fmt-check") => i32::from(!run(&workspace_root(), "cargo", FMT_CHECK)),
        Some("doc") => i32::from(!doc_gate(&workspace_root(), DOC_ARGS)),
        Some("licence") => licence::licence(&args[1..]),
        Some("output-hygiene") => output_hygiene::output_hygiene(&args[1..]),
        Some("line-endings") => line_endings::line_endings(&args[1..]),
        Some("skip-claims") => skip_claims::skip_claims(&args[1..]),
        Some("sweep") => sweep::sweep(&args[1..]),
        Some("gate") => match gate_name(&args) {
            Some(g) => with_machine_lock(&format!("cargo xtask gate {g}"), || {
                gates::gate(g, threads(&args), profile)
            }),
            None => {
                eprintln!(
                    "gate: name one, e.g. `cargo xtask gate physics`; known: {}",
                    gates::names()
                );
                2
            }
        },
        Some("gates") => with_machine_lock("cargo xtask gates", || all_gates(&args, profile)),
        Some("golden") => match guards::mains_power("cargo xtask golden", &args) {
            Ok(()) => {
                with_machine_lock("cargo xtask golden", || golden::golden(&args[1..], profile))
            }
            Err(e) => {
                eprintln!("golden: {e}");
                1
            }
        },
        Some("changelog") => changelog::changelog(),
        Some("cross-clippy") => cross_clippy::cross_clippy(),
        Some("lock") => guards::lock(&args[1..]),
        Some("bundle-macos") => bundle::bundle_macos(&args[1..]),
        Some("package") => package::package(&args[1..]),
        Some("base-pack") => base_pack::base_pack(&args[1..]),
        Some("ci") => ci(&args[1..]),
        Some("install-hook") => prepush::install_hook(&args[1..]),
        None | Some("help" | "--help" | "-h") => {
            print!("{}", usage());
            0
        }
        Some(other) => {
            eprint!("unknown task {other:?}\n\n{}", usage());
            2
        }
    };
    std::process::exit(code);
}

/// `--serial` on any command that runs a gate.
///
/// The default is the sharded form: `pyreal-client`'s `dat` and `gpu` binaries run as N single-threaded
/// processes over disjoint module slices rather than as one. `--serial` is the unsharded form, the
/// one a sharded run must be equivalent to -- so it stays reachable as the reference.
fn threads(args: &[String]) -> gates::Threads {
    if args.iter().any(|a| a == "--serial") {
        gates::Threads::Serial
    } else {
        gates::Threads::Sharded
    }
}

/// The gate `cargo xtask gate <name>` names, skipping any flag.
///
/// A free function because the alternative -- `args[1]` -- made `cargo xtask gate --serial client`
/// look up a gate called `--serial` and print "unknown gate", which is a flag silently eating
/// the argument after it. The two orders have to mean the same thing.
fn gate_name(args: &[String]) -> Option<&String> {
    args[1..].iter().find(|a| !a.starts_with("--"))
}

/// The rules that take seconds and read the whole repository: `check` and `ci tier0` both run
/// them, as one row each.
fn cheap_lints() -> Vec<Report> {
    let ws = workspace_root();
    vec![
        step("cargo fmt --all --check", run(&ws, "cargo", FMT_CHECK)),
        step("xtask lint (bans and seams)", lint::lint() == 0),
        step("xtask crate-docs", crate_docs::crate_docs() == 0),
        step("xtask comment-lint", comment_lint::comment_lint() == 0),
        step("xtask tier-targets", tier_targets::tier_targets() == 0),
    ]
}

/// One row of tier 0 that runs cargo.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Tier0Step {
    /// `cargo <argv>`, its row named by the label.
    Cargo(String, Vec<&'static str>),
    /// Clippy, naming the crates responsible when it fails: the row's label and `cargo <argv>`.
    Clippy(String, Vec<&'static str>),
    /// The documentation check: `cargo <argv>` with [`DOC_FLAGS`], its row named by the label.
    Doc(String, Vec<&'static str>),
}

impl Tier0Step {
    /// The row's label and `cargo <argv>`.
    fn label_and_argv(&self) -> (&str, &[&'static str]) {
        match self {
            Tier0Step::Cargo(label, argv)
            | Tier0Step::Clippy(label, argv)
            | Tier0Step::Doc(label, argv) => (label, argv),
        }
    }

    /// The part of tier 0 this row belongs to.
    fn part(&self) -> Tier0Part {
        match self {
            Tier0Step::Cargo(_, argv) if argv.first() == Some(&"build") => Tier0Part::Build,
            Tier0Step::Cargo(..) => Tier0Part::Test,
            Tier0Step::Clippy(..) => Tier0Part::Clippy,
            Tier0Step::Doc(..) => Tier0Part::Docs,
        }
    }
}

/// A part of tier 0, for `--only`: CI runs the tests and clippy on Linux, clippy on Windows, and the
/// rules and the docs, which read the same source everywhere, once. The build and the Mac's run are
/// `ci prepush`'s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tier0Part {
    Build,
    Test,
    Clippy,
    Docs,
    Rules,
}

impl Tier0Part {
    const ALL: [Tier0Part; 5] = [
        Tier0Part::Build,
        Tier0Part::Test,
        Tier0Part::Clippy,
        Tier0Part::Docs,
        Tier0Part::Rules,
    ];

    fn parse(name: &str) -> Result<Self, String> {
        match name {
            "build" => Ok(Tier0Part::Build),
            "test" => Ok(Tier0Part::Test),
            "clippy" => Ok(Tier0Part::Clippy),
            "docs" => Ok(Tier0Part::Docs),
            "rules" => Ok(Tier0Part::Rules),
            other => Err(format!(
                "--only takes build, test, clippy, docs and rules, not {other:?}"
            )),
        }
    }

    /// The parts `--only a,b` names; every part when it is not given.
    fn from_args(args: &[String]) -> Result<Vec<Self>, String> {
        let mut named = None;
        let mut it = args.iter();
        while let Some(arg) = it.next() {
            if arg == "--only" {
                named = Some(it.next().ok_or("--only names at least one part")?.as_str());
            } else if let Some(value) = arg.strip_prefix("--only=") {
                named = Some(value);
            }
        }
        match named {
            None => Ok(Self::ALL.to_vec()),
            Some(list) => list.split(',').map(Self::parse).collect(),
        }
    }
}

/// Tier 0's cargo rows, in order.
fn tier0_steps(profile: Profile) -> Vec<Tier0Step> {
    let with = |verb: &'static str, extra: &[&'static str]| -> Vec<&'static str> {
        let mut argv = vec![verb, "--workspace"];
        argv.extend_from_slice(extra);
        argv.extend_from_slice(profile.cargo_args());
        argv
    };
    let mut steps = vec![
        Tier0Step::Cargo("cargo build --workspace".to_owned(), with("build", &[])),
        Tier0Step::Cargo(
            "cargo test --workspace".to_owned(),
            with("test", &["--no-fail-fast"]),
        ),
    ];
    steps.push(Tier0Step::Clippy(
        "cargo clippy --workspace --all-targets --features retail-dats".to_owned(),
        CLIPPY_ARGS.to_vec(),
    ));
    steps.push(Tier0Step::Doc(
        "xtask doc (rustdoc warnings denied)".to_owned(),
        DOC_ARGS.to_vec(),
    ));
    // One workspace doc build merges the features every member asks of the runtime, the test
    // kit's test support among them, which the client is shipped without. So the runtime is
    // also documented on its own, with its own features, as the client has it.
    steps.push(Tier0Step::Doc(
        "xtask doc -p pyreal-client-runtime (the runtime as shipped)".to_owned(),
        RUNTIME_DOC_ARGS.to_vec(),
    ));
    steps
}

/// Tier 0's test rows as commands: the label and `cargo <argv>` of each.
fn tier0_test_commands(profile: Profile) -> Vec<(String, Vec<&'static str>)> {
    tier0_steps(profile)
        .into_iter()
        .filter(|planned| planned.part() == Tier0Part::Test)
        .map(|planned| match planned {
            Tier0Step::Cargo(label, argv)
            | Tier0Step::Clippy(label, argv)
            | Tier0Step::Doc(label, argv) => (label, argv),
        })
        .collect()
}

/// Tier 0 (everything that needs no data, each thing once), or the `parts` of it, in the order
/// given. The build, the tests, clippy and the docs cover the whole workspace in one invocation
/// each. Each cargo row says how long it took: the numbers to read when tier 0 gets slow.
fn tier0_parts(profile: Profile, parts: &[Tier0Part], title: &str) -> i32 {
    print_table(title, &tier0_reports(profile, parts))
}

/// The rows of [`tier0_parts`], run.
fn tier0_reports(profile: Profile, parts: &[Tier0Part]) -> Vec<Report> {
    let ws = workspace_root();
    let mut reports = Vec::new();
    for part in parts {
        if *part == Tier0Part::Rules {
            reports.extend(cheap_lints());
            reports.push(source_rule("licence", || {
                licence::report(&licence::check(&ws))
            }));
            reports.push(source_rule("output-hygiene", || output_hygiene::run(&ws)));
            continue;
        }
        for planned in tier0_steps(profile) {
            if planned.part() != *part {
                continue;
            }
            let started = std::time::Instant::now();
            let mut rows = match planned {
                Tier0Step::Cargo(label, argv) => vec![step(&label, run(&ws, "cargo", &argv))],
                Tier0Step::Clippy(label, argv) => vec![clippy_step(&ws, &label, &argv)],
                Tier0Step::Doc(label, argv) => vec![step(&label, doc_gate(&ws, &argv))],
            };
            say_time_taken(&mut rows, started.elapsed());
            reports.extend(rows);
        }
    }
    reports
}

/// What [`tier0_parts`] would run for `parts`, one line per row, for `--list`.
fn tier0_plan(profile: Profile, parts: &[Tier0Part]) -> Vec<String> {
    let mut lines = Vec::new();
    for part in parts {
        if *part == Tier0Part::Rules {
            lines.push(format!("rules: {}", RULE_NAMES.join(", ")));
            continue;
        }
        for planned in tier0_steps(profile) {
            if planned.part() == *part {
                let (label, argv) = planned.label_and_argv();
                lines.push(format!("{label}: cargo {}", argv.join(" ")));
            }
        }
    }
    lines
}

/// The rules tier 0's `rules` part runs, by the names `cargo xtask <name>` runs each alone.
const RULE_NAMES: &[&str] = &[
    "fmt-check",
    "lint",
    "crate-docs",
    "comment-lint",
    "tier-targets",
    "licence",
    "output-hygiene",
];

/// The crates whose tests CI runs on Windows: those with code of their own for each system that
/// build without the graphics stack. Clippy there checks every crate compiles, and the Windows
/// package's build that what it ships links.
const SMOKE_CRATES: &[&str] = &[
    "pyreal-client-contract",
    "pyreal-client-model",
    "pyreal-client-runtime",
    "pyreal-clipboard",
    "pyreal-console",
    "pyreal-dat",
    "pyreal-input",
    "pyreal-launch",
    "pyreal-primitives",
];

/// `cargo <argv>` of `ci smoke`: the tests of [`SMOKE_CRATES`], run on past a failure.
fn smoke_argv(profile: Profile) -> Vec<&'static str> {
    let mut argv = vec!["test", "--no-fail-fast"];
    for krate in SMOKE_CRATES {
        argv.extend(["-p", krate]);
    }
    argv.extend_from_slice(profile.cargo_args());
    argv
}

/// `ci smoke`: the tests of [`SMOKE_CRATES`], one row.
fn smoke(profile: Profile) -> i32 {
    let started = std::time::Instant::now();
    let argv = smoke_argv(profile);
    let mut rows = vec![step(
        "cargo test (the smoke crates)",
        run(&workspace_root(), "cargo", &argv),
    )];
    say_time_taken(&mut rows, started.elapsed());
    print_table("CI smoke -- the crates with code for each system", &rows)
}

/// Adds how long a tier 0 row took to its first report's detail.
fn say_time_taken(rows: &mut [Report], taken: std::time::Duration) {
    let Some(row) = rows.first_mut() else {
        return;
    };
    let secs = taken.as_secs();
    let taken = format!("took {}m {:02}s", secs / 60, secs % 60);
    row.detail = if row.detail.is_empty() {
        taken
    } else {
        format!("{}, {taken}", row.detail)
    };
}

/// Tier 1: what is found, then every area's data tier (the gates without their `gpu` binaries). Data that is not found reports its rows NO-ORACLE and the run exits
/// non-zero.
fn tier1(args: &[String], profile: Profile) -> i32 {
    let dats = retail_data_available();
    match &dats {
        Some(dir) => println!("retail dats found at {}", dir.display()),
        None => {
            // Name the gates whose harnesses read them: derived from the catalogue, so the
            // message cannot go stale when it changes.
            let owed: Vec<&str> = gates::GATES
                .iter()
                .filter(|g| g.needs_retail())
                .map(|g| g.name)
                .collect();
            println!(
                "retail dats NOT found; the dat tiers will fail and {} gate harness(es) report \
                 NO-ORACLE ({}), so this run exits non-zero. Set PYREAL_TEST_DAT_DIR to the \
                 directory holding them.",
                owed.len(),
                owed.join(", "),
            );
        }
    }
    let reports = gates::gates(threads(args), profile, gates::GpuTier::Exclude);
    print_table("CI tier 1 -- the retail data", &reports)
}

/// Tier 2: the `gpu` binaries, then the comparisons against retail.
fn tier2(args: &[String], profile: Profile) -> i32 {
    let dats = retail_data_available();
    let mut reports: Vec<Report> = gates::gpu_invocations(profile, threads(args))
        .into_iter()
        .map(|(krate, call)| {
            let label = format!("gpu tier: {krate}");
            if dats.is_none() {
                return Report::new(
                    label,
                    Outcome::NotRun(NotRun::OracleAbsent),
                    "the retail dats are not found; set PYREAL_TEST_DAT_DIR",
                );
            }
            step(&label, call.run())
        })
        .collect();
    reports.extend(golden::tier2_rows(profile));
    reports.push(golden::physics_traces_row());
    print_table("CI tier 2 -- hardware and retail comparisons", &reports)
}

/// `cargo xtask gates`: every area's gate, one table.
fn all_gates(args: &[String], profile: Profile) -> i32 {
    let reports = gates::gates(threads(args), profile, gates::GpuTier::Include);
    print_table("Gates -- every area's whole suite", &reports)
}

/// A CI tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CiTier {
    Zero,
    PrePush,
    Smoke,
    NoData,
    One,
    Two,
}

/// The usage line of `cargo xtask ci`.
const CI_USAGE: &str = "usage: cargo xtask ci prepush|tier0|smoke|no-data|tier1|tier2 \
     [--only part,...] [--list] [--hook] [--serial] [--debug] [--on-battery]";

/// The tier `cargo xtask ci <args>` asks for.
fn ci_request(args: &[String]) -> Result<CiTier, String> {
    match args.first().map(String::as_str) {
        Some("tier0") => Ok(CiTier::Zero),
        Some("prepush") => Ok(CiTier::PrePush),
        Some("smoke") => Ok(CiTier::Smoke),
        Some("no-data") => Ok(CiTier::NoData),
        Some("tier1") => Ok(CiTier::One),
        Some("tier2") => Ok(CiTier::Two),
        _ => Err("name a tier".into()),
    }
}

fn ci(args: &[String]) -> i32 {
    let profile = Profile::from_args(args);
    let parts = match Tier0Part::from_args(args) {
        Ok(parts) => parts,
        Err(e) => {
            eprintln!("ci: {e}\n{CI_USAGE}");
            return 2;
        }
    };
    if parts != Tier0Part::ALL && args.first().map(String::as_str) != Some("tier0") {
        eprintln!("ci: --only is for tier0\n{CI_USAGE}");
        return 2;
    }
    let list = args.iter().any(|a| a == "--list");
    let listable = matches!(
        ci_request(args),
        Ok(CiTier::Zero | CiTier::PrePush | CiTier::Smoke)
    );
    if list && !listable {
        eprintln!("ci: --list is for prepush, tier0 and smoke\n{CI_USAGE}");
        return 2;
    }
    if let Err(e) = ci_guards(args, list) {
        eprintln!("ci: {e}");
        return 1;
    }
    match ci_request(args) {
        Ok(CiTier::Zero) if list => print_plan(&tier0_plan(profile, &parts)),
        Ok(CiTier::Zero) => tier0_parts(profile, &parts, TIER0_TITLE),
        Ok(CiTier::PrePush) => prepush::prepush(args, profile),
        Ok(CiTier::Smoke) if list => {
            print_plan(&[format!("cargo {}", smoke_argv(profile).join(" "))])
        }
        Ok(CiTier::Smoke) => smoke(profile),
        Ok(CiTier::NoData) => no_data::no_data(no_data::commands(profile)),
        Ok(CiTier::One) => with_machine_lock("cargo xtask ci tier1", || tier1(args, profile)),
        Ok(CiTier::Two) => with_machine_lock("cargo xtask ci tier2", || tier2(args, profile)),
        Err(e) => {
            eprintln!("ci: {e}\n{CI_USAGE}");
            2
        }
    }
}

/// The battery guard of the tiers that load the machine: tier 0, tier 1 and tier 2 wait for mains
/// power unless told otherwise (`ci prepush` does not).
fn ci_guards(args: &[String], list: bool) -> Result<(), String> {
    match ci_request(args) {
        Ok(CiTier::Zero | CiTier::One | CiTier::Two) if !list => {
            guards::mains_power(&format!("cargo xtask ci {}", args[0]), args)
        }
        _ => Ok(()),
    }
}

/// `body`'s exit code, run holding a heavy hold on the machine lock (see [`guards`]).
fn with_machine_lock(what: &str, body: impl FnOnce() -> i32) -> i32 {
    match guards::machine_lock(pyreal_machine_lock::Mode::Heavy, what) {
        Ok(_lock) => body(),
        Err(e) => {
            eprintln!("{what}: {e}");
            1
        }
    }
}

/// The title of tier 0's table.
const TIER0_TITLE: &str = "CI tier 0 -- no data";

/// Prints a `--list` plan, one command a line. Exits 0: nothing ran.
fn print_plan(lines: &[String]) -> i32 {
    for line in lines {
        println!("{line}");
    }
    0
}

/// Clippy, but naming the crates responsible when it fails; its row is called `label`.
///
/// A workspace-wide gate that just says "clippy failed" makes everyone read the whole log to
/// find out whether the problem is theirs. This names the crates; whoever owns them decides.
fn clippy_step(ws: &std::path::Path, label: &str, argv: &[&str]) -> Report {
    let (ok, out) = util::run_captured(ws, "cargo", argv);
    clippy_report(label, ok, &out)
}

/// A clippy row from its success and its output, naming the crates responsible when it failed.
fn clippy_report(label: &str, ok: bool, out: &str) -> Report {
    if ok {
        return Report::new(label, Outcome::Pass, String::new());
    }
    print!("{out}");
    // With `-D warnings` a lint becomes an error, so the summary line reads
    //   error: could not compile `pyreal-client-net` (lib) due to 4 previous errors
    // Without it, the same crate reports
    //   warning: `pyreal-client-net` (lib) generated 4 warnings
    let mut crates: Vec<&str> = out
        .lines()
        .filter(|l| l.contains("could not compile ") || l.contains(") generated "))
        .filter_map(|l| l.split('`').nth(1))
        .collect();
    crates.sort_unstable();
    crates.dedup();
    let detail = if crates.is_empty() {
        "see the log above".to_owned()
    } else {
        format!("warnings in: {}", crates.join(", "))
    };
    Report::new(label, Outcome::Fail, detail)
}

/// Clippy over every crate and target, every warning an error. `retail-dats` is on, or
/// `--all-targets` would leave out every test binary that reads the retail dats (each crate's `dat`
/// and `gpu` tiers) without a word; it only selects targets, so nothing needs the data.
const CLIPPY_ARGS: &[&str] = &[
    "clippy",
    "--workspace",
    "--all-targets",
    "--features",
    "retail-dats",
    "--",
    "-D",
    "warnings",
];

/// The workspace is rustfmt-clean; this keeps it so.
const FMT_CHECK: &[&str] = &["fmt", "--all", "--check"];

/// `cargo doc` over the whole workspace, private items included.
const DOC_ARGS: &[&str] = &[
    "doc",
    "--workspace",
    "--no-deps",
    "--document-private-items",
];

/// Every rustdoc lint is an error except `private_intra_doc_links`.
///
/// Most of this workspace's readers are working on it, not calling it, and most of its items are
/// crate-private, so the docs are built with `--document-private-items` and a link from a public
/// item to a private one resolves and renders. The lint that flags such a link exists for crates
/// whose published docs omit private items; here it would only push the docs to stop naming the
/// code they describe. Everything else rustdoc reports (a link that resolves to nothing, an
/// ambiguous link, a bare URL, a stray HTML tag) is a real defect and fails the gate.
const DOC_FLAGS: &str = "-D warnings -A rustdoc::private_intra_doc_links";

/// `cargo doc` of the client runtime alone, with its own features: the workspace's docs turn on
/// its test support, which the client is shipped without.
const RUNTIME_DOC_ARGS: &[&str] = &[
    "doc",
    "-p",
    "pyreal-client-runtime",
    "--no-deps",
    "--document-private-items",
];

/// What the documentation gate adds to [`DOC_FLAGS`]: rustdoc reports everything it would and
/// writes no pages. The pages are the cost: a whole-workspace build writes about 80,000 files, and
/// each crate rewrites the shared search index under a lock, which took six times as long as the
/// check itself.
const DOC_CHECK_ONLY: &str = "--check -Zunstable-options";

/// While set, this program stands in for rustdoc, whose path is the value ([`doc_gate`]).
const RUSTDOC_STAND_IN: &str = "XTASK_RUSTDOC";

/// Run the documentation gate: `args` ([`DOC_ARGS`], or one crate's) with [`DOC_FLAGS`] as
/// `RUSTDOCFLAGS`, checking without writing pages.
///
/// That flag is unstable, which a stable rustdoc allows only under `RUSTC_BOOTSTRAP`. Setting that
/// for the whole of `cargo doc` would reach the build scripts of the dependencies, some of which
/// read it to turn on nightly features, so this program stands in for rustdoc and sets it for
/// rustdoc alone.
fn doc_gate(ws: &std::path::Path, args: &[&str]) -> bool {
    println!(
        "$ RUSTDOCFLAGS=\"{DOC_CHECK_ONLY} {DOC_FLAGS}\" cargo {}",
        args.join(" ")
    );
    let Ok(this_program) = std::env::current_exe() else {
        eprintln!("the documentation gate could not find its own program");
        return false;
    };
    let rustdoc = std::env::var_os("RUSTDOC").unwrap_or_else(|| "rustdoc".into());
    std::process::Command::new("cargo")
        .args(args)
        .env("RUSTDOCFLAGS", format!("{DOC_CHECK_ONLY} {DOC_FLAGS}"))
        .env("RUSTDOC", this_program)
        .env(RUSTDOC_STAND_IN, rustdoc)
        .current_dir(ws)
        .status()
        .is_ok_and(|s| s.success())
}

/// Stands in for `rustdoc`: runs it with this program's arguments and allows its unstable flags.
fn rustdoc_stand_in(rustdoc: &std::ffi::OsStr) -> i32 {
    std::process::Command::new(rustdoc)
        .args(std::env::args_os().skip(1))
        .env("RUSTC_BOOTSTRAP", "1")
        .status()
        .ok()
        .and_then(|s| s.code())
        .unwrap_or(1)
}

fn step(name: &str, ok: bool) -> Report {
    Report::new(
        name,
        if ok { Outcome::Pass } else { Outcome::Fail },
        String::new(),
    )
}

/// A whole-workspace source rule as a tier row: it passes when the check exits 0, and it runs
/// whatever came before it failed.
fn source_rule(name: &str, check: impl FnOnce() -> i32) -> Report {
    println!("--- {name}");
    let outcome = if check() == 0 {
        Outcome::Pass
    } else {
        Outcome::Fail
    };
    Report::new(name, outcome, String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|s| (*s).to_owned()).collect()
    }

    /// **Clippy lints the tests that read the retail dats too**: the workspace's clippy, which
    /// `xtask clippy` and tier 0 run, turns `retail-dats` on, so `--all-targets` takes in every
    /// crate's `dat` and `gpu` binaries.
    #[test]
    fn clippy_turns_retail_dats_on() {
        let features = CLIPPY_ARGS.iter().position(|a| *a == "--features");
        assert!(features
            .and_then(|i| CLIPPY_ARGS.get(i + 1))
            .is_some_and(|f| f.split(',').any(|f| f == "retail-dats")));
        assert!(CLIPPY_ARGS.contains(&"--all-targets"));
    }

    /// Tier 0's cargo rows, each as its label and its command.
    fn tier0_rows() -> Vec<(String, String)> {
        tier0_steps(Profile::TestRelease)
            .into_iter()
            .map(|s| match s {
                Tier0Step::Cargo(label, argv) => (label, argv.join(" ")),
                Tier0Step::Clippy(label, argv) => (format!("clippy: {label}"), argv.join(" ")),
                Tier0Step::Doc(label, argv) => (format!("doc: {label}"), argv.join(" ")),
            })
            .collect()
    }

    fn parts_of(args: &[&str]) -> Result<Vec<Tier0Part>, String> {
        Tier0Part::from_args(&args.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>())
    }

    /// **`--only` selects tier 0's parts**, and with none named every part runs, so the command
    /// a contributor types still runs what public CI runs across all its jobs.
    #[test]
    fn only_selects_tier0_parts_and_no_flag_means_all_of_them() {
        assert_eq!(parts_of(&["tier0"]).unwrap(), Tier0Part::ALL);
        assert_eq!(
            parts_of(&["tier0", "--only", "build,clippy"]).unwrap(),
            [Tier0Part::Build, Tier0Part::Clippy]
        );
        assert_eq!(parts_of(&["--only=docs"]).unwrap(), [Tier0Part::Docs]);
        assert!(parts_of(&["tier0", "--only", "bild"]).is_err());
        assert!(parts_of(&["tier0", "--only"]).is_err());
    }

    /// **Every cargo row of tier 0 belongs to the part that names it**: the steps of public CI's
    /// jobs together run every row.
    #[test]
    fn every_tier0_row_has_the_part_its_command_names() {
        for planned in tier0_steps(Profile::TestRelease) {
            let expected = match &planned {
                Tier0Step::Cargo(_, argv) => match argv[0] {
                    "build" => Tier0Part::Build,
                    "test" => Tier0Part::Test,
                    other => panic!("a cargo row runs {other}"),
                },
                Tier0Step::Clippy(..) => Tier0Part::Clippy,
                Tier0Step::Doc(..) => Tier0Part::Docs,
            };
            assert_eq!(planned.part(), expected);
        }
    }

    /// **A row says how long it took**, beside what it already says.
    #[test]
    fn a_tier0_rows_time_is_added_to_its_detail() {
        let mut rows = [
            Report::new("a", Outcome::Pass, ""),
            Report::new("b", Outcome::Pass, ""),
        ];
        say_time_taken(&mut rows, std::time::Duration::from_secs(125));
        assert_eq!(rows[0].detail, "took 2m 05s");
        assert_eq!(rows[1].detail, "");
        let mut rows = [Report::new("a", Outcome::Pass, "17.4s")];
        say_time_taken(&mut rows, std::time::Duration::from_secs(60));
        assert_eq!(rows[0].detail, "17.4s, took 1m 00s");
        say_time_taken(&mut [], std::time::Duration::ZERO);
    }

    /// **Tier 0 runs exactly these commands**, under these labels, and its tests run on past a
    /// failure, so one run reports them all.
    #[test]
    fn tier0_runs_these_commands() {
        let rows = tier0_rows();
        let expected = [
            (
                "cargo build --workspace",
                "build --workspace --profile test-release",
            ),
            (
                "cargo test --workspace",
                "test --workspace --no-fail-fast --profile test-release",
            ),
            (
                "clippy: cargo clippy --workspace --all-targets --features retail-dats",
                "clippy --workspace --all-targets --features retail-dats -- -D warnings",
            ),
            (
                "doc: xtask doc (rustdoc warnings denied)",
                "doc --workspace --no-deps --document-private-items",
            ),
            (
                "doc: xtask doc -p pyreal-client-runtime (the runtime as shipped)",
                "doc -p pyreal-client-runtime --no-deps --document-private-items",
            ),
        ]
        .map(|(l, c)| (l.to_owned(), c.to_owned()));
        assert_eq!(rows, expected);
    }

    /// **`--list` names every row a run would make, in the order it makes them**: `--only`'s order,
    /// and one line for the rules.
    #[test]
    fn a_plan_lists_the_rows_in_the_order_they_run() {
        let plan = tier0_plan(Profile::TestRelease, &[Tier0Part::Rules, Tier0Part::Test]);
        assert_eq!(plan.len(), 2);
        assert!(
            plan[0].starts_with("rules: fmt-check, lint,"),
            "{}",
            plan[0]
        );
        assert_eq!(
            plan[1],
            "cargo test --workspace: cargo test --workspace --no-fail-fast --profile test-release"
        );
        assert_eq!(tier0_plan(Profile::TestRelease, &Tier0Part::ALL).len(), 6);
        for name in RULE_NAMES {
            assert!(
                USAGE.contains(&format!("\n  {name} ")),
                "{name} is a task of its own"
            );
        }
    }

    /// **The Windows smoke run names workspace crates**, each once, so a renamed crate fails here
    /// rather than on CI.
    #[test]
    fn the_smoke_crates_are_workspace_crates() {
        let ws = workspace_root();
        for krate in SMOKE_CRATES {
            assert!(
                util::crate_dir(&ws, krate).join("Cargo.toml").is_file(),
                "{krate}"
            );
        }
        let mut sorted = SMOKE_CRATES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, SMOKE_CRATES);
        let argv = smoke_argv(Profile::TestRelease).join(" ");
        assert!(
            argv.starts_with("test --no-fail-fast -p pyreal-client-contract -p "),
            "{argv}"
        );
        assert!(argv.ends_with(" --profile test-release"), "{argv}");
        assert!(!argv.contains("--workspace"));
    }

    /// **`ci` takes a tier**, and nothing else in its place.
    #[test]
    fn ci_takes_a_tier() {
        for (words, want) in [
            (&["tier0"][..], CiTier::Zero),
            (&["prepush", "--hook"][..], CiTier::PrePush),
            (&["smoke"][..], CiTier::Smoke),
            (&["no-data"][..], CiTier::NoData),
            (&["tier1"], CiTier::One),
            (&["tier2", "--serial"], CiTier::Two),
        ] {
            assert_eq!(ci_request(&argv(words)), Ok(want), "{words:?}");
        }
        for bad in [&[][..], &["tier3"], &["nightly"], &["--serial", "tier1"]] {
            assert!(ci_request(&argv(bad)).is_err(), "{bad:?}");
        }
    }

    /// **The test tiers run in `test-release` unless `--debug` is given**, from either side of the
    /// gate name, and no near-miss selects the dev profile.
    #[test]
    fn the_tiers_run_in_test_release_unless_debug_is_given() {
        for cmd in [
            &["test"][..],
            &["gates"],
            &["gate", "client"],
            &["ci", "tier0"],
            &["ci", "tier1"],
        ] {
            assert_eq!(
                Profile::from_args(&argv(cmd)),
                Profile::TestRelease,
                "{cmd:?}"
            );
        }
        assert_eq!(
            Profile::TestRelease.cargo_args(),
            &["--profile", "test-release"]
        );
        for cmd in [
            &["test", "--debug"][..],
            &["gate", "--debug", "client"],
            &["gate", "client", "--debug"],
            &["ci", "tier1", "--serial", "--debug"],
        ] {
            assert_eq!(Profile::from_args(&argv(cmd)), Profile::Dev, "{cmd:?}");
        }
        assert!(
            Profile::Dev.cargo_args().is_empty(),
            "dev is cargo's default profile"
        );
        for near in [
            &["gates", "debug"][..],
            &["gates", "--debug=1"],
            &["gates", "--serial"],
        ] {
            assert_eq!(
                Profile::from_args(&argv(near)),
                Profile::TestRelease,
                "{near:?} is not `--debug`"
            );
        }
        assert_eq!(
            gate_name(&argv(&["gate", "--debug", "client"])),
            Some(&"client".to_owned()),
            "the flag is never mistaken for the gate"
        );
    }

    /// **`--serial` is opt-in and order-independent** (unit R-shard).
    ///
    /// The default has to be the sharded form -- a gate nobody types a flag for is the gate that
    /// actually runs -- and the flag has to be reachable from either side of the gate name,
    /// because `gate --serial client` and `gate client --serial` are both things a person types. The
    /// negative control is the one that matters: nothing else on the command line may turn
    /// sharding off by accident.
    #[test]
    fn serial_is_opt_in_from_either_side_of_the_gate_name() {
        assert_eq!(threads(&argv(&["gates"])), gates::Threads::Sharded);
        assert_eq!(threads(&argv(&["gate", "client"])), gates::Threads::Sharded);
        assert_eq!(
            threads(&argv(&["gates", "--serial"])),
            gates::Threads::Serial
        );
        assert_eq!(
            threads(&argv(&["gate", "--serial", "client"])),
            gates::Threads::Serial
        );
        assert_eq!(
            threads(&argv(&["gate", "client", "--serial"])),
            gates::Threads::Serial
        );
        assert_eq!(
            threads(&argv(&["ci", "tier1", "--serial"])),
            gates::Threads::Serial
        );
        // Negative control: a near-miss must not disarm the default.
        for near in [
            &["gates", "--milestone"][..],
            &["gates", "serial"],
            &["gates", "--serial=1"],
        ] {
            assert_eq!(
                threads(&argv(near)),
                gates::Threads::Sharded,
                "{near:?} is not `--serial` and must not turn sharding off"
            );
        }
        // And the flag must never be mistaken for the gate.
        assert_eq!(
            gate_name(&argv(&["gate", "--serial", "client"])),
            Some(&"client".to_owned())
        );
        assert_eq!(
            gate_name(&argv(&["gate", "client", "--serial"])),
            Some(&"client".to_owned())
        );
        assert_eq!(gate_name(&argv(&["gate"])), None);
        assert_eq!(
            gate_name(&argv(&["gate", "--serial"])),
            None,
            "a flag is not a gate"
        );
    }
}
