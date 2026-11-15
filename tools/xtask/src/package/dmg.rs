//! The macOS release file: `Pyreal.app` on a disk image beside a link to Applications, signed
//! with the Developer ID in `APPLE_SIGNING_IDENTITY`, then notarized and stapled when an App Store
//! Connect API key is named ([`Notary::from_env`]).

use std::path::Path;
use std::process::Command;

use super::guard::{self, Entry};

/// The link beside the app that a player drags it onto.
pub const APPLICATIONS: &str = "Applications";

/// The disk image's volume name, shown in the Finder's sidebar.
pub const VOLUME: &str = "Pyreal";

/// The `.p8` key file notarytool signs in with.
pub const KEY_PATH_ENV: &str = "APPLE_API_KEY_PATH";
/// The key's id.
pub const KEY_ID_ENV: &str = "APPLE_API_KEY";
/// The key's issuer id.
pub const ISSUER_ENV: &str = "APPLE_API_ISSUER";

/// The App Store Connect API key that notarizes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notary {
    key_path: String,
    key_id: String,
    issuer: String,
}

impl Notary {
    /// The key named by the environment: all three variables or none.
    pub fn from_env() -> Result<Option<Self>, String> {
        let get = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        Self::from_values(get(KEY_PATH_ENV), get(KEY_ID_ENV), get(ISSUER_ENV))
    }

    /// The key from its three parts: all of them, or none for no notarizing.
    pub fn from_values(
        key_path: Option<String>,
        key_id: Option<String>,
        issuer: Option<String>,
    ) -> Result<Option<Self>, String> {
        match (key_path, key_id, issuer) {
            (None, None, None) => Ok(None),
            (Some(key_path), Some(key_id), Some(issuer)) => Ok(Some(Self {
                key_path,
                key_id,
                issuer,
            })),
            _ => Err(format!(
                "notarizing needs {KEY_PATH_ENV}, {KEY_ID_ENV} and {ISSUER_ENV} together"
            )),
        }
    }

    fn key_args(&self) -> Vec<String> {
        vec![
            "--key".to_owned(),
            self.key_path.clone(),
            "--key-id".to_owned(),
            self.key_id.clone(),
            "--issuer".to_owned(),
            self.issuer.clone(),
        ]
    }

    /// notarytool's arguments to submit `file` and wait for Apple's answer.
    pub fn submit_args(&self, file: &str) -> Vec<String> {
        let mut args = vec![
            "notarytool".to_owned(),
            "submit".to_owned(),
            file.to_owned(),
        ];
        args.extend(self.key_args());
        args.extend(["--wait", "--timeout", "2h", "--output-format", "json"].map(str::to_owned));
        args
    }

    /// notarytool's arguments to fetch the log of submission `id`.
    pub fn log_args(&self, id: &str) -> Vec<String> {
        let mut args = vec!["notarytool".to_owned(), "log".to_owned(), id.to_owned()];
        args.extend(self.key_args());
        args
    }
}

/// The signing identity from `APPLE_SIGNING_IDENTITY`; none (or `-`) signs ad hoc.
pub fn signing_identity() -> Option<String> {
    std::env::var("APPLE_SIGNING_IDENTITY")
        .ok()
        .map(|i| i.trim().to_owned())
        .filter(|i| !i.is_empty() && i != "-")
}

/// The submission's id and status from notarytool's JSON answer.
pub fn submission(json: &str) -> Result<(String, String), String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("notarytool's answer: {e}: {json}"))?;
    let field = |k: &str| v.get(k).and_then(|s| s.as_str()).map(str::to_owned);
    match (field("id"), field("status")) {
        (Some(id), Some(status)) => Ok((id, status)),
        _ => Err(format!(
            "notarytool's answer names no id and status: {json}"
        )),
    }
}

/// Whether the `.manifest` part `text` records `name` at `size` bytes with this SHA-256.
pub fn recorded(text: &str, name: &str, size: usize, sha256: &str) -> bool {
    text.lines().any(|l| {
        let f: Vec<&str> = l.split('\t').collect();
        f.len() >= 4 && f[1] == name && f[2] == size.to_string() && f[3] == sha256
    })
}

/// Make `out` from `folder` (holding `Pyreal.app`, signed): the app beside a link to
/// Applications, the image signed with `identity`, and notarized and stapled by `notary`.
pub fn make(
    folder: &Path,
    out: &Path,
    identity: Option<&str>,
    notary: Option<&Notary>,
) -> Result<(), String> {
    let link = folder.join(APPLICATIONS);
    if link.symlink_metadata().is_ok() {
        std::fs::remove_file(&link).map_err(|e| format!("{}: {e}", link.display()))?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink("/Applications", &link)
        .map_err(|e| format!("{}: {e}", link.display()))?;
    if out.exists() {
        std::fs::remove_file(out).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    let folder_s = folder.display().to_string();
    let out_s = out.display().to_string();
    let create = [
        "create",
        "-volname",
        VOLUME,
        "-srcfolder",
        &folder_s,
        "-fs",
        "HFS+",
        "-format",
        "UDZO",
        "-ov",
        &out_s,
    ];
    // hdiutil sometimes finds the new image busy on a shared machine; a second try clears it.
    let mut made = Err(String::new());
    for _ in 0..3 {
        made = run("hdiutil", &create);
        if made.is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
    made?;
    if let Some(id) = identity {
        let mut args = vec!["--force", "--sign", id];
        if notary.is_some() {
            args.push("--timestamp");
        }
        args.push(&out_s);
        run("codesign", &args)?;
    }
    if let Some(n) = notary {
        notarize(n, &out_s)?;
        run("xcrun", &["stapler", "staple", &out_s])?;
        run("xcrun", &["stapler", "validate", &out_s])?;
        run(
            "spctl",
            &[
                "--assess",
                "--type",
                "open",
                "--context",
                "context:primary-signature",
                "-v",
                &out_s,
            ],
        )?;
    }
    Ok(())
}

/// Submit `file` and wait; anything but Accepted prints Apple's log and fails.
fn notarize(n: &Notary, file: &str) -> Result<(), String> {
    let args = n.submit_args(file);
    println!("$ xcrun notarytool submit {file} (waiting for Apple)");
    let out = Command::new("xcrun")
        .args(&args)
        .output()
        .map_err(|e| format!("xcrun could not be run: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let (id, status) = submission(text.trim())
        .map_err(|e| format!("{e}\n{}", String::from_utf8_lossy(&out.stderr)))?;
    println!("  notarization {id}: {status}");
    if status == "Accepted" {
        return Ok(());
    }
    if let Ok(log) = Command::new("xcrun").args(n.log_args(&id)).output() {
        println!("{}", String::from_utf8_lossy(&log.stdout));
    }
    Err(format!(
        "Apple did not accept {file} for notarization ({status}, submission {id})"
    ))
}

/// The deny scan of a written disk image, mounted read-only under `scratch`: its top holds
/// `Pyreal.app` and the Applications link alone, and the app passes [`guard::scan_tree`].
pub fn check(
    scratch: &Path,
    image: &Path,
    app: &str,
    required: &[String],
    optional: &[String],
    file_cap: u64,
    total_cap: u64,
) -> Result<(), String> {
    let mount = scratch.join("dmg-mount");
    std::fs::create_dir_all(&mount).map_err(|e| format!("{}: {e}", mount.display()))?;
    let mount_s = mount.display().to_string();
    run(
        "hdiutil",
        &[
            "attach",
            "-readonly",
            "-nobrowse",
            "-noautoopen",
            "-mountpoint",
            &mount_s,
            &image.display().to_string(),
        ],
    )?;
    let scanned = scan_mounted(&mount, app, required, optional, file_cap, total_cap);
    let detached = run("hdiutil", &["detach", &mount_s, "-force"]);
    let findings = scanned?;
    detached?;
    if findings.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the deny scan refused {}:\n  {}",
            image.display(),
            findings.join("\n  ")
        ))
    }
}

fn scan_mounted(
    mount: &Path,
    app: &str,
    required: &[String],
    optional: &[String],
    file_cap: u64,
    total_cap: u64,
) -> Result<Vec<String>, String> {
    let mut top = Vec::new();
    for e in std::fs::read_dir(mount).map_err(|e| format!("{}: {e}", mount.display()))? {
        let e = e.map_err(|e| e.to_string())?;
        let name = e.file_name().to_string_lossy().into_owned();
        // The file system's own records, which every HFS+ image carries.
        if matches!(
            name.as_str(),
            ".fseventsd" | ".Trashes" | ".HFS+ Private Directory Data\r"
        ) {
            continue;
        }
        top.push((name, e.path()));
    }
    let mut findings = top_findings(&top, app);
    let entries: Vec<Entry> = guard::staged_entries(&mount.join(app))
        .map_err(|e| format!("{}: {e}", mount.join(app).display()))?;
    findings.extend(guard::scan_tree(
        &entries, required, optional, file_cap, total_cap,
    ));
    Ok(findings)
}

/// What is wrong with the image's top level: anything but the app folder and the link to
/// `/Applications`.
fn top_findings(top: &[(String, std::path::PathBuf)], app: &str) -> Vec<String> {
    let mut findings = Vec::new();
    for (name, path) in top {
        let ok = if name == app {
            path.symlink_metadata().is_ok_and(|m| m.is_dir())
        } else if name == APPLICATIONS {
            std::fs::read_link(path).is_ok_and(|t| t == Path::new("/Applications"))
        } else {
            false
        };
        if !ok {
            findings.push(format!("{name}: not part of the disk image"));
        }
    }
    for want in [app, APPLICATIONS] {
        if !top.iter().any(|(n, _)| n == want) {
            findings.push(format!("{want}: missing from the disk image"));
        }
    }
    findings
}

fn run(program: &str, args: &[&str]) -> Result<(), String> {
    println!("$ {program} {}", args.join(" "));
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|e| format!("{program} could not be run: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`{program} {}` failed ({status})", args.join(" ")))
    }
}
