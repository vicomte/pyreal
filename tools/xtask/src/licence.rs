//! `cargo xtask licence [ROOT]`: the workspace is MIT throughout, checked mechanically.
//!
//! 1. Every workspace member's `license` is `MIT`.
//! 2. No copyleft provenance marker (a "ported from" ACE or GDLE header, or the copyleft licence's
//!    name) appears in a `.rs` or `.toml` file under [`MIT_DIRS`]: the server, a port of ACE, is
//!    kept apart from this repository, and none of its code may come in.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use crate::util::workspace_root;

const LICENSE: &str = "MIT";

/// The folders the provenance scan covers: every crate's.
const MIT_DIRS: &[&str] = &["core", "pyreal", "tools"];

/// The copyleft licence's short name, spelled in pieces so this file does not itself carry the
/// marker it looks for.
fn copyleft() -> String {
    ["A", "GPL"].concat()
}

/// The provenance marker ported files carry.
static MARKER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        "(?i)Ported from ({}|GDLE)|{}",
        ["ACE", "mulator"].concat(),
        copyleft()
    ))
    .expect("MARKER")
});

/// What a check found: the passes worth saying and the failures.
#[derive(Debug, Default)]
pub struct Findings {
    pub notes: Vec<String>,
    pub failures: Vec<String>,
}

fn name(v: &Value) -> &str {
    v["name"].as_str().unwrap_or("")
}

/// Rule 1, over `cargo metadata` output.
pub fn check_metadata(meta: &Value, f: &mut Findings) {
    let packages = meta["packages"].as_array().map_or(&[][..], Vec::as_slice);
    let before = f.failures.len();
    for p in packages {
        let have = p["license"].as_str();
        if have != Some(LICENSE) {
            f.failures.push(format!(
                "{} has license {have:?}, expected {LICENSE:?}",
                name(p)
            ));
        }
    }
    if f.failures.len() == before {
        f.notes
            .push(format!("{} packages, every one {LICENSE}", packages.len()));
    }
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Rule 2.
pub fn check_files(ws: &Path, f: &mut Findings) {
    let mut hits = Vec::new();
    for top in MIT_DIRS {
        for p in crate::util::source_files(&ws.join(top), &[]) {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "rs" && ext != "toml" {
                continue;
            }
            let body = std::fs::read(&p).unwrap_or_default();
            if MARKER.is_match(&String::from_utf8_lossy(&body)) {
                hits.push(rel(ws, &p));
            }
        }
    }
    if hits.is_empty() {
        f.notes.push(format!(
            "no {} provenance markers under {}",
            copyleft(),
            MIT_DIRS.join("/, ") + "/"
        ));
    } else {
        f.failures.push(format!(
            "{}/ACE provenance marker found (ported code in an MIT crate): {}",
            copyleft(),
            hits.join(", ")
        ));
    }
}

/// `cargo metadata` for the workspace at `ws`, offline and locked.
fn metadata(ws: &Path) -> Result<Value, String> {
    let out = Command::new("cargo")
        .args([
            "metadata",
            "--no-deps",
            "--locked",
            "--offline",
            "--format-version",
            "1",
        ])
        .current_dir(ws)
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "cargo metadata failed in {}:\n{}",
            ws.display(),
            err.trim().chars().take(400).collect::<String>()
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata: {e}"))
}

/// Run both rules over the workspace at `ws`.
pub fn check(ws: &Path) -> Findings {
    let mut f = Findings::default();
    match metadata(ws) {
        Ok(meta) => check_metadata(&meta, &mut f),
        Err(e) => f.failures.push(e),
    }
    check_files(ws, &mut f);
    f
}

/// Print the findings; the exit code.
pub fn report(f: &Findings) -> i32 {
    for n in &f.notes {
        println!("ok    {n}");
    }
    for x in &f.failures {
        println!("FAIL  {x}");
    }
    println!();
    if f.failures.is_empty() {
        println!("every crate is {LICENSE}");
        0
    } else {
        println!("{} licence rule(s) violated", f.failures.len());
        1
    }
}

/// `cargo xtask licence`.
pub fn licence(args: &[String]) -> i32 {
    if args.len() > 1 || args.iter().any(|a| a.starts_with("--")) {
        eprintln!("usage: cargo xtask licence [workspace root]");
        return 2;
    }
    let ws = args
        .first()
        .map_or_else(workspace_root, |r| PathBuf::from(r.as_str()));
    let ws = std::fs::canonicalize(&ws).unwrap_or(ws);
    let ws = PathBuf::from(
        ws.to_string_lossy()
            .strip_prefix(r"\\?\")
            .map_or_else(|| ws.to_string_lossy().into_owned(), str::to_owned),
    );
    report(&check(&ws))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Every member must be MIT; any other licence, or none, is named.
    #[test]
    fn every_crate_must_be_mit() {
        let pkg = |n: &str, l: Option<&str>| json!({ "name": n, "license": l });
        let mut clean = Findings::default();
        check_metadata(
            &json!({ "packages": [pkg("pyreal-a", Some("MIT")), pkg("xtask", Some("MIT"))] }),
            &mut clean,
        );
        assert!(clean.failures.is_empty(), "{:?}", clean.failures);
        let mut bad = Findings::default();
        check_metadata(
            &json!({ "packages": [
                pkg("pyreal-a", Some("MIT")),
                pkg("pyreal-b", Some("MIT OR Apache-2.0")),
                pkg("pyreal-c", None),
            ] }),
            &mut bad,
        );
        assert_eq!(bad.failures.len(), 2, "{:?}", bad.failures);
        assert!(bad.failures[0].contains("pyreal-b"), "{:?}", bad.failures);
    }

    /// A provenance marker anywhere under a scanned folder is refused; a tree without one is
    /// clean.
    #[test]
    fn a_provenance_marker_is_refused() {
        let root = std::env::temp_dir().join(format!("xtask-licence-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(root.join("pyreal/example/src")).expect("dirs");
        let lib = root.join("pyreal/example/src/lib.rs");
        std::fs::write(&lib, "pub fn example() {}\n").expect("write");
        let mut clean = Findings::default();
        check_files(&root, &mut clean);
        assert!(clean.failures.is_empty(), "{:?}", clean.failures);
        for marker in [
            format!("// {}\n", copyleft()),
            format!("//! Ported from {}\n", ["ACE", "mulator"].concat()),
        ] {
            std::fs::write(&lib, format!("{marker}pub fn example() {{}}\n")).expect("write");
            let mut leaked = Findings::default();
            check_files(&root, &mut leaked);
            assert!(
                leaked
                    .failures
                    .iter()
                    .any(|x| x.contains("pyreal/example/src/lib.rs")),
                "{:?}",
                leaked.failures
            );
        }
        std::fs::remove_dir_all(&root).ok();
    }
}
