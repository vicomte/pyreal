//! What the CI workflows must keep doing, checked by reading them, because none of it can be tried
//! until a workflow reaches its forge.

use crate::util::workspace_root;

fn read(path: &str) -> String {
    std::fs::read_to_string(workspace_root().join(path)).expect(path)
}

/// The text of the job called `name` in a workflow, up to the next job.
fn job<'a>(workflow: &'a str, name: &str) -> &'a str {
    let jobs = workflow.split_once("\njobs:\n").expect("a jobs table").1;
    let start = jobs
        .find(&format!("  {name}:\n"))
        .unwrap_or_else(|| panic!("no job {name}"));
    let body = &jobs[start + name.len() + 4..];
    let end = body
        .match_indices("\n  ")
        .find(|(at, _)| {
            let rest = &body[at + 3..];
            let line = rest.lines().next().unwrap_or_default();
            !rest.starts_with(' ') && !line.starts_with('#') && line.ends_with(':')
        })
        .map_or(body.len(), |(at, _)| at);
    &body[..end]
}

/// The tier 0 parts a job's steps name with `--only`, in order.
fn parts(job: &str) -> Vec<&str> {
    job.lines()
        .filter_map(|line| line.split_once("cargo xtask ci tier0 --only ").map(|l| l.1))
        .flat_map(|list| list.split(','))
        .map(str::trim)
        .collect()
}

/// **CI runs each check where it pays**: the tests and clippy on Linux, clippy and a smoke set on
/// Windows,
/// the rules and the docs once, and nothing on a Mac, whose checks `ci prepush` runs before a
/// push. Dropping a part from a job would pass silently otherwise.
#[test]
fn ci_runs_tests_and_clippy_on_linux_clippy_and_smoke_tests_on_windows_and_the_rules_once() {
    let ci = read(".github/workflows/ci.yml");
    let linux = job(&ci, "linux");
    assert_eq!(parts(linux), ["test", "clippy"]);
    assert!(linux.contains("runs-on: ubuntu-latest"));
    let windows = job(&ci, "windows");
    assert!(windows.contains("runs-on: windows-latest"));
    assert_eq!(parts(windows), ["clippy"]);
    assert!(windows.contains("run: cargo xtask ci smoke\n"));
    let rules = job(&ci, "rules");
    assert_eq!(parts(rules), ["rules", "docs"]);
    assert!(rules.contains("runs-on: ubuntu-latest"));
    for (name, job) in [("linux", linux), ("windows", windows), ("rules", rules)] {
        assert!(!job.contains("matrix"), "{name}: once, on one system");
    }
    assert!(
        !ci.contains("macos"),
        "the Mac's checks run before the push"
    );
}

/// **Every package's programs start on the system that built them**, the one run of the shipped
/// build on the Mac and Windows.
#[test]
fn every_package_job_starts_the_programs_it_packaged() {
    let release = read(".github/workflows/release.yml");
    let package = job(&release, "package");
    let built = package
        .find("cargo xtask package pyreal")
        .expect("the package step");
    let started = package
        .find("name: The packaged programs start")
        .expect("the start step");
    assert!(built < started);
    let step = &package[started..];
    assert!(step.contains("foreach ($name in 'pyreal', 'pyreal-plugin')"));
    assert!(step.contains("-ArgumentList '--version' -Wait"));
    assert!(step.contains("$run.ExitCode -ne 0"));
}

/// **A step that fails does not hide the next one's failures**: every part after the first runs
/// whatever came before it, as tier 0's rows do.
#[test]
fn a_failed_part_of_public_ci_does_not_stop_the_parts_after_it() {
    let ci = read(".github/workflows/ci.yml");
    for name in ["linux", "windows", "rules"] {
        let job = job(&ci, name);
        let lines: Vec<&str> = job.lines().collect();
        let mut seen = 0;
        for (at, line) in lines.iter().enumerate() {
            if !line.contains("run: cargo xtask ci ") {
                continue;
            }
            seen += 1;
            if seen > 1 {
                assert!(
                    lines[at.saturating_sub(4)..at]
                        .iter()
                        .any(|l| l.contains("if: ${{ !cancelled() }}")),
                    "{name}: {line}"
                );
            }
        }
        assert!(seen >= 2, "{name}");
    }
}

/// **sccache can only make a build faster, never fail one**: its install may fail, and the compiler
/// is only wrapped by the step that saw its server start, never in a job's `env`.
#[test]
fn a_failure_of_sccache_or_its_cache_service_leaves_the_build_running_without_it() {
    let ci = read(".github/workflows/ci.yml");
    for name in ["linux", "windows", "rules"] {
        let job = job(&ci, name);
        let install = job.find("name: Install sccache").expect("the install");
        let use_it = job.find("name: Use sccache if it starts").expect("the use");
        let build = job.find("run: cargo xtask ci ").expect("the build");
        assert!(install < use_it && use_it < build, "{name}");
        assert!(job[install..use_it].contains("continue-on-error: true"));
        let use_step = &job[use_it..build];
        assert!(use_step.contains("if: steps.sccache.outcome == 'success'"));
        assert!(use_step.contains("continue-on-error: true"));
        assert!(use_step.contains("sccache --start-server"));
        let started = use_step.find("$LASTEXITCODE -eq 0").expect("the check");
        let wrapped = use_step.find("RUSTC_WRAPPER=sccache").expect("the wrapper");
        assert!(started < wrapped, "{name}");
        assert_eq!(
            job.matches("RUSTC_WRAPPER=").count(),
            1,
            "{name}: only the step that saw it start"
        );
        assert!(
            job.contains("SCCACHE_IGNORE_SERVER_IO_ERROR: '1'"),
            "{name}"
        );
        assert!(job.contains("CARGO_INCREMENTAL: '0'"), "{name}");
    }
}

/// The `on:` table of a workflow, up to the next top-level key.
fn triggers(workflow: &str) -> &str {
    let on = workflow.split_once("\non:\n").expect("an on table").1;
    let end = on
        .match_indices('\n')
        .find(|(at, _)| on[at + 1..].starts_with(|c: char| c.is_ascii_alphabetic()))
        .map_or(on.len(), |(at, _)| at);
    &on[..end]
}

/// **Only a version tag builds**: no workflow runs on an ordinary push or pull request; the release
/// workflow runs on `v*` tags and calls the checks, and both can be run by hand.
#[test]
fn nothing_builds_on_a_push_or_pull_request_and_a_version_tag_runs_the_release() {
    for path in std::fs::read_dir(workspace_root().join(".github/workflows")).expect("workflows") {
        let path = path.expect("an entry").path();
        let text = std::fs::read_to_string(&path).expect("a workflow");
        let on = triggers(&text);
        let name = path.display();
        assert!(!on.contains("pull_request"), "{name}: {on}");
        assert!(!on.contains("branches"), "{name}: {on}");
        assert!(on.contains("workflow_dispatch:"), "{name}: {on}");
    }
    let ci = read(".github/workflows/ci.yml");
    let on = triggers(&ci);
    assert!(
        on.contains("workflow_call:") && !on.contains("push:"),
        "{on}"
    );
    let release = read(".github/workflows/release.yml");
    assert!(triggers(&release).contains("tags: ['v*']"));
    assert!(release.contains("uses: ./.github/workflows/ci.yml"));
}
