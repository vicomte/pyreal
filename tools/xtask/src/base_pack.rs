//! `cargo xtask base-pack`: the base art pack the app carries, made from the pack repository's
//! source at the commit [`PIN_FILE`] names.
//!
//! - `build` makes `<target>/base-pack/base-pack.dat` when it is not already the pinned pack: the
//!   commit's files are read out of the repository (its LFS files from the repository's own LFS
//!   store), checked with the repository's `build.py --provenance-only` and against its
//!   `tables/drawn-as-eor.tsv` ([`drawn_as_eor_agrees`]), packed with `pyreal-art` in layout 2 (each
//!   record compressed) and refused unless they hash to the pin.
//! - `update [<rev>]` packs the repository at `<rev>` (its `HEAD` by default) the same way and pins
//!   what it made.
//!
//! The repository is only read: nothing is checked out, fetched or written there.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use sha2::{Digest, Sha256};

use crate::util::{target_dir, workspace_root};

/// The pin, at the workspace's root.
pub const PIN_FILE: &str = "base-pack.toml";

/// The packed file's name, which the client looks for (`pyreal_desktop::folders::BASE_PACK_FILE`).
pub const FILE: &str = "base-pack.dat";

/// The folder in the target directory that holds the packed file, and the source while it packs.
const DIR: &str = "base-pack";

/// The first line of a Git LFS pointer.
const LFS_POINTER: &str = "version https://git-lfs.github.com/spec/v1\n";

const USAGE: &str = "usage: cargo xtask base-pack build | update [<rev>]";

/// The pack's table of what Pyreal draws with End of Retail's art beside an older world.
const EOR_TABLE: &str = "tables/drawn-as-eor.tsv";

/// Pyreal's own lists of those, from the workspace's root: the room pictures and the props.
const LATER_ROOMS: &str = "pyreal/client/crates/engine/src/later_rooms.rs";
const LATER_MODELS: &str = "pyreal/client/crates/engine/src/later_models.rs";

/// What [`PIN_FILE`] says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    /// The pack repository, relative to the workspace's root or absolute.
    pub repository: String,
    /// The commit packed.
    pub commit: String,
    /// The pack's version at that commit, as the repository's `release.py version` counts it.
    pub version: String,
    /// The SHA-256 of the packed file.
    pub sha256: String,
    /// Its length.
    pub bytes: u64,
}

impl Pin {
    /// The pin a file's text holds.
    ///
    /// # Errors
    /// The text is not TOML or lacks a field.
    pub fn parse(text: &str) -> Result<Self, String> {
        let t: toml::Table = text.parse().map_err(|e| format!("{PIN_FILE}: {e}"))?;
        let s = |k: &str| {
            t.get(k)
                .and_then(toml::Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| format!("{PIN_FILE}: no {k}"))
        };
        let bytes = t
            .get("bytes")
            .and_then(toml::Value::as_integer)
            .and_then(|b| u64::try_from(b).ok())
            .ok_or_else(|| format!("{PIN_FILE}: no bytes"))?;
        Ok(Self {
            repository: s("repository")?,
            commit: s("commit")?,
            version: s("version")?,
            sha256: s("sha256")?,
            bytes,
        })
    }

    /// The file's text.
    #[must_use]
    pub fn to_text(&self) -> String {
        format!(
            "# The base art pack the app carries: the pack repository's source at `commit`, packed\n\
             # by `cargo xtask base-pack build` into <target>/base-pack/base-pack.dat, which must\n\
             # hash to `sha256`. `cargo xtask base-pack update [<rev>]` moves the pin.\n\
             repository = \"{}\"\n\
             commit = \"{}\"\n\
             version = \"{}\"\n\
             sha256 = \"{}\"\n\
             bytes = {}\n",
            self.repository, self.commit, self.version, self.sha256, self.bytes
        )
    }
}

/// `cargo xtask base-pack ...`.
pub fn base_pack(args: &[String]) -> i32 {
    let result = match args.first().map(String::as_str) {
        Some("build") if args.len() == 1 => build(),
        Some("update") if args.len() <= 2 => update(args.get(1).map_or("HEAD", String::as_str)),
        _ => Err(USAGE.to_owned()),
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("base-pack: {e}");
            1
        }
    }
}

/// Where the packed file goes.
#[must_use]
pub fn out_file() -> PathBuf {
    target_dir().join(DIR).join(FILE)
}

fn read_pin() -> Result<Pin, String> {
    let path = workspace_root().join(PIN_FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Pin::parse(&text)
}

/// The pin's repository: a relative path from the workspace's root, or from the main checkout's
/// when this is a worktree elsewhere.
fn repository(pin: &str) -> PathBuf {
    let p = Path::new(pin);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    let here = workspace_root().join(p);
    if here.exists() {
        return here;
    }
    git_text(
        &workspace_root(),
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .ok()
    .and_then(|common| Path::new(&common).parent().map(|main| main.join(p)))
    .filter(|main| main.exists())
    .unwrap_or(here)
}

fn build() -> Result<(), String> {
    let pin = read_pin()?;
    let out = out_file();
    if out.is_file() && hash_file(&out)? == pin.sha256 {
        println!(
            "{} is the pinned pack {} ({})",
            out.display(),
            pin.version,
            short(&pin.commit)
        );
        return Ok(());
    }
    let repo = repository(&pin.repository);
    let made = pack_commit(&repo, &pin.commit)?;
    let sha = hash_file(&made)?;
    if sha != pin.sha256 {
        return Err(format!(
            "{} at {} packs into {sha}, not the pinned {}: the source or the packer changed; \
             cargo xtask base-pack update {} pins what it packs into now",
            repo.display(),
            short(&pin.commit),
            pin.sha256,
            pin.commit
        ));
    }
    install(&made, &out)?;
    println!(
        "{}: the pinned pack {} ({})",
        out.display(),
        pin.version,
        short(&pin.commit)
    );
    Ok(())
}

fn update(rev: &str) -> Result<(), String> {
    let old = read_pin().ok();
    let repository = old.as_ref().map_or_else(
        || "../dereth-enhanced-environment".to_owned(),
        |p| p.repository.clone(),
    );
    let repo = repository_dir(&repository)?;
    let commit = git_text(
        &repo,
        &["rev-parse", "--verify", &format!("{rev}^{{commit}}")],
    )?;
    let made = pack_commit(&repo, &commit)?;
    let pin = Pin {
        repository,
        version: version(&repo, &commit)?,
        sha256: hash_file(&made)?,
        bytes: std::fs::metadata(&made).map_err(|e| e.to_string())?.len(),
        commit,
    };
    let out = out_file();
    install(&made, &out)?;
    let path = workspace_root().join(PIN_FILE);
    std::fs::write(&path, pin.to_text()).map_err(|e| format!("{}: {e}", path.display()))?;
    match old {
        Some(o) if o == pin => println!("{PIN_FILE} already pins {}", short(&pin.commit)),
        Some(o) => println!(
            "{PIN_FILE}: {} ({}) -> {} ({}), {} bytes",
            o.version,
            short(&o.commit),
            pin.version,
            short(&pin.commit),
            pin.bytes
        ),
        None => println!("{PIN_FILE}: {} ({})", pin.version, short(&pin.commit)),
    }
    Ok(())
}

fn repository_dir(pin: &str) -> Result<PathBuf, String> {
    let repo = repository(pin);
    if repo.join(".git").exists() {
        Ok(repo)
    } else {
        Err(format!("{} is not a git checkout", repo.display()))
    }
}

/// The repository's files at `commit` exported, checked and packed; the packed file's path.
fn pack_commit(repo: &Path, commit: &str) -> Result<PathBuf, String> {
    let dir = target_dir().join(DIR);
    let src = dir.join("src");
    if src.exists() {
        std::fs::remove_dir_all(&src).map_err(|e| format!("{}: {e}", src.display()))?;
    }
    println!("exporting {} at {}", repo.display(), short(commit));
    export(repo, commit, &src)?;
    if src.join("build.py").is_file() {
        let ok = Command::new("python3")
            .args(["-I", "build.py", "--provenance-only"])
            .current_dir(&src)
            .status()
            .is_ok_and(|s| s.success());
        if !ok {
            return Err(format!(
                "the pack at {} fails its provenance check",
                short(commit)
            ));
        }
    }
    let table = src.join(EOR_TABLE);
    if table.is_file() {
        let read =
            |p: &Path| std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()));
        drawn_as_eor_agrees(
            &read(&table)?,
            &read(&workspace_root().join(LATER_ROOMS))?,
            &read(&workspace_root().join(LATER_MODELS))?,
        )
        .map_err(|e| format!("the pack at {}: {e}", short(commit)))?;
    }
    let made = dir.join(format!("{FILE}.new"));
    let ok = Command::new("cargo")
        .args(["run", "--release", "-q", "-p", "pyreal-art", "--", "pack"])
        .arg(src.join("pack"))
        .args(["--layout", "2", "--out"])
        .arg(&made)
        .current_dir(workspace_root())
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        return Err(format!(
            "pyreal-art could not pack {}",
            src.join("pack").display()
        ));
    }
    let _ = std::fs::remove_dir_all(&src);
    Ok(made)
}

/// Every file of `commit` written under `to`, an LFS pointer replaced by its object from the
/// repository's LFS store.
fn export(repo: &Path, commit: &str, to: &Path) -> Result<(), String> {
    let listing = git_bytes(repo, &["ls-tree", "-r", "-z", "--full-tree", commit])?;
    let mut files: Vec<(String, String)> = Vec::new();
    for entry in listing.split(|&b| b == 0).filter(|e| !e.is_empty()) {
        let entry = String::from_utf8_lossy(entry);
        let Some((meta, path)) = entry.split_once('\t') else {
            continue;
        };
        let mut parts = meta.split(' ');
        let (mode, kind, oid) = (parts.next(), parts.next(), parts.next());
        if kind == Some("blob") && mode != Some("120000") {
            files.push((oid.unwrap_or_default().to_owned(), path.to_owned()));
        }
    }
    let lfs = PathBuf::from(git_text(
        repo,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?)
    .join("lfs")
    .join("objects");
    let mut child = Command::new("git")
        .args(["cat-file", "--batch"])
        .current_dir(repo)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("git cat-file: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("git cat-file: no input")?;
    let oids: Vec<String> = files.iter().map(|(oid, _)| format!("{oid}\n")).collect();
    let writer = std::thread::spawn(move || {
        for oid in oids {
            if stdin.write_all(oid.as_bytes()).is_err() {
                break;
            }
        }
    });
    let mut stdout = BufReader::new(child.stdout.take().ok_or("git cat-file: no output")?);
    for (_, path) in &files {
        let mut header = String::new();
        stdout.read_line(&mut header).map_err(|e| e.to_string())?;
        let size: usize = header
            .split(' ')
            .nth(2)
            .and_then(|s| s.trim().parse().ok())
            .ok_or_else(|| format!("git cat-file: {header:?}"))?;
        let mut bytes = vec![0; size + 1];
        stdout.read_exact(&mut bytes).map_err(|e| e.to_string())?;
        bytes.pop();
        let bytes = match lfs_object(&bytes) {
            Some((oid, len)) => {
                let at = lfs.join(&oid[..2]).join(&oid[2..4]).join(&oid);
                let object = std::fs::read(&at).map_err(|_| {
                    format!(
                        "{path}: its LFS object is not in {}; run git lfs fetch there",
                        repo.display()
                    )
                })?;
                if object.len() as u64 != len || sha256_hex(&object) != oid {
                    return Err(format!(
                        "{path}: its LFS object {} is damaged",
                        at.display()
                    ));
                }
                object
            }
            None => bytes,
        };
        let dest = to.join(path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&dest, bytes).map_err(|e| format!("{}: {e}", dest.display()))?;
    }
    let _ = writer.join();
    let _ = child.wait();
    Ok(())
}

/// An LFS pointer's object id and length.
fn lfs_object(bytes: &[u8]) -> Option<(String, u64)> {
    if bytes.len() > 1024 {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?.strip_prefix(LFS_POINTER)?;
    let mut oid = None;
    let mut size = None;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("oid sha256:") {
            oid = Some(v.to_owned());
        } else if let Some(v) = line.strip_prefix("size ") {
            size = v.parse().ok();
        }
    }
    oid.filter(|o| o.len() == 64).zip(size)
}

/// The pack's version at `commit`: `VERSION`'s number, then the commits since it last changed.
fn version(repo: &Path, commit: &str) -> Result<String, String> {
    let base = git_text(repo, &["show", &format!("{commit}:VERSION")])?;
    let since = git_text(repo, &["log", "-1", "--format=%H", commit, "--", "VERSION"])?;
    let range = if since.is_empty() {
        commit.to_owned()
    } else {
        format!("{since}..{commit}")
    };
    let count = git_text(repo, &["rev-list", "--count", &range])?;
    Ok(format!("{base}.{count}"))
}

fn install(made: &Path, out: &Path) -> Result<(), String> {
    std::fs::rename(made, out).map_err(|e| format!("{}: {e}", out.display()))
}

fn git_bytes(repo: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(format!(
            "git {} in {}: {}",
            args.join(" "),
            repo.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

fn git_text(repo: &Path, args: &[&str]) -> Result<String, String> {
    git_bytes(repo, args).map(|b| String::from_utf8_lossy(&b).trim().to_owned())
}

fn sha256_hex(bytes: &[u8]) -> String {
    crate::package::sha256_hex(bytes)
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0; 1 << 20];
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

fn short(commit: &str) -> &str {
    commit.get(..8).unwrap_or(commit)
}

/// The tuples of the `const` named `name` in Rust source: each `(a, b, ...)` line, its numbers read
/// as hex (`0x0100_05F5`) and its `true` and `false` as 1 and 0.
fn listed(src: &str, name: &str) -> Vec<Vec<u32>> {
    let start = src.find(&format!("pub const {name}")).unwrap_or(src.len());
    src[start..]
        .lines()
        .skip(1)
        .take_while(|l| !l.trim_start().starts_with("];"))
        .filter_map(|l| {
            let l = l.trim().strip_prefix('(')?.strip_suffix("),")?;
            l.split(", ").map(number).collect()
        })
        .collect()
}

fn number(s: &str) -> Option<u32> {
    match s.trim() {
        "true" | "yes" => Some(1),
        "false" | "no" => Some(0),
        n => u32::from_str_radix(&n.strip_prefix("0x")?.replace('_', ""), 16).ok(),
    }
}

/// Whether the pack's `tables/drawn-as-eor.tsv` lists exactly the room pictures (`PAIRS`) and the
/// props (`MODELS`) Pyreal draws with End of Retail's art beside an older world: the pack leaves out
/// the older materials that table names, so the two repositories must agree on it.
///
/// # Errors
/// What one lists and the other does not, a few lines of it.
pub fn drawn_as_eor_agrees(table: &str, rooms: &str, models: &str) -> Result<(), String> {
    use std::collections::BTreeSet;
    let mut pack = (BTreeSet::new(), BTreeSet::new());
    for line in table
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
    {
        let f: Vec<&str> = line.split('\t').collect();
        let row: Option<Vec<u32>> = f
            .get(1..4)
            .and_then(|v| v.iter().map(|x| number(x)).collect::<Option<_>>());
        match (f.first(), row) {
            (Some(&"room"), Some(r)) => pack.0.insert(r),
            (Some(&"model"), Some(r)) => pack.1.insert(vec![r[0], r[2]]),
            _ => return Err(format!("{EOR_TABLE}: cannot read {line:?}")),
        };
    }
    let ours: (BTreeSet<_>, BTreeSet<_>) = (
        listed(rooms, "PAIRS").into_iter().collect(),
        listed(models, "MODELS").into_iter().collect(),
    );
    let mut why = Vec::new();
    for (what, p, o) in [("room", &pack.0, &ours.0), ("model", &pack.1, &ours.1)] {
        let hex = |r: &Vec<u32>| {
            r.iter()
                .map(|n| format!("{n:#010X}"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        why.extend(
            o.difference(p)
                .map(|r| format!("{what} {} is Pyreal's, not in {EOR_TABLE}", hex(r))),
        );
        why.extend(
            p.difference(o)
                .map(|r| format!("{what} {} is in {EOR_TABLE}, not Pyreal's", hex(r))),
        );
    }
    if why.is_empty() {
        return Ok(());
    }
    let more = why.len().saturating_sub(8);
    why.truncate(8);
    if more > 0 {
        why.push(format!("and {more} more"));
    }
    Err(format!(
        "{EOR_TABLE} and Pyreal's lists differ:\n  {}",
        why.join("\n  ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pin_reads_back_what_it_writes() {
        let pin = Pin {
            repository: "../dereth-enhanced-environment".into(),
            commit: "c6c9e378".repeat(5),
            version: "0.1.12".into(),
            sha256: "ab".repeat(32),
            bytes: 263_000_000,
        };
        assert_eq!(Pin::parse(&pin.to_text()), Ok(pin));
        assert!(Pin::parse("commit = \"x\"").is_err());
    }

    #[test]
    fn an_lfs_pointer_names_its_object() {
        let oid = "0f".repeat(32);
        let pointer = format!("{LFS_POINTER}oid sha256:{oid}\nsize 42\n");
        assert_eq!(lfs_object(pointer.as_bytes()), Some((oid, 42)));
        assert_eq!(lfs_object(b"not a pointer"), None);
    }

    const ROOMS: &str = "pub const PAIRS: &[(u32, u32, bool)] = &[\n    (0x05000ABB, 0x0500271E, false),\n    (0x05000CDB, 0x05002626, true),\n];\n";
    const MODELS: &str = "pub const MODELS: &[(u32, bool)] = &[\n    (0x0100_05F5, false),\n    (0x0200_02FA, true),\n];\n";

    #[test]
    fn the_table_agrees_with_pyreals_lists() {
        let table = "# kind\tid\teor\ttinted\tleft out\n\
                     room\t0x05000ABB\t0x0500271E\tno\t2005:0x05000ABB\n\
                     room\t0x05000CDB\t0x05002626\tyes\t-\n\
                     model\t0x010005F5\t0x010005F5\tno\t-\n\
                     model\t0x020002FA\t0x020002FA\tyes\t2005:0x05000E11\n";
        assert_eq!(drawn_as_eor_agrees(table, ROOMS, MODELS), Ok(()));
    }

    #[test]
    fn a_table_that_differs_says_how() {
        let table = "room\t0x05000ABB\t0x0500271E\tyes\t-\nmodel\t0x010005F5\t0x010005F5\tno\t-\n";
        let e = drawn_as_eor_agrees(table, ROOMS, MODELS).unwrap_err();
        assert!(
            e.contains("room 0x05000CDB 0x05002626 0x00000001 is Pyreal's"),
            "{e}"
        );
        assert!(
            e.contains("room 0x05000ABB 0x0500271E 0x00000001 is in"),
            "{e}"
        );
        assert!(e.contains("model 0x020002FA 0x00000001 is Pyreal's"), "{e}");
        assert!(drawn_as_eor_agrees("odd\trow\n", ROOMS, MODELS).is_err());
    }
}
