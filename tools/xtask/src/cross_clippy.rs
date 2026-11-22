//! `cargo xtask cross-clippy`: clippy for Windows and Linux from this machine, so code that only
//! builds for one system fails before a push rather than in CI.
//!
//! Rust's standard library for each target comes from rustup (`rustup target add <target>`). The
//! C in a few dependencies (zstd, Lua, BLAKE3, ring) would need each system's C toolchain, so it is
//! stood in for: clippy never links, so a compiler that writes empty objects, an archiver that
//! writes empty archives and a `pkg-config` that finds every library let the build scripts finish.
//! The C itself is not checked here; CI builds it. A target whose standard library is missing is
//! reported NO-HOST with the command that adds it.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::util::{workspace_root, NotRun, Outcome, Report};

/// The targets checked, besides the host's own.
pub(crate) const TARGETS: [&str; 2] = ["x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu"];

/// Writes an empty object wherever one is asked for, and nothing for `-E`.
const STUB_CC: &str = r#"#!/bin/sh
# Written by `cargo xtask cross-clippy`: a C compiler that writes empty objects.
out=""; prev=""
for a in "$@"; do
  [ "$prev" = "-o" ] && out="$a"
  case "$a" in -E|/E|-EP|/EP) exit 0;; -Fo*|/Fo*) out="${a#??}"; out="${out#o}";; esac
  prev="$a"
done
[ -n "$out" ] && : > "$out"
exit 0
"#;

/// Writes an empty archive, named by `/OUT:` or by the argument after `ar`'s operation.
const STUB_AR: &str = r#"#!/bin/sh
# Written by `cargo xtask cross-clippy`: an archiver that writes empty archives.
out=""
for a in "$@"; do case "$a" in /OUT:*|-OUT:*|/out:*|-out:*) out="${a#*:}";; esac; done
[ -z "$out" ] && out="$2"
[ -n "$out" ] && printf '!<arch>\n' > "$out"
exit 0
"#;

/// Finds every library, with no flags.
const STUB_PKG_CONFIG: &str = r#"#!/bin/sh
# Written by `cargo xtask cross-clippy`: a pkg-config that finds every library.
for a in "$@"; do case "$a" in --modversion) echo 99.0; exit 0;; --version) echo 0.29.2; exit 0;; esac; done
echo
"#;

/// The targets checked from this host.
fn targets() -> Vec<&'static str> {
    let host = host_triple();
    TARGETS
        .into_iter()
        .filter(|t| Some(*t) != host.as_deref())
        .collect()
}

fn host_triple() -> Option<String> {
    let out = Command::new("rustc").arg("-vV").output().ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("host: ").map(str::to_owned))
}

/// `cargo <argv>` for `target`.
pub(crate) fn argv(target: &str) -> Vec<&str> {
    let mut argv = crate::CLIPPY_ARGS.to_vec();
    let dashes = argv.iter().position(|a| *a == "--").unwrap_or(argv.len());
    argv.splice(dashes..dashes, ["--target", target]);
    argv
}

fn label(target: &str) -> String {
    format!("cargo clippy --target {target}")
}

/// What [`reports`] would run, for `--list`.
pub(crate) fn plan() -> Vec<String> {
    targets()
        .into_iter()
        .map(|t| format!("{}: cargo {} (C stood in)", label(t), argv(t).join(" ")))
        .collect()
}

/// Whether rustup has the standard library for `target`.
fn std_installed(target: &str) -> bool {
    let Ok(out) = Command::new("rustc").args(["--print", "sysroot"]).output() else {
        return false;
    };
    let sysroot = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    sysroot
        .join("lib/rustlib")
        .join(target)
        .join("lib")
        .is_dir()
}

/// The stand-in tools' folder, written afresh. It must hold no whitespace: the `cc` crate splits
/// a compiler's path on it.
fn stubs() -> Result<PathBuf, String> {
    let dir = pyreal_machine_lock::cache_dir()
        .ok_or("no home folder for the cache")?
        .join("cross-clippy");
    if dir.to_string_lossy().chars().any(char::is_whitespace) {
        return Err(format!("{} has a space in it", dir.display()));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for (name, text) in [
        ("cc", STUB_CC),
        ("ar", STUB_AR),
        ("pkg-config", STUB_PKG_CONFIG),
    ] {
        write_executable(&dir.join(name), text)?;
    }
    Ok(dir)
}

#[cfg(unix)]
fn write_executable(path: &Path, text: &str) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, text)
        .and_then(|()| std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)))
        .map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(not(unix))]
fn write_executable(_: &Path, _: &str) -> Result<(), String> {
    Err("the stand-in C tools are shell scripts, run from macOS or Linux".to_owned())
}

/// The variables that point `target`'s build scripts at the stand-ins in `stubs`.
fn stub_env(target: &str, stubs: &Path) -> Vec<(String, PathBuf)> {
    let t = target.replace('-', "_");
    vec![
        (format!("CC_{t}"), stubs.join("cc")),
        (format!("CXX_{t}"), stubs.join("cc")),
        (format!("AR_{t}"), stubs.join("ar")),
        (format!("PKG_CONFIG_{t}"), stubs.join("pkg-config")),
        ("PKG_CONFIG_ALLOW_CROSS".to_owned(), PathBuf::from("1")),
    ]
}

/// One row per target.
pub(crate) fn reports() -> Vec<Report> {
    let ws = workspace_root();
    let stubs = stubs();
    let mut rows = Vec::new();
    for target in targets() {
        let label = label(target);
        if !std_installed(target) {
            rows.push(Report::new(
                label,
                Outcome::NotRun(NotRun::Unhostable),
                format!("no standard library for it: `rustup target add {target}`"),
            ));
            continue;
        }
        let dir = match &stubs {
            Ok(dir) => dir,
            Err(e) => {
                rows.push(Report::new(
                    label,
                    Outcome::NotRun(NotRun::Unhostable),
                    e.clone(),
                ));
                continue;
            }
        };
        let started = std::time::Instant::now();
        let argv = argv(target);
        let mut row = match Command::new("cargo")
            .args(&argv)
            .envs(stub_env(target, dir))
            // sccache refuses the stub C compiler; empty, not unset, so a cargo wrapper cannot set it again.
            .env("RUSTC_WRAPPER", "")
            .env("CARGO_BUILD_RUSTC_WRAPPER", "")
            .current_dir(&ws)
            .output()
        {
            Ok(out) => {
                let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
                text.push_str(&String::from_utf8_lossy(&out.stderr));
                crate::clippy_report(&label, out.status.success(), &text)
            }
            Err(e) => Report::new(
                &label,
                Outcome::Fail,
                format!("failed to launch cargo: {e}"),
            ),
        };
        crate::say_time_taken(std::slice::from_mut(&mut row), started.elapsed());
        rows.push(row);
    }
    rows
}

/// `cargo xtask cross-clippy`.
pub(crate) fn cross_clippy() -> i32 {
    crate::util::print_table("Clippy for the other systems", &reports())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The target goes before clippy's own flags**, with the workspace, every target and
    /// `-D warnings` kept.
    #[test]
    fn the_target_goes_before_clippys_own_flags() {
        let argv = argv("x86_64-pc-windows-msvc");
        let at = argv.iter().position(|a| *a == "--target").unwrap();
        assert_eq!(argv[at + 1], "x86_64-pc-windows-msvc");
        assert!(at < argv.iter().position(|a| *a == "--").unwrap());
        assert!(argv.ends_with(&["--", "-D", "warnings"]));
        assert!(argv.contains(&"--workspace") && argv.contains(&"--all-targets"));
    }

    /// **The stand-ins are named for the target alone**, so the host's own C is built as usual.
    #[test]
    fn the_stand_ins_are_for_the_target_alone() {
        let env = stub_env("x86_64-unknown-linux-gnu", Path::new("/stubs"));
        let names: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            names,
            [
                "CC_x86_64_unknown_linux_gnu",
                "CXX_x86_64_unknown_linux_gnu",
                "AR_x86_64_unknown_linux_gnu",
                "PKG_CONFIG_x86_64_unknown_linux_gnu",
                "PKG_CONFIG_ALLOW_CROSS",
            ]
        );
    }

    /// **The stand-in compiler and archiver write what a build script expects to find.**
    #[cfg(unix)]
    #[test]
    fn the_stand_ins_write_an_object_and_an_archive() {
        let dir = std::env::temp_dir().join(format!("xtask-stubs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        write_executable(&dir.join("cc"), STUB_CC).unwrap();
        write_executable(&dir.join("ar"), STUB_AR).unwrap();
        let run = |tool: &str, args: &[&str]| {
            assert!(Command::new(dir.join(tool))
                .args(args)
                .status()
                .unwrap()
                .success());
        };
        let gnu_obj = dir.join("a.o");
        run("cc", &["-c", "x.c", "-o", gnu_obj.to_str().unwrap()]);
        assert!(gnu_obj.is_file());
        let msvc_obj = dir.join("b.obj");
        run("cc", &["-c", "x.c", &format!("-Fo{}", msvc_obj.display())]);
        assert!(msvc_obj.is_file());
        let gnu_lib = dir.join("liba.a");
        run("ar", &["cq", gnu_lib.to_str().unwrap(), "a.o"]);
        assert_eq!(std::fs::read(&gnu_lib).unwrap(), b"!<arch>\n");
        let msvc_lib = dir.join("b.lib");
        run("ar", &[&format!("-out:{}", msvc_lib.display()), "b.obj"]);
        assert_eq!(std::fs::read(&msvc_lib).unwrap(), b"!<arch>\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
