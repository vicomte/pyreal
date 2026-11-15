//! `cargo xtask bundle-macos`: the client, assembled into the `Pyreal.app` that macOS reads an
//! application's identity out of. `package pyreal` lays its bundle out the same way
//! ([`lay_out`]).
//!
//! A Mach-O binary is not an application on this platform: the Dock, Finder, the application
//! switcher and Launch Services all read the name, the version and the icon out of a *bundle* --
//! a directory with a `Contents/Info.plist` -- and a binary cargo built has none, which is why a
//! plain `cargo run` shows a generic tile with the executable's name under it. This command is
//! the answer to that, and the only one: the client sets no icon of its own at run time, because
//! doing so means asking AppKit at a moment the system does not document, and the bundle is the
//! mechanism macOS actually supports. Running the binary *inside* the bundle
//! (`Pyreal.app/Contents/MacOS/Pyreal`) is a bundled run like any other -- same terminal,
//! same log on stderr, right icon -- so a developer who wants the icon has it without leaving
//! the command line.
//!
//! What it assembles, from parts that already exist in the tree:
//!
//! ```text
//! Pyreal.app/Contents/Info.plist              name, identifier, version, icon keys, URL scheme
//! Pyreal.app/Contents/MacOS/Pyreal            the binary cargo just built (`pyreal`)
//! Pyreal.app/Contents/Resources/…             the icon, in whichever of its two forms
//! Pyreal.app/Contents/Resources/base-pack.dat   the base art pack, with --base-pack <file>
//! ```
//!
//! **The icon has two forms, and which one is built depends on the machine.** macOS 26 draws an
//! application's icon itself -- the rounded square, the material, the shadow, the light and dark
//! and tinted variants -- from a layered source, `assets/Pyreal.icon`, which `actool` (Xcode's
//! asset compiler) turns into an `Assets.car` and a plain `.icns` beside it for the systems that
//! predate all that. Handed a flat `.icns` instead, macOS 26 masks it into the same
//! square itself, which is the older and plainer result. So: `actool` is used when it is on the
//! machine, and when it is not the flat `assets/Pyreal.icns` (rendered from `assets/pyreal.svg`
//! by `assets/render-icon.py`) is copied in, so a host with no developer tools still builds a
//! bundle with an icon, simply the one an older macOS would have shown. With [`ICON_DIR_ENV`]
//! set, both forms are taken from that folder instead: a personal build's own emblem.
//!
//! **The dats are the one thing a bundle cannot carry.** The app asks for their folder on its own
//! screens and keeps it; `--dat-dir <dir>` links the four retail files into `Contents/MacOS/`,
//! where the client also looks, for a development bundle that should not ask. Links rather than
//! copies: the files are multiple gigabytes and the install is read-only for a run anyway.

use std::path::{Path, PathBuf};

use pyreal_dat::ModernDat;

use crate::util::{run, workspace_root};

/// The bundle's name, and with it the name under the icon in the Dock.
pub(crate) const APP_NAME: &str = "Pyreal";

/// The client's crate.
const PACKAGE: &str = "pyreal-client";

/// The binary cargo builds.
const BINARY: &str = "pyreal";

/// The bundle's executable, `Contents/MacOS/<this>`, named in `Info.plist` as `CFBundleExecutable`.
pub(crate) const EXECUTABLE: &str = "Pyreal";

/// The icon's name: `pyreal/client/assets/<this>.icon` (layered) and `<this>.icns` (flat).
const ICON: &str = "Pyreal";

/// A folder holding the icon's two forms to use instead of the committed ones, as
/// `render-icon.py --out` writes them (build.py's `icon_emblem`).
const ICON_DIR_ENV: &str = "PYREAL_ICON_DIR";

/// Launch Services' identity for the application: what it remembers a permission grant, a window
/// position or a default-application choice under. Stable across builds on purpose -- changing it
/// makes the system treat the result as a different application -- and reverse-DNS by convention.
pub(crate) const BUNDLE_ID: &str = "com.jacobjohnston.Pyreal";

/// The URL scheme of join links, which the bundle registers.
const URL_SCHEME: &str = "pyreal";

/// The oldest macOS this is claimed to run on. The client's own floor, not the toolchain's.
const MINIMUM_MACOS: &str = "11.0";

/// `cargo xtask bundle-macos [--debug] [--dat-dir <dir>] [--base-pack <file>]`.
pub fn bundle_macos(args: &[String]) -> i32 {
    let mut debug = false;
    let mut dat_dir: Option<PathBuf> = None;
    let mut base_pack: Option<PathBuf> = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--debug" => debug = true,
            "--dat-dir" => match rest.next() {
                Some(dir) => dat_dir = Some(PathBuf::from(dir)),
                None => {
                    eprintln!(
                        "bundle-macos: --dat-dir takes the directory holding the retail dats"
                    );
                    return 2;
                }
            },
            "--base-pack" => match rest.next() {
                Some(file) => base_pack = Some(PathBuf::from(file)),
                None => {
                    eprintln!("bundle-macos: --base-pack takes the base pack's file");
                    return 2;
                }
            },
            other => {
                eprintln!("bundle-macos: unexpected argument `{other}`");
                eprintln!(
                    "usage: cargo xtask bundle-macos [--debug] [--dat-dir <dir>] [--base-pack <file>]"
                );
                return 2;
            }
        }
    }
    // The bundle holds a macOS binary and is assembled with macOS's own layout; building one
    // anywhere else would produce a directory that cannot be run and would not say so until it
    // was carried to a Mac.
    if !cfg!(target_os = "macos") {
        eprintln!("bundle-macos: this command builds a macOS application and runs on macOS");
        return 2;
    }
    if let Some(dir) = &dat_dir {
        if !pyreal_dat::holds_modern_dats(dir) {
            eprintln!("bundle-macos: no retail dats in {}", dir.display());
            return 1;
        }
    }
    if let Some(file) = &base_pack {
        if let Err(e) = crate::package::pyreal::check_base_pack(file) {
            eprintln!("bundle-macos: {e}");
            return 1;
        }
    }
    match assemble(debug, dat_dir.as_deref(), base_pack.as_deref()) {
        Ok(app) => {
            report(&app, dat_dir.as_deref(), base_pack.is_some());
            0
        }
        Err(e) => {
            eprintln!("bundle-macos: {e}");
            1
        }
    }
}

/// Build the binary and lay the bundle out around it, answering where it is.
fn assemble(
    debug: bool,
    dat_dir: Option<&Path>,
    base_pack: Option<&Path>,
) -> Result<PathBuf, String> {
    let root = workspace_root();
    let (profile_dir, cargo_args): (&str, &[&str]) = if debug {
        ("debug", &["build", "-p", PACKAGE])
    } else {
        ("release", &["build", "--release", "-p", PACKAGE])
    };
    if !run(&root, "cargo", cargo_args) {
        return Err("the client did not build".to_owned());
    }

    let target = root.join("target").join(profile_dir);
    let binary = target.join(BINARY);
    if !binary.is_file() {
        return Err(format!("{} is not where cargo left it", binary.display()));
    }

    // Replaced whole rather than written over: a bundle left from an earlier build can hold a
    // file this one does not write, and a half-old application is the kind of thing that is
    // debugged for an hour.
    let app = target.join(format!("{APP_NAME}.app"));
    if app.exists() {
        std::fs::remove_dir_all(&app).map_err(|e| format!("{}: {e}", app.display()))?;
    }
    lay_out(
        &root,
        &app,
        &binary,
        &target.join("icon-build"),
        env!("CARGO_PKG_VERSION"),
    )?;
    let macos = app.join("Contents/MacOS");
    let resources = app.join("Contents/Resources");
    if let Some(pack) = base_pack {
        copy(pack, &resources.join(crate::package::pyreal::BASE_PACK))?;
    }

    if let Some(dats) = dat_dir {
        let dats = std::fs::canonicalize(dats).map_err(|e| format!("{}: {e}", dats.display()))?;
        for dat in ModernDat::ALL {
            let file = dat.in_dir(&dats);
            // The high-resolution partition is the optional one; the other three are not, and
            // `holds_modern_dats` has already reported on the file that stands for the set.
            if file.is_file() {
                let at = macos.join(dat.file_name());
                link(&file, &at).map_err(|e| format!("{}: {e}", at.display()))?;
            }
        }
    }
    Ok(app)
}

/// Lay the bundle `app` out around the client binary `client`: its plist at `version`, the
/// four-byte `PkgInfo`, the icon (built in `work`) and the program as [`EXECUTABLE`].
///
/// # Errors
/// A folder or file could not be written, or the icon could not be built.
pub(crate) fn lay_out(
    root: &Path,
    app: &Path,
    client: &Path,
    work: &Path,
    version: &str,
) -> Result<(), String> {
    let macos = app.join("Contents/MacOS");
    let resources = app.join("Contents/Resources");
    for dir in [&macos, &resources] {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let form = install_icon(root, work, &resources)?;
    write(&app.join("Contents/Info.plist"), &info_plist(form, version))?;
    // Four bytes of classic Mac OS that the Finder still reads before the plist.
    write(&app.join("Contents/PkgInfo"), "APPL????")?;
    copy(client, &macos.join(EXECUTABLE))
}

/// Which of the two icon forms a bundle ended up with. The plist's icon keys differ, so the
/// answer travels from the step that built it to the step that writes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IconForm {
    /// `assets/Pyreal.icon`, compiled: `Assets.car` for macOS 26's own drawing of it, and the
    /// `.icns` `actool` generates beside it for every system before that.
    Composed,
    /// The flat `assets/Pyreal.icns`: what a machine without Xcode gets, and what macOS 26 will
    /// mask into its rounded square itself.
    Fallback,
}

/// Put the icon in `Contents/Resources`, by whichever route this machine can take, building it in
/// `built`.
fn install_icon(root: &Path, built: &Path, resources: &Path) -> Result<IconForm, String> {
    let assets = std::env::var_os(ICON_DIR_ENV)
        .filter(|d| !d.is_empty())
        .map_or_else(|| root.join("pyreal/client/assets"), PathBuf::from);
    let source = assets.join(format!("{ICON}.icon"));
    if !source.is_dir() {
        return Err(format!("{} is missing", source.display()));
    }
    // A directory of its own for the intermediate files: `actool` writes three outputs beside
    // each other, and none of that belongs in the bundle or in the source tree.
    if built.exists() {
        std::fs::remove_dir_all(built).map_err(|e| format!("{}: {e}", built.display()))?;
    }
    std::fs::create_dir_all(built).map_err(|e| format!("{}: {e}", built.display()))?;
    let icns = format!("{ICON}.icns");

    if !actool_is_installed() {
        println!("bundle-macos: no actool (Xcode) here; using the flat {icns}");
        copy(&assets.join(&icns), &resources.join(&icns))?;
        return Ok(IconForm::Fallback);
    }

    if !actool_compile(root, &source, ICON, MINIMUM_MACOS, built) {
        return Err(format!("actool could not compile {}", source.display()));
    }
    // Both outputs, because they are for different systems: `Assets.car` is what macOS 26 reads,
    // and the generated `.icns` is what every earlier one reads.
    copy(&built.join("Assets.car"), &resources.join("Assets.car"))?;
    copy(&built.join(&icns), &resources.join(&icns))?;
    Ok(IconForm::Composed)
}

/// Compile the layered icon `source` (an `.icon` named `name`) with Xcode's `actool` into `out`:
/// `Assets.car`, which macOS 26 draws the icon from, and `<name>.icns` beside it for every earlier
/// system. `minimum` is the floor the bundle's plist claims, so the compiler makes what that claim
/// needs. Whether it compiled.
pub(crate) fn actool_compile(
    root: &Path,
    source: &Path,
    name: &str,
    minimum: &str,
    out: &Path,
) -> bool {
    let partial = out.join("partial-info.plist");
    run(
        root,
        "xcrun",
        &[
            "actool",
            "--output-format",
            "human-readable-text",
            "--notices",
            "--warnings",
            "--app-icon",
            name,
            "--output-partial-info-plist",
            &partial.to_string_lossy(),
            "--target-device",
            "mac",
            "--platform",
            "macosx",
            "--minimum-deployment-target",
            minimum,
            "--compile",
            &out.to_string_lossy(),
            &source.to_string_lossy(),
        ],
    )
}

/// Whether Xcode's asset compiler is on this machine. `xcrun --find` is the question the
/// toolchain answers for itself, and it answers it without compiling anything.
pub(crate) fn actool_is_installed() -> bool {
    std::process::Command::new("xcrun")
        .args(["--find", "actool"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// `Info.plist`: what Finder, the Dock and Launch Services read instead of asking the binary.
///
/// `CFBundleIconFile` names the file in `Contents/Resources`, `CFBundleExecutable` the one in
/// `Contents/MacOS`, and `NSHighResolutionCapable` is what stops a Retina display from scaling
/// the window up by two instead of handing the client the pixels it asked for.
fn info_plist(icon: IconForm, version: &str) -> String {
    // `CFBundleIconFile` names a file in `Contents/Resources` and is read by every macOS;
    // `CFBundleIconName` names an icon inside `Assets.car` and is what macOS 26 looks for first.
    // A composed bundle carries both, because it holds both.
    let icon_keys = match icon {
        IconForm::Composed => format!(
            "\t<key>CFBundleIconFile</key>\n\t<string>{ICON}</string>\n\t\
             <key>CFBundleIconName</key>\n\t<string>{ICON}</string>"
        ),
        IconForm::Fallback => {
            format!("\t<key>CFBundleIconFile</key>\n\t<string>{ICON}</string>")
        }
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key>
	<string>{APP_NAME}</string>
	<key>CFBundleDisplayName</key>
	<string>{APP_NAME}</string>
	<key>CFBundleIdentifier</key>
	<string>{BUNDLE_ID}</string>
	<key>CFBundleExecutable</key>
	<string>{EXECUTABLE}</string>
{icon_keys}
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleShortVersionString</key>
	<string>{version}</string>
	<key>CFBundleVersion</key>
	<string>{version}</string>
	<key>LSMinimumSystemVersion</key>
	<string>{MINIMUM_MACOS}</string>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.role-playing-games</string>
	<key>LSSupportsGameMode</key>
	<true/>
	<key>GCSupportsGameMode</key>
	<true/>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>CFBundleURLTypes</key>
	<array>
		<dict>
			<key>CFBundleURLName</key>
			<string>{BUNDLE_ID}.join</string>
			<key>CFBundleURLSchemes</key>
			<array>
				<string>{URL_SCHEME}</string>
			</array>
		</dict>
	</array>
</dict>
</plist>
"#
    )
}

/// What was built, and the two ways to start it.
fn report(app: &Path, dat_dir: Option<&Path>, base_pack: bool) {
    println!("\nbundle-macos: {}", app.display());
    if !base_pack {
        println!("  no base pack in the bundle: the client draws without one");
    }
    match dat_dir {
        Some(dir) => println!(
            "  the retail dats are linked from {}, so opening it in Finder works",
            dir.display()
        ),
        None => println!(
            "  no dats in the bundle: it asks for their folder on its first run, or rebuild with \
             --dat-dir <dir> to link them in"
        ),
    }
    println!(
        "  a bundle in a build directory is a bundle Launch Services has not indexed; copy it to \
         /Applications to give it a permanent identity"
    );
}

/// `fs::write`, with the path in the error.
fn write(at: &Path, contents: &str) -> Result<(), String> {
    std::fs::write(at, contents).map_err(|e| format!("{}: {e}", at.display()))
}

/// `fs::copy`, with both paths in the error. The binary keeps its mode bits, and with them the
/// execute permission that makes it the bundle's executable.
fn copy(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| format!("{} -> {}: {e}", from.display(), to.display()))
}

/// A symbolic link at `at` pointing to `target`.
#[cfg(unix)]
fn link(target: &Path, at: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, at)
}

/// Unreachable: the command refuses to run off macOS before it reaches a link. Present so that
/// the module compiles, and is checked, on the hosts the rest of the workspace is built on.
#[cfg(not(unix))]
fn link(_target: &Path, _at: &Path) -> std::io::Result<()> {
    Err(std::io::Error::other("a symbolic link is a Unix facility"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The plist is the bundle's whole identity, and three of its keys are cross-references that
    /// a typo breaks silently: an application whose `CFBundleExecutable` names no file will not
    /// start, and one whose `CFBundleIconFile` names no file shows the generic icon -- which is
    /// the symptom this command exists to remove.
    #[test]
    fn info_plist_names_the_executable_and_the_icon_file_the_bundle_actually_holds() {
        for form in [IconForm::Composed, IconForm::Fallback] {
            let plist = info_plist(form, "1.2.3");
            assert!(
                plist.contains(&format!("<string>{EXECUTABLE}</string>")),
                "{plist}"
            );
            assert!(plist.contains("<string>pyreal</string>"), "{plist}");
            assert!(plist.contains("<string>1.2.3</string>"), "{plist}");
            assert!(
                plist.contains(&format!("<string>{BUNDLE_ID}</string>")),
                "{plist}"
            );
            assert!(plist.starts_with("<?xml version=\"1.0\""), "{plist}");
            assert!(plist.contains("<key>LSSupportsGameMode</key>"), "{plist}");
        }
    }

    /// The two forms name different files, and each has to name the one its own branch of
    /// `install_icon` actually wrote: a plist naming the other is an application that shows the
    /// generic icon and says nothing about why.
    #[test]
    fn each_icon_form_names_the_file_its_own_branch_installs() {
        let composed = info_plist(IconForm::Composed, "1.0.0");
        assert!(
            composed.contains("<key>CFBundleIconName</key>"),
            "{composed}"
        );
        assert!(
            composed.contains(&format!("<string>{ICON}</string>")),
            "{composed}"
        );
        let fallback = info_plist(IconForm::Fallback, "1.0.0");
        assert!(!fallback.contains("CFBundleIconName"), "{fallback}");
        assert!(
            fallback.contains("<key>CFBundleIconFile</key>"),
            "{fallback}"
        );
    }

    /// `--dat-dir` or `--base-pack` without its value, and any argument that is not one of the
    /// three, are usage errors rather than a bundle built from a guess.
    #[test]
    fn a_malformed_command_line_is_refused_and_builds_nothing() {
        assert_eq!(bundle_macos(&["--dat-dir".to_owned()]), 2);
        assert_eq!(bundle_macos(&["--base-pack".to_owned()]), 2);
        assert_eq!(bundle_macos(&["--everything".to_owned()]), 2);
    }
}
