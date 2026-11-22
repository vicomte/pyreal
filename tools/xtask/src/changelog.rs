//! `cargo xtask changelog`: a branch that changes what players get adds to the changelog.
//!
//! The branch is compared with its merge base with `main` (on `main` itself, with `origin/main`).
//! When a file the player gets has changed (the client and shared crates' code, the shaders and
//! the options, and the base pack's pin; not tests, tools, docs or notes), `CHANGELOG.md`'s
//! Unreleased section must have changed too. A branch with nothing for players to read says so
//! with a `Changelog: none` trailer on one of its commits, or with `PYREAL_CHANGELOG=none`.

use std::process::Command;

use crate::util::{print_table, workspace_root, NotRun, Outcome, Report};

/// The variable that marks a branch as having nothing for the changelog.
const ENV: &str = "PYREAL_CHANGELOG";

/// The row's label.
const LABEL: &str = "xtask changelog";

/// The folders whose files players get, and the single files.
const PLAYER_FACING_DIRS: &[&str] = &["core/", "pyreal/client/", "pyreal/desktop/"];
const PLAYER_FACING_FILES: &[&str] = &["base-pack.toml"];

/// Folders within those that players do not get.
const INTERNAL_DIRS: &[&str] = &["tests", "benches", "examples", "fixtures"];

/// `cargo xtask changelog`.
pub(crate) fn changelog() -> i32 {
    print_table("Changelog", &[report()])
}

/// The check as one row.
pub(crate) fn report() -> Report {
    match check() {
        Ok((outcome, detail)) => Report::new(LABEL, outcome, detail),
        Err(e) => Report::new(LABEL, Outcome::Fail, e),
    }
}

/// Whether players get the tracked file at `path`.
fn player_facing(path: &str) -> bool {
    let included = PLAYER_FACING_FILES.contains(&path)
        || PLAYER_FACING_DIRS.iter().any(|d| path.starts_with(d));
    let (dirs, name) = path.rsplit_once('/').unwrap_or(("", path));
    let internal = dirs.split('/').any(|d| INTERNAL_DIRS.contains(&d))
        || name == "tests.rs"
        || name.ends_with("_tests.rs")
        || name.ends_with(".md")
        || name == "Cargo.toml";
    included && !internal
}

/// The Unreleased section of a changelog, heading to the next version's heading.
fn unreleased(changelog: &str) -> Option<String> {
    let start = changelog.find("\n## [Unreleased]")? + 1;
    let rest = &changelog[start..];
    let end = rest[1..].find("\n## ").map_or(rest.len(), |at| at + 1);
    Some(rest[..end].to_owned())
}

/// Whether `git log`'s `Changelog` trailer values mark the branch as having nothing to add.
fn marked_none(trailers: &str) -> bool {
    trailers
        .lines()
        .any(|v| v.trim().eq_ignore_ascii_case("none"))
}

fn git(args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(workspace_root())
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_owned())
}

/// The ref the branch is compared with: `main`, or `origin/main` on `main` itself or when there is
/// no local `main`.
fn base_ref() -> Option<&'static str> {
    let exists = |r: &str| git(&["rev-parse", "--verify", "--quiet", r]).is_ok();
    let branch = git(&["symbolic-ref", "--short", "-q", "HEAD"]).unwrap_or_default();
    if branch != "main" && exists("main") {
        Some("main")
    } else if exists("origin/main") {
        Some("origin/main")
    } else {
        None
    }
}

fn check() -> Result<(Outcome, String), String> {
    let Some(base_ref) = base_ref() else {
        return Ok((
            Outcome::NotRun(NotRun::Unhostable),
            "no main or origin/main to compare with".to_owned(),
        ));
    };
    let base = git(&["merge-base", "HEAD", base_ref])?;
    let short = &base[..base.len().min(8)];
    let changed: Vec<String> = git(&["diff", "--name-only", &base])?
        .lines()
        .filter(|p| player_facing(p))
        .map(str::to_owned)
        .collect();
    if changed.is_empty() {
        return Ok((
            Outcome::Pass,
            format!("nothing players get changed since {short} ({base_ref})"),
        ));
    }
    let before = git(&["show", &format!("{base}:CHANGELOG.md")])?;
    let now = std::fs::read_to_string(workspace_root().join("CHANGELOG.md"))
        .map_err(|e| format!("CHANGELOG.md: {e}"))?;
    if unreleased(&before) != unreleased(&now) {
        return Ok((
            Outcome::Pass,
            "the Unreleased section is updated".to_owned(),
        ));
    }
    if std::env::var(ENV).is_ok_and(|v| v.eq_ignore_ascii_case("none")) {
        return Ok((Outcome::Pass, format!("{ENV}=none: nothing for players")));
    }
    let trailers = git(&[
        "log",
        "--format=%(trailers:key=Changelog,valueonly)",
        &format!("{base}..HEAD"),
    ])?;
    if marked_none(&trailers) {
        return Ok((
            Outcome::Pass,
            "a commit's `Changelog: none` trailer: nothing for players".to_owned(),
        ));
    }
    let mut named = changed
        .iter()
        .take(3)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if changed.len() > 3 {
        named.push_str(&format!(" and {} more", changed.len() - 3));
    }
    Ok((
        Outcome::Fail,
        format!(
            "{named} changed since {short} ({base_ref}) but CHANGELOG.md's Unreleased section did \
             not: add an entry, or mark the branch internal with a `Changelog: none` commit \
             trailer or {ENV}=none"
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Players get the client's code, shaders and options; not its tests, docs or tools.**
    #[test]
    fn player_facing_files_are_the_shipped_code() {
        for path in [
            "pyreal/client/src/options/display.rs",
            "pyreal/client/crates/engine/src/shaders/sky.wgsl",
            "core/physics/src/collide.rs",
            "pyreal/desktop/src/main.rs",
            "base-pack.toml",
        ] {
            assert!(player_facing(path), "{path}");
        }
        for path in [
            "core/physics/tests/cpu/main.rs",
            "core/client-net/examples/isaac_state.rs",
            "core/client-runtime/src/app/tests/login.rs",
            "pyreal/client/src/settings_tests.rs",
            "pyreal/client/src/ui/tests.rs",
            "pyreal/client/INPUT-REPLAY.md",
            "pyreal/client/Cargo.toml",
            "pyreal/testkit/src/lib.rs",
            "tools/engine-view/src/main.rs",
            "docs/BUILDING.md",
            "CHANGELOG.md",
            "notes/plans/x.md",
        ] {
            assert!(!player_facing(path), "{path}");
        }
    }

    /// **The Unreleased section runs from its heading to the next version's**, so an entry under
    /// a released version does not count.
    #[test]
    fn the_unreleased_section_ends_at_the_next_version() {
        let text = "# Changelog\n\nIntro.\n\n## [Unreleased]\n\n### Added\n\n- A thing.\n\n\
                    ## [0.2.0] - 2026-11-21\n\n- Old.\n";
        assert_eq!(
            unreleased(text).as_deref(),
            Some("## [Unreleased]\n\n### Added\n\n- A thing.\n")
        );
        let older = text.replace("- Old.", "- Older.");
        assert_eq!(unreleased(text), unreleased(&older));
        assert_eq!(
            unreleased("# Changelog\n\n## [Unreleased]\n").as_deref(),
            Some("## [Unreleased]\n")
        );
        assert_eq!(unreleased("# Changelog\n"), None);
    }

    /// **Only a `Changelog: none` trailer opts out**: a title that starts `Changelog:` is no
    /// trailer, and git gives the trailers' values alone.
    #[test]
    fn only_a_none_trailer_opts_out() {
        assert!(marked_none("\nnone\n\n"));
        assert!(marked_none("None"));
        assert!(!marked_none(""));
        assert!(!marked_none("far buildings kept to the land's edge"));
    }
}
