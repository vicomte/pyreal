//! `cargo xtask package`: the release files of Pyreal (the client, the art pack and the plugin
//! author's tool), built the same way on a contributor's machine and in a
//! release workflow ([`pyreal`]), and what they share: the build facts, the targets, the
//! deny scan, the header checks, the archives and the notices.

pub(crate) mod archive;
pub mod dmg;
mod finder;
mod guard;
mod headers;
mod notice;
pub mod pyreal;
pub(crate) mod targets;
pub mod version;
// The build hash's rule, shared with the client's build script.
#[path = "../../../../pyreal/client/build_hash.rs"]
pub mod build_hash;

#[cfg(test)]
mod pyreal_tests;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::util::target_dir;
use targets::{Host, Os};

/// What the build stamps into the binaries, and what the index records.
#[derive(Debug, Clone)]
pub struct BuildFacts {
    pub commit: String,
    /// The build hash the client compiles in ([`build_hash`]): the one `build.py` hands on,
    /// else the commit's own; empty without git.
    pub hash: String,
    /// Seconds since 1970: `SOURCE_DATE_EPOCH`, else the commit's time.
    pub epoch: i64,
    /// The public repository the release files are downloaded from ([`pyreal::SOURCE_URL_ENV`]);
    /// empty when the environment names none: no upstream is baked in.
    pub source_url: String,
}

impl BuildFacts {
    /// ISO 8601, UTC.
    pub fn iso_time(&self) -> String {
        let (y, mo, d, h, mi, s) = archive::civil(self.epoch);
        format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
    }
}

/// Run `git` in the workspace; its trimmed output, or why it failed.
pub fn git(ws: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(ws)
        .output()
        .map_err(|e| format!("git could not be run: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "`git {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// The tracked files with uncommitted changes (`git status`), untracked files not counted.
pub fn uncommitted(ws: &Path) -> Result<Vec<String>, String> {
    Ok(git(ws, &["status", "--porcelain", "--untracked-files=no"])?
        .lines()
        .map(str::to_owned)
        .collect())
}

/// The facts of the checked-out commit. Uncommitted changes are refused unless `allow_dirty`.
pub fn build_facts(ws: &Path, allow_dirty: bool) -> Result<BuildFacts, String> {
    let dirty = uncommitted(ws)?;
    if !dirty.is_empty() && !allow_dirty {
        return Err(format!(
            "the tree has uncommitted changes, so the binaries would not be the commit they name \
             ({}); commit them, or pass --allow-dirty for a local trial",
            dirty.join("; ")
        ));
    }
    let commit = git(ws, &["rev-parse", "HEAD"])?;
    let hash = build_hash::pick(std::env::var(build_hash::ENV).ok().as_deref(), || {
        git(ws, &build_hash::GIT_ARGS).ok()
    })
    .unwrap_or_default();
    let epoch = match std::env::var("SOURCE_DATE_EPOCH") {
        Ok(v) => v
            .trim()
            .parse::<i64>()
            .map_err(|_| format!("SOURCE_DATE_EPOCH is not a number of seconds: `{v}`"))?,
        Err(_) => git(ws, &["show", "-s", "--format=%ct", "HEAD"])?
            .parse::<i64>()
            .map_err(|_| "the commit's time is not a number".to_owned())?,
    };
    let source_url = match std::env::var(pyreal::SOURCE_URL_ENV) {
        Ok(u) if !u.trim().is_empty() => u.trim().trim_end_matches('/').to_owned(),
        _ => String::new(),
    };
    Ok(BuildFacts {
        commit,
        hash,
        epoch,
        source_url,
    })
}

/// What this host can build with.
fn host() -> Host {
    let installed = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|l| l.trim().to_owned())
                .filter(|l| !l.is_empty())
                .collect()
        });
    Host {
        os: Os::host(),
        installed,
        glibc: host_glibc(),
    }
}

/// The glibc this host's programs link against, on Linux; none elsewhere, or when `getconf`
/// cannot say.
fn host_glibc() -> Option<(u32, u32)> {
    if Os::host() != Some(Os::Linux) {
        return None;
    }
    let out = Command::new("getconf")
        .arg("GNU_LIBC_VERSION")
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    targets::glibc_of(&String::from_utf8_lossy(&out.stdout))
}

/// The host's own target triple (`rustc -vV`).
fn host_triple() -> Result<String, String> {
    let out = Command::new("rustc")
        .arg("-vV")
        .output()
        .map_err(|e| format!("rustc could not be run: {e}"))?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .map(|h| h.trim().to_owned())
        .ok_or_else(|| "rustc -vV named no host".to_owned())
}

/// The local folders a build would otherwise name in the binaries: the checkout, the target
/// folder (where build scripts write the source they generate, and which can be anywhere),
/// cargo's crate cache (`CARGO_HOME`, else `~/.cargo`) and the toolchain (`rustc --print
/// sysroot`), whose library source the compiler names when it is installed.
///
/// The compiler applies the last remap that matches, so the target folder, which is often inside
/// the checkout, comes after it.
fn local_remaps(ws: &Path) -> Vec<(PathBuf, &'static str)> {
    let mut remaps = vec![(ws.to_path_buf(), "/pyreal")];
    let target = target_dir();
    if target != ws {
        remaps.push((target, "/target"));
    }
    let cargo_home = std::env::var("CARGO_HOME")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .ok()
                .map(|h| PathBuf::from(h).join(".cargo"))
        });
    if let Some(home) = cargo_home {
        remaps.push((home, "/cargo"));
    }
    if let Some(sysroot) = Command::new("rustc")
        .args(["--print", "sysroot"])
        .current_dir(ws)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|s| !s.is_empty())
    {
        remaps.push((PathBuf::from(sysroot), "/rustc/sysroot"));
    }
    remaps
}

/// `cargo xtask package ...`.
pub fn package(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("pyreal") => pyreal::package(&args[1..]),
        _ => {
            eprintln!("{}", pyreal::USAGE);
            2
        }
    }
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("reading {}: {e}", path.display()))
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("writing {}: {e}", path.display()))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
