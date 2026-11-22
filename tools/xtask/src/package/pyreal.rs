//! `cargo xtask package pyreal`: Pyreal's release files (the client and the plugin author's
//! tool, with the base art pack when one is named), built the same way on a contributor's machine
//! and in a release workflow.
//!
//! ```text
//! cargo xtask package pyreal [--base-pack <file>] [--target <triple>]... [--out <dir>] [--allow-dirty]
//! cargo xtask package pyreal --gather <dir>
//! ```
//!
//! For each target (the host's own by default; each builds on its own operating system) it builds
//! the client and the tool with cargo and writes into `<target dir>/package/pyreal-<version>/` (or
//! `--out`):
//!
//! | system | the release file |
//! |---|---|
//! | Windows | `pyreal-<v>-windows-x86_64.zip`: `Pyreal.exe`, `pyreal-plugin.exe` and the notices, under `pyreal-<v>/` |
//! | macOS | `Pyreal-<v>-macos-<arch>.dmg`: `Pyreal.app` beside a link to Applications in a laid-out window, the client and the tool in `Contents/MacOS`, the notices in `Contents/Resources` |
//! | Linux | `pyreal-<v>-linux-x86_64.tar.gz`: `pyreal`, `pyreal-plugin` and the notices, under `pyreal-<v>/` |
//!
//! The plugin author's tool is also a file of its own for the same platform
//! ([`tool_file_name`]). Each target's lines of `MANIFEST.txt` go in `<file>.manifest`; then the
//! index is written over the folder: `MANIFEST.txt`, `release.json` and `SHA256SUMS`.
//! `--gather <dir>` builds nothing: it checks the files already in `<dir>` and writes the index.
//!
//! **The dat guard.** Only named files are copied, and the deny scan runs over what was staged and
//! again over each written archive: a file that is not on the package's list, a game data file or
//! anything named like one, a link, or a file over the cap fails the package. A file that names
//! one of this machine's folders (the checkout, the target folder, the home folder or cargo's)
//! fails it too.
//!
//! **The base art pack.** `--base-pack <file>` ships it as [`BASE_PACK`] (beside the client, in
//! `Contents/Resources` on macOS), which the client always draws, under the pack the player
//! chooses. Without it the client draws without one. It is the one data file the deny scan lets
//! in, and only when its header is a packed art pack's. It has no size cap: it is as large as the
//! art needs.
//!
//! **macOS:** the bundle is laid out as `bundle-macos` lays it out ([`crate::bundle::lay_out`])
//! and signed with the identity in `APPLE_SIGNING_IDENTITY`, under the hardened runtime, or ad
//! hoc without one ([`codesign_runs`]). It goes on a disk image, which is notarized and stapled
//! when an App Store Connect API key is named ([`dmg`]).

use std::path::{Path, PathBuf};
use std::process::Command;

use super::archive::{self, Member};
use super::dmg;
use super::guard::{self, Entry, Kind};
use super::notice;
use super::targets::{self, Os, Target};
use super::version::{self, Product};
use super::{build_facts, headers, read, sha256_hex, write, BuildFacts};
use crate::util::{target_dir, workspace_root};

/// The licences of everything `manifest`'s package is built from for `target`, as cargo-about's
/// page, written as `<name>.html` in `dir`.
pub(super) fn licence_page(
    ws: &Path,
    manifest: &str,
    target: Target,
    dir: &Path,
    name: &str,
) -> Result<String, String> {
    about_page(
        ws,
        (ABOUT_CONFIG, ABOUT_TEMPLATE),
        manifest,
        target,
        dir,
        name,
    )
}

/// [`licence_page`], with cargo-about's configuration and template named as `about`.
pub(super) fn about_page(
    ws: &Path,
    about: (&str, &str),
    manifest: &str,
    target: Target,
    dir: &Path,
    name: &str,
) -> Result<String, String> {
    let (config, template) = about;
    let out = dir.join(format!("{name}.html"));
    let out_arg = out.display().to_string();
    run_in(
        ws,
        "cargo",
        &[
            "about",
            "generate",
            "--locked",
            "--fail",
            "-m",
            manifest,
            "-c",
            config,
            "--target",
            target.triple,
            "-o",
            &out_arg,
            template,
        ],
        &[],
        &[],
    )?;
    Ok(String::from_utf8_lossy(&read(&out)?).into_owned())
}

/// cargo-about's configuration and template.
const ABOUT_CONFIG: &str = "pyreal/about.toml";
const ABOUT_TEMPLATE: &str = "pyreal/about.hbs";

/// The typefaces Horizon's glyph pages are drawn from, and their licence files (`fonts/README.md`).
pub const HORIZON_FONTS: [(&str, &str); 4] = [
    ("Noto Sans", "NotoSans-OFL.txt"),
    ("Saira", "Saira-OFL.txt"),
    ("Pathway Gothic One", "PathwayGothicOne-OFL.txt"),
    ("Cormorant SC", "CormorantSC-OFL.txt"),
];
pub const HORIZON_FONT_LICENCES: &str = "pyreal/client/crates/horizon/fonts/licences";

/// The notices every package carries beside its programs.
pub const NOTICES: &[&str] = &["LICENSE", "NOTICE.txt", "THIRD-PARTY-LICENSES.html"];

/// No file of a package but the base pack is larger than this. The programs are tens of megabytes,
/// so a larger file is something that should not be there.
pub const FILE_CAP: u64 = 320 << 20;

/// No package holds more than this in all, the base pack aside, for the same reason.
pub const TOTAL_CAP: u64 = 512 << 20;

/// The base art pack's name in a package, whatever `--base-pack` names.
pub const BASE_PACK: &str = guard::BASE_PACK;

/// The public repository the release files are downloaded from, when one is named.
pub const SOURCE_URL_ENV: &str = "PYREAL_BUILD_SOURCE_URL";

/// The macOS bundle's name.
pub const APP: &str = "Pyreal.app";

/// The plugin author's tool, released beside the client for each platform.
pub const TOOL: &str = "pyreal-plugin";

/// The tool's crate, by its manifest.
const TOOL_MANIFEST: &str = "tools/plugin/Cargo.toml";

/// The client's crate.
const CLIENT: &str = "pyreal-client";

/// The binary cargo builds from the client's crate.
const CLIENT_BINARY: &str = "pyreal";

/// The client's name in a package for `target`.
pub fn client_in_release(target: Target) -> String {
    match target.os {
        Os::Windows => "Pyreal.exe".to_owned(),
        Os::Mac => format!("Contents/MacOS/{}", crate::bundle::EXECUTABLE),
        Os::Linux => "pyreal".to_owned(),
    }
}

/// The plugin author's tool's release file for `target`: a zip on Windows, a tarball elsewhere.
pub fn tool_file_name(target: Target, version: &str) -> String {
    let ext = if target.os == Os::Windows {
        "zip"
    } else {
        "tar.gz"
    };
    format!("{TOOL}-{version}-{}.{ext}", target.platform_name())
}

/// The one top-level folder of the tool's archive.
pub fn tool_root(version: &str) -> String {
    format!("{TOOL}-{version}")
}

/// Where the tool is in the release file for `target`, beside the client.
pub fn tool_in_release(target: Target) -> String {
    match target.os {
        Os::Windows => format!("{TOOL}{}", target.exe_suffix()),
        Os::Mac => format!("Contents/MacOS/{TOOL}"),
        Os::Linux => TOOL.to_owned(),
    }
}

/// Where the base pack is in the release file for `target`: beside the client, or in the bundle's
/// `Contents/Resources` on macOS (where the client looks, `<exe>/../Resources`).
pub fn base_pack_in_release(target: Target) -> String {
    match target.os {
        Os::Mac => format!("Contents/Resources/{BASE_PACK}"),
        Os::Windows | Os::Linux => BASE_PACK.to_owned(),
    }
}

/// Where the notices are in the release file for `target`.
fn notices_in(target: Target) -> &'static str {
    if target.os == Os::Mac {
        "Contents/Resources/"
    } else {
        ""
    }
}

/// The base pack at `path`, checked: a file, not empty, and a packed art pack by its header.
pub fn check_base_pack(path: &Path) -> Result<(), String> {
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() && m.len() > 0 => {
            use std::io::Read;
            let mut head = Vec::new();
            std::fs::File::open(path)
                .and_then(|f| f.take(guard::HEAD_LEN as u64).read_to_end(&mut head))
                .map_err(|e| format!("the base pack {}: {e}", path.display()))?;
            if guard::is_packed_art_pack(&head) {
                Ok(())
            } else {
                Err(format!(
                    "the base pack {} is not a packed art pack",
                    path.display()
                ))
            }
        }
        Ok(m) if m.is_file() => Err(format!("the base pack {} is empty", path.display())),
        Ok(_) => Err(format!("the base pack {} is not a file", path.display())),
        Err(e) => Err(format!("the base pack {}: {e}", path.display())),
    }
}

/// The files the tool's archive holds below its folder: the program and the notices.
pub fn tool_contents(target: Target) -> Vec<String> {
    let mut out = vec![format!("{TOOL}{}", target.exe_suffix())];
    out.extend(NOTICES.iter().map(|n| (*n).to_owned()));
    out.sort();
    out
}

/// The release file for `target`.
pub fn release_file_name(target: Target, version: &str) -> String {
    let p = target.platform_name();
    match target.os {
        Os::Windows => format!("pyreal-{version}-{p}.zip"),
        Os::Mac => format!("Pyreal-{version}-{p}.dmg"),
        Os::Linux => format!("pyreal-{version}-{p}.tar.gz"),
    }
}

/// The one top-level folder of the release file's archive.
pub fn archive_root(target: Target, version: &str) -> String {
    match target.os {
        Os::Windows | Os::Linux => format!("pyreal-{version}"),
        Os::Mac => APP.to_owned(),
    }
}

/// The files the release file's archive holds below its top-level folder: those it must hold, and
/// those it may (what the icon compiler and the signature write on some machines and not others).
pub fn contents(target: Target) -> (Vec<String>, Vec<String>) {
    let mut required = vec![client_in_release(target), tool_in_release(target)];
    required.extend(NOTICES.iter().map(|n| format!("{}{n}", notices_in(target))));
    let mut optional = vec![base_pack_in_release(target)];
    if target.os == Os::Mac {
        required.extend([
            "Contents/Info.plist".to_owned(),
            "Contents/PkgInfo".to_owned(),
        ]);
        optional.extend(
            [
                "Contents/Resources/Assets.car",
                "Contents/Resources/Pyreal.icns",
                "Contents/_CodeSignature/CodeResources",
            ]
            .map(str::to_owned),
        );
    }
    required.sort();
    optional.sort();
    (required, optional)
}

/// The macOS signing runs for the bundle `app`: each program inside it first, then the bundle
/// over them. With an identity, under the hardened runtime, with Apple's secure timestamp when
/// `timestamp` (notarizing needs it); without one, ad hoc.
pub fn codesign_runs(
    app: &str,
    identity: Option<&str>,
    timestamp: bool,
    nested: &[String],
) -> Vec<Vec<String>> {
    let identity = identity
        .map(str::trim)
        .filter(|i| !i.is_empty() && *i != "-");
    let sign = |path: String| {
        let mut args = vec!["--force".to_owned(), "--sign".to_owned()];
        match identity {
            Some(id) => {
                args.extend([id.to_owned(), "--options".to_owned(), "runtime".to_owned()]);
                if timestamp {
                    args.push("--timestamp".to_owned());
                }
            }
            None => args.push("-".to_owned()),
        }
        args.push(path);
        args
    };
    let mut runs: Vec<Vec<String>> = nested.iter().map(|n| sign(format!("{app}/{n}"))).collect();
    runs.push(sign(app.to_owned()));
    runs
}

/// What a file of a release folder is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asset {
    /// The release file: the client and the tool.
    Client(Target),
    /// The plugin author's tool for a platform.
    Tool(Target),
    /// One file's lines of `MANIFEST.txt`.
    ManifestPart,
    /// A file the index writes, replaced on every run.
    Index,
}

/// The index files.
const INDEX_FILES: &[&str] = &["SHA256SUMS", "release.json", "MANIFEST.txt"];

/// Classify a file of the release folder, or refuse it: the folder holds this version's release
/// files and their manifest parts, and the index.
pub fn classify(name: &str, version: &str) -> Result<Asset, String> {
    if INDEX_FILES.contains(&name) {
        return Ok(Asset::Index);
    }
    for triple in targets::PYREAL_TRIPLES {
        let t = targets::find(triple)?;
        let file = release_file_name(t, version);
        if name == file {
            return Ok(Asset::Client(t));
        }
        let tool = tool_file_name(t, version);
        if name == tool {
            return Ok(Asset::Tool(t));
        }
        if name == format!("{file}.manifest") || name == format!("{tool}.manifest") {
            return Ok(Asset::ManifestPart);
        }
    }
    Err(format!(
        "`{name}` is not a file of the Pyreal {version} release (a release file, the plugin \
         author's tool, either's .manifest or the index)"
    ))
}

/// The options of one `package pyreal` run.
#[derive(Debug, Default)]
pub(super) struct Options {
    pub targets: Vec<String>,
    pub out: Option<PathBuf>,
    pub gather: Option<PathBuf>,
    pub allow_dirty: bool,
    pub base_pack: Option<PathBuf>,
}

pub const USAGE: &str = "usage: cargo xtask package pyreal [--base-pack <file>] [--target <triple>]... [--out <dir>] [--allow-dirty]
       cargo xtask package pyreal --gather <dir>";

pub(super) fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut o = Options::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value\n{USAGE}"))
        };
        match a.as_str() {
            "--target" => o.targets.push(value()?),
            "--out" => o.out = Some(PathBuf::from(value()?)),
            "--gather" => o.gather = Some(PathBuf::from(value()?)),
            "--base-pack" => o.base_pack = Some(PathBuf::from(value()?)),
            "--allow-dirty" => o.allow_dirty = true,
            other => return Err(format!("unexpected argument `{other}`\n{USAGE}")),
        }
    }
    if o.gather.is_some() && (!o.targets.is_empty() || o.out.is_some() || o.base_pack.is_some()) {
        return Err(format!(
            "--gather builds nothing: it takes no --target, --out or --base-pack\n{USAGE}"
        ));
    }
    Ok(o)
}

/// `cargo xtask package pyreal ...` (the arguments after `pyreal`).
pub fn package(args: &[String]) -> i32 {
    let options = match parse_options(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let ws = workspace_root();
    let result = match &options.gather {
        Some(dir) => gather(&ws, dir, options.allow_dirty),
        None => {
            let triples = if options.targets.is_empty() {
                match super::host_triple() {
                    Ok(h) => vec![h],
                    Err(e) => {
                        eprintln!("package: {e}");
                        return 1;
                    }
                }
            } else {
                options.targets.clone()
            };
            build_packages(
                &ws,
                &triples,
                options.out.as_deref(),
                options.allow_dirty,
                options.base_pack.as_deref(),
            )
        }
    };
    match result {
        Ok(dir) => {
            println!(
                "\npackage: done; the release files are in {}",
                dir.display()
            );
            0
        }
        Err(e) => {
            eprintln!("\npackage: FAILED\n{e}");
            1
        }
    }
}

pub(super) fn succeeds(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Build, check and write every target's files, then the index. The folder they are in.
pub fn build_packages(
    ws: &Path,
    triples: &[String],
    out: Option<&Path>,
    allow_dirty: bool,
    base_pack: Option<&Path>,
) -> Result<PathBuf, String> {
    let version = Product::Pyreal.version(ws)?;
    version::parse_release_version(&version)?;
    // Everything a host lacks is said before anything is built.
    let host = super::host();
    let mut plans = Vec::new();
    let mut refusals = Vec::new();
    for triple in triples {
        match targets::find_pyreal(triple).and_then(|t| targets::plan_pyreal(t, &host).map(|()| t))
        {
            Ok(t) => plans.push(t),
            Err(e) => refusals.push(format!("  {triple}: {e}")),
        }
    }
    if !succeeds("cargo", &["about", "--version"]) {
        refusals.push(
            "  cargo-about is not installed, or does not run on this host; it writes THIRD-PARTY-LICENSES.html: \
             `cargo install --locked cargo-about --features cli`"
                .to_owned(),
        );
    }
    if let Some(Err(e)) = base_pack.map(check_base_pack) {
        refusals.push(format!("  {e}"));
    }
    if !refusals.is_empty() {
        return Err(format!(
            "this host cannot package what was asked:\n{}",
            refusals.join("\n")
        ));
    }
    let facts = build_facts(ws, allow_dirty)?;
    let base = target_dir().join("package");
    let out_dir = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| base.join(format!("pyreal-{version}")));
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    for target in plans {
        println!("\n=== pyreal {version} for {}", target.triple);
        let stage = base.join("stage").join(format!("pyreal-{}", target.triple));
        fresh_dir(&stage)?;
        let job = Job {
            ws,
            target,
            facts: &facts,
            version: &version,
            out_dir: &out_dir,
            stage: &stage,
            base_pack,
        };
        job.run()?;
    }
    write_index(&out_dir, &version, &facts)?;
    Ok(out_dir)
}

/// `--gather <dir>`: check the files there and write the index over them.
fn gather(ws: &Path, dir: &Path, allow_dirty: bool) -> Result<PathBuf, String> {
    let version = Product::Pyreal.version(ws)?;
    let facts = build_facts(ws, allow_dirty)?;
    write_index(dir, &version, &facts)?;
    Ok(dir.to_path_buf())
}

/// The plugin engine's compiler flags in a release build: its assertions off.
pub const LUAU_RELEASE_FLAGS: &str = "-DNDEBUG";

/// The environment a release build runs with: the build stamp and the flags that keep local paths
/// out of the binaries. On Windows the C runtime is linked in, so no program needs a Visual C++
/// redistributable, and the linker writes no time.
///
/// The plugin engine's C++ is built as any optimised build of it is, without its own assertions
/// ([`LUAU_RELEASE_FLAGS`]): each names its source file by the full path it was compiled from,
/// which holds the builder's home folder, and the compiler's path remapping reaches only Rust.
pub fn build_env(ws: &Path, facts: &BuildFacts, target: Target) -> Vec<(String, String)> {
    let mut flags: Vec<String> = super::local_remaps(ws)
        .iter()
        .map(|(from, to)| format!("--remap-path-prefix={}={to}", from.display()))
        .collect();
    if target.os == Os::Windows {
        flags.push("-Ctarget-feature=+crt-static".to_owned());
        flags.push("-Clink-arg=/Brepro".to_owned());
    }
    let mut env = vec![
        ("CARGO_ENCODED_RUSTFLAGS".to_owned(), flags.join("\u{1f}")),
        ("SOURCE_DATE_EPOCH".to_owned(), facts.epoch.to_string()),
        ("PYREAL_BUILD_COMMIT".to_owned(), facts.commit.clone()),
        (super::build_hash::ENV.to_owned(), facts.hash.clone()),
        ("PYREAL_BUILD_TARGET".to_owned(), target.triple.to_owned()),
        (SOURCE_URL_ENV.to_owned(), facts.source_url.clone()),
        ("LUAU_CXXFLAGS".to_owned(), LUAU_RELEASE_FLAGS.to_owned()),
    ];
    if target.os == Os::Mac {
        env.push((
            "MACOSX_DEPLOYMENT_TARGET".to_owned(),
            format!("{}.{}", targets::MACOS_FLOOR.0, targets::MACOS_FLOOR.1),
        ));
    }
    env
}

/// Run `program` in `dir`, with `env` added and `remove` taken out of the environment.
pub(super) fn run_in(
    dir: &Path,
    program: &str,
    args: &[&str],
    env: &[(String, String)],
    remove: &[&str],
) -> Result<(), String> {
    println!("$ {program} {}", args.join(" "));
    let mut cmd = Command::new(program);
    cmd.args(args)
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .current_dir(dir);
    for r in remove {
        cmd.env_remove(r);
    }
    let status = cmd
        .status()
        .map_err(|e| format!("{program} could not be run: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`{program} {}` failed ({status})", args.join(" ")))
    }
}

pub(super) fn mkdir(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))
}

pub(super) fn fresh_dir(dir: &Path) -> Result<(), String> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    mkdir(dir)
}

pub(super) fn copy(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(parent) = to.parent() {
        mkdir(parent)?;
    }
    std::fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| format!("{} -> {}: {e}", from.display(), to.display()))
}

/// Whether a file on disk is executable (always false where there is no such bit).
pub(super) fn executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

/// The crates `cargo tree` says the package in `manifest` is built from, for `target`.
pub(super) fn crates_of(
    ws: &Path,
    manifest: &str,
    package: &str,
    target: Target,
) -> Result<Vec<notice::Crate>, String> {
    let tree = Command::new("cargo")
        .args([
            "tree",
            "--locked",
            "-e",
            "normal",
            "--target",
            target.triple,
        ])
        .args(["--manifest-path", manifest, "-p", package])
        .args(["--prefix", "none", "--format", "{p}|{l}"])
        .current_dir(ws)
        .output()
        .map_err(|e| format!("cargo tree could not be run: {e}"))?;
    if !tree.status.success() {
        return Err(format!(
            "cargo tree failed: {}",
            String::from_utf8_lossy(&tree.stderr)
        ));
    }
    Ok(notice::parse_cargo_tree(&String::from_utf8_lossy(
        &tree.stdout,
    )))
}

/// One target's build.
struct Job<'a> {
    ws: &'a Path,
    target: Target,
    facts: &'a BuildFacts,
    version: &'a str,
    out_dir: &'a Path,
    /// This target's staging folder, made anew.
    stage: &'a Path,
    base_pack: Option<&'a Path>,
}

impl Job<'_> {
    fn run(&self) -> Result<(), String> {
        let client = self.build_program(CLIENT, CLIENT_BINARY)?;
        let tool = self.build_program(TOOL, TOOL)?;
        let notices = self.notices()?;
        let t = self.target;
        let root = archive_root(t, self.version);
        let tree = self.stage.join("tree").join(&root);
        fresh_dir(&tree)?;
        if t.os == Os::Mac {
            crate::bundle::lay_out(
                self.ws,
                &tree,
                &client,
                &self.stage.join("icon-build"),
                self.version,
            )?;
        } else {
            copy(&client, &tree.join(client_in_release(t)))?;
        }
        copy(&tool, &tree.join(tool_in_release(t)))?;
        if let Some(pack) = self.base_pack {
            copy(pack, &tree.join(base_pack_in_release(t)))?;
        }
        for n in NOTICES {
            copy(
                &notices.join(n),
                &tree.join(format!("{}{n}", notices_in(t))),
            )?;
        }
        for program in [client_in_release(t), tool_in_release(t)] {
            set_executable(&tree.join(program))?;
        }
        if t.os == Os::Mac {
            let identity = dmg::signing_identity();
            let notary = dmg::Notary::from_env()?;
            if notary.is_some() && identity.is_none() {
                return Err("notarizing needs a Developer ID in APPLE_SIGNING_IDENTITY".to_owned());
            }
            for args in codesign_runs(
                &tree.display().to_string(),
                identity.as_deref(),
                notary.is_some(),
                &[tool_in_release(t), client_in_release(t)],
            ) {
                let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                run_in(self.stage, "codesign", &refs, &[], &[])?;
            }
        }
        self.finish_tree(&tree, &root)?;
        self.package_tool(&tool)
    }

    /// A program of the workspace, built with cargo for the target and staged under a name of
    /// its own, so a later build in the same target folder cannot change what this package holds.
    fn build_program(&self, package: &str, binary: &str) -> Result<PathBuf, String> {
        let t = self.target;
        let env = build_env(self.ws, self.facts, t);
        run_in(
            self.ws,
            "cargo",
            &[
                "build",
                "--locked",
                "--release",
                "-p",
                package,
                "--target",
                t.triple,
            ],
            &env,
            &[],
        )?;
        let exe = format!("{binary}{}", t.exe_suffix());
        let built = target_dir().join(t.triple).join("release").join(&exe);
        let staged =
            self.stage
                .join("bin")
                .join(format!("{binary}-{}{}", t.triple, t.exe_suffix()));
        copy(&built, &staged)?;
        Ok(staged)
    }

    /// A console program's header: its line for the manifest.
    fn check_tool(&self, bytes: &[u8]) -> Result<String, String> {
        let t = self.target;
        headers::check_console_app(bytes, t, targets::PYREAL_GLIBC_FLOOR).map_err(|problems| {
            format!(
                "the header check refused {TOOL} for {}:\n  {}",
                t.triple,
                problems.join("\n  ")
            )
        })
    }

    /// The client's header, checked as a windowed program's.
    fn check_client(&self, bytes: &[u8]) -> Result<String, String> {
        headers::check_app_binary(bytes, self.target, targets::PYREAL_GLIBC_FLOOR).map_err(
            |problems| {
                format!(
                    "the header check refused the client for {}:\n  {}",
                    self.target.triple,
                    problems.join("\n  ")
                )
            },
        )
    }

    /// The plugin author's tool as a release file of its own beside the client's: its header
    /// checked as a console program's, and archived with its notices ([`tool_archive`]).
    fn package_tool(&self, tool: &Path) -> Result<(), String> {
        let t = self.target;
        let exe = format!("{TOOL}{}", t.exe_suffix());
        let program = read(tool)?;
        let described = self.check_tool(&program)?;
        println!("  {exe}: {described}");
        let about = self.stage.join("tool-about");
        mkdir(&about)?;
        let licenses = licence_page(self.ws, TOOL_MANIFEST, t, &about, TOOL)?;
        let crates = crates_of(self.ws, "Cargo.toml", TOOL, t)?;
        let mit = String::from_utf8_lossy(&read(&self.ws.join("LICENSE"))?).into_owned();
        let notice = notice::tool_notice(&notice::ToolFacts {
            version: self.version,
            target: t.triple,
            commit: &self.facts.commit,
            source_url: &self.facts.source_url,
            crates: &crates,
            mit_license: &mit,
        });
        let members = tool_members(t, program, &mit, &notice, &licenses);
        let named: Vec<(&str, &[u8])> = members
            .iter()
            .map(|m| (m.name.as_str(), m.bytes.as_slice()))
            .collect();
        self.refuse_machine_folders(&self.stage.join(TOOL), &named)?;
        let bytes = tool_archive(t, self.version, &members, self.facts.epoch)?;
        let file = tool_file_name(t, self.version);
        let path = self.out_dir.join(&file);
        write(&path, &bytes)?;
        check_tool_archive(&path, t, self.version)?;
        println!(
            "  {}: {} bytes, sha256 {}",
            path.display(),
            bytes.len(),
            sha256_hex(&bytes)
        );
        let lines: Vec<(String, Vec<u8>, String)> = members
            .into_iter()
            .map(|m| {
                let d = if m.name == exe {
                    described.clone()
                } else {
                    String::new()
                };
                (m.name, m.bytes, d)
            })
            .collect();
        self.write_manifest(&file, None, &lines)
    }

    /// `LICENSE`, `NOTICE.txt` and `THIRD-PARTY-LICENSES.html`: the folder they are written in.
    fn notices(&self) -> Result<PathBuf, String> {
        let t = self.target;
        let mut all_crates = crates_of(self.ws, "Cargo.toml", CLIENT, t)?;
        all_crates.extend(crates_of(self.ws, "Cargo.toml", TOOL, t)?);
        all_crates.sort();
        all_crates.dedup();
        let mit = String::from_utf8_lossy(&read(&self.ws.join("LICENSE"))?).into_owned();
        let about = self.stage.join("about");
        mkdir(&about)?;
        let page = |manifest: &str, name: &str| licence_page(self.ws, manifest, t, &about, name);
        let exe = t.exe_suffix();
        let client_page = page("pyreal/client/Cargo.toml", CLIENT)?;
        let tool_page = page(TOOL_MANIFEST, TOOL)?;
        let dir = self.stage.join("notices");
        mkdir(&dir)?;
        let fonts = HORIZON_FONTS
            .iter()
            .map(|(name, file)| {
                let text = read(&self.ws.join(HORIZON_FONT_LICENCES).join(file))?;
                Ok((
                    *name,
                    "The interface's text is drawn from pictures of its glyphs",
                    String::from_utf8_lossy(&text).into_owned(),
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let text = notice::pyreal_notice(&notice::PyrealFacts {
            version: self.version,
            target: t.triple,
            commit: &self.facts.commit,
            source_url: &self.facts.source_url,
            crates: &all_crates,
            mit_licence: &mit,
            fonts: &fonts,
        });
        write(&dir.join("NOTICE.txt"), text.as_bytes())?;
        write(&dir.join("LICENSE"), mit.as_bytes())?;
        let client_name = client_in_release(t)
            .rsplit('/')
            .next()
            .unwrap_or(CLIENT)
            .to_owned();
        let tool_name = format!("{TOOL}{exe}");
        let pages = [
            (client_name.as_str(), client_page),
            (tool_name.as_str(), tool_page),
        ];
        write(
            &dir.join("THIRD-PARTY-LICENSES.html"),
            notice::splice_licence_pages(&pages)?.as_bytes(),
        )?;
        Ok(dir)
    }

    /// Scan the folder `dir`, check its programs, archive it under `root`, scan the archive, and
    /// write its manifest lines.
    fn finish_tree(&self, dir: &Path, root: &str) -> Result<(), String> {
        let t = self.target;
        let (required, optional) = contents(t);
        let staged = guard::staged_entries(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let findings = guard::scan_tree(&staged, &required, &optional, FILE_CAP, TOTAL_CAP);
        if !findings.is_empty() {
            return Err(format!(
                "the deny scan refused {}:\n  {}",
                dir.display(),
                findings.join("\n  ")
            ));
        }
        let client = client_in_release(t);
        let tool = tool_in_release(t);
        let mut described = Vec::new();
        let d = self.check_client(&read(&dir.join(&client))?)?;
        println!("  {client}: {d}");
        described.push((client.clone(), d));
        let d = self.check_tool(&read(&dir.join(&tool))?)?;
        println!("  {tool}: {d}");
        described.push((tool.clone(), d));
        let mut members = Vec::new();
        for e in &staged {
            if e.kind == Kind::File {
                let path = dir.join(&e.path);
                members.push(Member {
                    name: e.path.clone(),
                    bytes: read(&path)?,
                    executable: executable(&path) || e.path == client || e.path == tool,
                });
            }
        }
        let named: Vec<(&str, &[u8])> = members
            .iter()
            .map(|m| (m.name.as_str(), m.bytes.as_slice()))
            .collect();
        self.refuse_machine_folders(dir, &named)?;
        let file = release_file_name(t, self.version);
        let path = self.out_dir.join(&file);
        let bytes = if t.os == Os::Mac {
            let folder = dir
                .parent()
                .ok_or_else(|| format!("{} has no parent folder", dir.display()))?;
            let identity = dmg::signing_identity();
            let notary = dmg::Notary::from_env()?;
            dmg::make(
                &self.stage.join("dmg"),
                folder,
                root,
                &path,
                identity.as_deref(),
                notary.as_ref(),
            )?;
            dmg::check(
                self.stage, &path, root, &required, &optional, FILE_CAP, TOTAL_CAP,
            )?;
            read(&path)?
        } else {
            let bytes = if t.os == Os::Windows {
                archive::zip_bytes(root, &members, self.facts.epoch)?
            } else {
                archive::tar_gz_bytes(root, &members, self.facts.epoch)?
            };
            write(&path, &bytes)?;
            check_archive(&path, t, self.version)?;
            bytes
        };
        println!(
            "  {}: {} bytes, sha256 {}",
            path.display(),
            bytes.len(),
            sha256_hex(&bytes)
        );
        // The disk image itself is recorded too, so the index can check it where it cannot be
        // opened (any system but macOS).
        let own = (t.os == Os::Mac).then(|| (bytes.len(), sha256_hex(&bytes)));
        let lines: Vec<(String, Vec<u8>, String)> = members
            .into_iter()
            .map(|m| {
                let d = described
                    .iter()
                    .find(|(n, _)| *n == m.name)
                    .map_or(String::new(), |(_, d)| d.clone());
                (m.name, m.bytes, d)
            })
            .collect();
        self.write_manifest(&file, own, &lines)
    }

    /// Refuse the package at `what` when any of `files` (name in the package, contents) names one
    /// of this machine's folders: the checkout, the target folder, the home folder or cargo's.
    fn refuse_machine_folders(&self, what: &Path, files: &[(&str, &[u8])]) -> Result<(), String> {
        let folders = guard::local_folders(self.ws);
        let findings: Vec<String> = files
            .iter()
            .flat_map(|(name, bytes)| {
                guard::local_paths_in(bytes, &folders)
                    .into_iter()
                    .map(move |f| format!("{name}: names this machine's folder {f}"))
            })
            .collect();
        if findings.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "the deny scan refused {}:\n  {}",
                what.display(),
                findings.join("\n  ")
            ))
        }
    }

    /// `<file>.manifest`: the disk image's own size and SHA-256 when there is one, then each
    /// member's, with what the header check read.
    fn write_manifest(
        &self,
        file: &str,
        own: Option<(usize, String)>,
        lines: &[(String, Vec<u8>, String)],
    ) -> Result<(), String> {
        let mut text = String::new();
        if let Some((size, sha)) = own {
            text.push_str(&format!(
                "{}\t{file}\t{size}\t{sha}\tdisk image\n",
                self.target.triple
            ));
        }
        for (name, bytes, d) in lines {
            text.push_str(&format!(
                "{}\t{file}/{name}\t{}\t{}\t{d}\n",
                self.target.triple,
                bytes.len(),
                sha256_hex(bytes)
            ));
        }
        write(
            &self.out_dir.join(format!("{file}.manifest")),
            text.as_bytes(),
        )
    }
}

/// Make the file at `path` executable.
fn set_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("{}: {e}", path.display()))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

/// The members of the tool's archive for `target`: the program, then the license, the notice and
/// the third-party licenses' page.
pub fn tool_members(
    target: Target,
    program: Vec<u8>,
    mit: &str,
    notice: &str,
    licenses: &str,
) -> Vec<Member> {
    let text = |name: &str, body: &str| Member {
        name: name.to_owned(),
        bytes: body.as_bytes().to_vec(),
        executable: false,
    };
    vec![
        Member {
            name: format!("{TOOL}{}", target.exe_suffix()),
            bytes: program,
            executable: true,
        },
        text("LICENSE", mit),
        text("NOTICE.txt", notice),
        text("THIRD-PARTY-LICENSES.html", licenses),
    ]
}

/// The tool's archive: `members` under [`tool_root`], a zip on Windows and a tarball elsewhere,
/// each file dated `epoch`.
///
/// # Errors
/// The archive could not be written.
pub fn tool_archive(
    target: Target,
    version: &str,
    members: &[Member],
    epoch: i64,
) -> Result<Vec<u8>, String> {
    let root = tool_root(version);
    if target.os == Os::Windows {
        archive::zip_bytes(&root, members, epoch)
    } else {
        archive::tar_gz_bytes(&root, members, epoch)
    }
}

/// What the deny scan finds in the tool's archive, as read back: exactly [`tool_contents`], none
/// of them named like the game's data or carrying its header, under the caps.
pub fn tool_findings(entries: &[Entry], target: Target) -> Vec<String> {
    guard::scan_tree(entries, &tool_contents(target), &[], FILE_CAP, TOTAL_CAP)
}

/// The deny scan of a written tool archive.
pub(crate) fn check_tool_archive(path: &Path, target: Target, version: &str) -> Result<(), String> {
    let entries = archive::archive_entries(path, &tool_root(version))?;
    let findings = tool_findings(&entries, target);
    if findings.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the deny scan refused {}:\n  {}",
            path.display(),
            findings.join("\n  ")
        ))
    }
}

/// The deny scan of a written release file.
pub(crate) fn check_archive(path: &Path, target: Target, version: &str) -> Result<(), String> {
    let entries: Vec<Entry> = archive::archive_entries(path, &archive_root(target, version))?;
    let (required, optional) = contents(target);
    let findings = guard::scan_tree(&entries, &required, &optional, FILE_CAP, TOTAL_CAP);
    if findings.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the deny scan refused {}:\n  {}",
            path.display(),
            findings.join("\n  ")
        ))
    }
}

/// A disk image in the release folder: as its `.manifest` part records it, and on macOS mounted
/// and scanned again.
fn check_disk_image(dir: &Path, name: &str, target: Target) -> Result<(), String> {
    let bytes = read(&dir.join(name))?;
    let part = String::from_utf8_lossy(&read(&dir.join(format!("{name}.manifest")))?).into_owned();
    if !dmg::recorded(&part, name, bytes.len(), &sha256_hex(&bytes)) {
        return Err(format!("{name} is not the disk image its manifest records"));
    }
    if cfg!(target_os = "macos") {
        let scratch = std::env::temp_dir().join(format!("pyreal-index-{}", std::process::id()));
        let (required, optional) = contents(target);
        let checked = dmg::check(
            &scratch,
            &dir.join(name),
            APP,
            &required,
            &optional,
            FILE_CAP,
            TOTAL_CAP,
        );
        let _ = std::fs::remove_dir_all(&scratch);
        checked?;
    }
    Ok(())
}

/// The index over a release folder: every release file scanned again, then `MANIFEST.txt`,
/// `release.json` and `SHA256SUMS`.
pub(crate) fn write_index(dir: &Path, version: &str, facts: &BuildFacts) -> Result<(), String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let mut clients = Vec::new();
    let mut tools = Vec::new();
    let mut parts = Vec::new();
    let mut refused = Vec::new();
    for name in &names {
        match classify(name, version) {
            Ok(Asset::Client(t)) => clients.push((name.clone(), t)),
            Ok(Asset::Tool(t)) => tools.push((name.clone(), t)),
            Ok(Asset::ManifestPart) => parts.push(name.clone()),
            Ok(Asset::Index) => {}
            Err(e) => refused.push(e),
        }
    }
    if !refused.is_empty() {
        return Err(format!(
            "the release folder {} holds:\n  {}",
            dir.display(),
            refused.join("\n  ")
        ));
    }
    if clients.is_empty() {
        return Err(format!(
            "no Pyreal {version} release files in {}",
            dir.display()
        ));
    }
    for (name, t) in &clients {
        if name.ends_with(".dmg") {
            check_disk_image(dir, name, *t)?;
        } else {
            check_archive(&dir.join(name), *t, version)?;
        }
        if !tools.iter().any(|(_, tt)| tt == t) {
            return Err(format!(
                "{name} has no plugin author's tool beside it ({})",
                tool_file_name(*t, version)
            ));
        }
    }
    for (name, t) in &tools {
        check_tool_archive(&dir.join(name), *t, version)?;
        if !clients.iter().any(|(_, ct)| ct == t) {
            return Err(format!("{name} has no client file beside it"));
        }
    }
    let mut manifest = format!(
        "# pyreal {version}, commit {}\n# target\tfile\tsize\tsha256\theader\n",
        facts.commit
    );
    for p in &parts {
        manifest.push_str(&String::from_utf8_lossy(&read(&dir.join(p))?));
    }
    write(&dir.join("MANIFEST.txt"), manifest.as_bytes())?;
    let mut assets = Vec::new();
    for (program, list) in [("pyreal", &clients), (TOOL, &tools)] {
        for (name, t) in list {
            let bytes = read(&dir.join(name))?;
            assets.push(serde_json::json!({
                "program": program,
                "target": t.triple,
                "platform": t.updater_platform(),
                "file": name,
                "size": bytes.len(),
                "sha256": sha256_hex(&bytes),
            }));
        }
    }
    let release = serde_json::json!({
        "schema": 1,
        "product": "pyreal",
        "version": version,
        "tag": Product::Pyreal.tag_for(version),
        "commit": facts.commit,
        "commit_time": facts.iso_time(),
        "source_url": facts.source_url,
        "prerelease": version::is_prerelease(version),
        "assets": assets,
    });
    let mut json = serde_json::to_string_pretty(&release).map_err(|e| e.to_string())?;
    json.push('\n');
    write(&dir.join("release.json"), json.as_bytes())?;
    let mut summed: Vec<String> = clients
        .iter()
        .chain(&tools)
        .map(|(n, _)| n.clone())
        .collect();
    summed.extend(["MANIFEST.txt".to_owned(), "release.json".to_owned()]);
    summed.sort();
    let mut sums = String::new();
    for name in &summed {
        sums.push_str(&format!(
            "{}  {name}\n",
            sha256_hex(&read(&dir.join(name))?)
        ));
    }
    write(&dir.join("SHA256SUMS"), sums.as_bytes())?;
    println!("\nSHA256SUMS\n{sums}");
    Ok(())
}
