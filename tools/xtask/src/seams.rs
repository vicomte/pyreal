//! The seams between the game, the contract and the interface, checked rather than described.
//!
//! Four crates, and the client's own crates, sit on the boundaries the code is being cut along,
//! and each has a rule that until now lived only in a doc comment:
//!
//! 0. **`pyreal-primitives`** is plain data and pure conversions: no domain logic, no engine code,
//!    no I/O. Its direct production dependencies are exactly [`PRIMITIVES_DIRECT`], nothing in its
//!    tree is another `pyreal-*` crate or a platform crate, its code names no I/O facility of
//!    [`PRIMITIVES_NO_IO`], and the only `unsafe` in it is `core/primitives/src/text/windows.rs`, the host NLS arm,
//!    declared behind the `host-nls` feature.
//! 1. **`pyreal-client-contract`** is the read-only view onto the game and the request channel
//!    back out. Its direct production dependencies are exactly [`CONTRACT_DIRECT`], and nothing it
//!    pulls in, transitively, is another `pyreal-*` crate or a presentation or platform crate.
//! 2. **`pyreal-ui-logic`** is the game's words, tables and rules the interface reads. It reads
//!    the game through the contract only, so it never names `pyreal-client-model` or
//!    `pyreal-client-runtime` -- not as a dependency of any kind, and not as a path in its code.
//! 3. **`pyreal-client-runtime`** is the game without a UI, a device or a window. Its production
//!    dependency tree holds none of [`CORE_FORBIDDEN`] and no crate under `pyreal/client/crates/`,
//!    and its code never names the interface or the drawing crates ([`CORE_NAMES_NOT`]).
//! 5. **The client's own crates** (`pyreal/client/crates/*`) are presentation. Those that draw or
//!    lay out the game ([`PRESENTATION_ON_CONTRACT`]) depend directly on `pyreal-client-contract`,
//!    and none of them has `pyreal-client-runtime` anywhere in any dependency table: rendering
//!    sees the game only through the contract.
//! 6. **The runtime's input seam is actions.** The code of [`ACTION_SEAM_CRATES`] names no key,
//!    key map, input map or device message ([`DEVICE_NAMES_NOT`]): a bot, a script or another
//!    front end tells the runtime what the player does with an action, and turning a device into
//!    actions is the product's own business.
//! 7. **Libraries log through the facade.** No crate under `core/` or `pyreal/client/crates/` has
//!    a log subscriber ([`LIBRARY_NO_SUBSCRIBER`]) anywhere in its production tree: they emit
//!    `tracing` events, and the binary that runs them installs the one subscriber that decides
//!    where the events go. (`pyreal-primitives` does not log at all; rule 0 keeps `tracing` out of
//!    its tree.)
//! 9. **The application's halves** ([`APPLICATION_HALVES`]): `pyreal-scene`, the drawn world,
//!    and the front end: `pyreal-client-shell`, the shell, and `pyreal-horizon`, the interface it
//!    runs. They sit on the runtime, so rule 5's runtime ban
//!    does not apply to them, and they reach no platform: no table of theirs names a platform
//!    crate or the desktop's own platform crates ([`HALVES_FORBIDDEN`]), and their code names none
//!    of them ([`HALVES_NAMES_NOT`]). The platform arrives through the shell's host trait, which
//!    the desktop host (`pyreal-desktop`) implements. The scene does not depend on the shell.
//! 10. **A front end never holds the application.** A front end's code ([`FRONT_ENDS`]: the
//!     shell's `src/`, but for [`FRONT_END_ASSEMBLY`], where the executable puts the application
//!     and its front end side by side, and the Horizon interface's) names none of
//!     [`FRONT_END_NAMES_NOT`]: the
//!     application and the runtime's internals behind it. Each step of the frame hands a front end
//!     a `UiContext`, and that is all of the game it reaches. (The game's words it reads,
//!     `pyreal-ui-logic`, cannot name the runtime at all: rule 2.)
//! 11. **The plugin host's crates** keep to their layers. `pyreal-plugin-api` (the interface
//!     as types) depends directly on [`PLUGIN_API_DIRECT`] only; `pyreal-plugin-package` (packages
//!     as files) on [`PLUGIN_PACKAGE_DIRECT`] only; `pyreal-plugin-wasm` (compiled plugins) on
//!     [`PLUGIN_WASM_DIRECT`] only; `pyreal-plugin-lua` (the engine)
//!     on [`PLUGIN_LUA_DIRECT`] only, with no other `pyreal-*` crate in its tree; and
//!     `pyreal-plugin-host` reaches the game with no crate under `pyreal/client/` anywhere in its
//!     tree and none of their names in its code.
//! 8. **One place finds the retail dats.** No workspace file outside `core/dat/` joins a retail dat
//!    file name onto a path ([`dat_path_violations`]): the client and every test find the
//!    directory with `pyreal_dat::locate_modern_dats` (the tests through `pyreal_dat::testing`)
//!    and name a file in it with `pyreal_dat::ModernDat`. And no tracked file of the workspace
//!    names a retired environment variable ([`env_name_violations`]).
//!
//! The dependency half reads `cargo tree`, so a rule broken two crates down is still caught. The
//! code half is a text scan with comments stripped: the crates' docs are allowed to talk about the
//! other side of a seam, their code is not allowed to reach across it.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::util::{print_table, workspace_root, Outcome, Report};

/// `pyreal-primitives`' allowed direct production dependencies, with its default features: the
/// error derive (`thiserror`) and the two portable math libraries `num::math` is built on (`pxfm`,
/// `libm`). Adding one is a decision about the lowest crate in the workspace, made here in review.
/// No geometry library: the shapes are over the crate's own `Vec3`.
pub const PRIMITIVES_DIRECT: &[&str] = &["thiserror", "pxfm", "libm"];

/// Platform crates `pyreal-primitives` must not reach, at any depth.
const PRIMITIVES_FORBIDDEN: &[&str] = &["winit", "windows", "windows-sys", "cpal", "ash", "libc"];

/// What `pyreal-primitives`' production code must not name: the standard library's I/O, process,
/// thread, environment and clock facilities. (`std::io::Error` as a *type* in an error enum is
/// data, and is allowed.)
pub const PRIMITIVES_NO_IO: &[&str] = &[
    "std::fs",
    "std::net",
    "std::process",
    "std::thread",
    "std::env",
    "std::time",
    "std::io::stdin",
    "std::io::stdout",
    "std::io::stderr",
    "std::io::Read",
    "std::io::Write",
];

/// The one file of `pyreal-primitives` allowed `unsafe` and `extern` blocks, and the gate its
/// module declaration must carry.
const PRIMITIVES_UNSAFE_FILE: &str = "src/text/windows.rs";
const PRIMITIVES_UNSAFE_DECL: &str =
    "#[cfg(all(windows, feature = \"host-nls\"))]\n#[allow(unsafe_code)]\nmod windows;";

/// `pyreal-client-contract`'s allowed direct production dependencies. Adding one is a decision about the
/// seam, so it is made here, in review, and not by editing a manifest.
pub const CONTRACT_DIRECT: &[&str] = &["pyreal-primitives", "glam", "raw-window-handle"];

/// The only `pyreal-*` crates allowed anywhere in `pyreal-client-contract`'s production tree.
const CONTRACT_DERE: &[&str] = &["pyreal-client-contract", "pyreal-primitives"];

/// What `pyreal-ui-logic` must never depend on, in any dependency table.
const UI_LOGIC_FORBIDDEN: &[&str] = &[
    "pyreal-client-model",
    "pyreal-client-runtime",
    "pyreal-client",
];

/// What `pyreal-client-runtime`'s production tree must never contain: a UI, a device, the drawing,
/// the application or a platform. Every crate under `pyreal/client/crates/` is added to this list
/// when the check runs, so a new presentation crate is covered without editing it.
pub const CORE_FORBIDDEN: &[&str] = &[
    "pyreal-client",
    "pyreal-horizon",
    "pyreal-ui-logic",
    "pyreal-engine",
    "pyreal-codecs",
    "pyreal-world-render",
    "pyreal-clipboard",
    "pyreal-console",
    "winit",
    "windows",
    "cpal",
    "ash",
    "gilrs",
];

/// What `pyreal-client-runtime`'s code must never name.
pub const CORE_NAMES_NOT: &[&str] = &[
    "pyreal_horizon",
    "pyreal_ui_logic",
    "pyreal_codecs",
    "pyreal_world_render",
];

/// The crates whose code must speak actions and never devices.
pub const ACTION_SEAM_CRATES: &[&str] = &["pyreal-client-runtime"];

/// What the code of [`ACTION_SEAM_CRATES`] must never name: the device input crate, its key,
/// key-map and input-map types, the message it reads, and the keyboard and mouse window messages
/// and virtual keys. The window's lifecycle messages (close, focus, size) are not here: the
/// runtime owns the window procedure's state and feeds it those.
pub const DEVICE_NAMES_NOT: &[&str] = &[
    "pyreal_input",
    "InputEvent",
    "InputManager",
    "InputShell",
    "InputMapId",
    "MasterInputMap",
    "ControlCode",
    "ControlChord",
    "ToggleType",
    "Win32Message",
    "KeyCode",
    "KeyboardInput",
    "MouseInput",
    "scan_code",
    "virtual_key",
    "WM_KEYDOWN",
    "WM_KEYUP",
    "WM_SYSKEYDOWN",
    "WM_SYSKEYUP",
    "WM_CHAR",
    "WM_SYSCHAR",
    "WM_MOUSEMOVE",
    "WM_MOUSEWHEEL",
    "WM_MOUSELEAVE",
    "WM_LBUTTONDOWN",
    "WM_LBUTTONUP",
    "WM_RBUTTONDOWN",
    "WM_RBUTTONUP",
    "WM_MBUTTONDOWN",
    "WM_MBUTTONUP",
    "WM_XBUTTONDOWN",
    "WM_XBUTTONUP",
    "VK_RETURN",
    "VK_F4",
];

/// The client's own crates that draw or lay out the game, and so read it through the contract: each
/// depends directly on `pyreal-client-contract`.
pub const PRESENTATION_ON_CONTRACT: &[&str] = &["pyreal-codecs", "pyreal-ui-logic"];

/// What no crate under `pyreal/client/crates/` may have in any dependency table, at any depth,
/// except the application's two halves ([`APPLICATION_HALVES`]), which sit on the runtime.
const PRESENTATION_FORBIDDEN: &[&str] = &["pyreal-client-runtime"];

/// The application's two halves: the drawn world and the front end over the runtime. Rule 9.
pub const APPLICATION_HALVES: &[&str] = &["pyreal-scene", "pyreal-client-shell", "pyreal-horizon"];

/// The front ends rule 10 holds to their context.
pub const FRONT_ENDS: &[&str] = &["pyreal-client-shell", "pyreal-horizon"];

/// What no dependency table of an application half may name directly: the window system, the
/// operating system's bindings, the sound device, the pads, and the desktop's own platform crates.
pub const HALVES_FORBIDDEN: &[&str] = &[
    "winit",
    "windows",
    "windows-sys",
    "cpal",
    "gilrs",
    "pyreal-client",
    "pyreal-desktop",
    "pyreal-clipboard",
    "pyreal-console",
];

/// The crates whose paths the code of an application half must never name: `name::` anywhere, so
/// a `cfg(windows)` or a field called `windows` is not a path.
pub const HALVES_NAMES_NOT: &[&str] = &[
    "winit",
    "windows",
    "cpal",
    "gilrs",
    "pyreal_client",
    "pyreal_desktop",
    "pyreal_clipboard",
    "pyreal_console",
];

/// The one file of the retail front end's crate that holds the application: the assembly, which
/// builds the runtime's application beside the front end and feeds the one to the other.
pub const FRONT_END_ASSEMBLY: &str = "src/app.rs";

/// What a front end's code must not name: the application, and the runtime's internals a holder of
/// it would reach (the interaction state, the network link).
pub const FRONT_END_NAMES_NOT: &[&str] = &["App", "CoreApp", "Interaction", "NetLink"];

/// Presentation and platform crates the contract must not reach either.
const CONTRACT_FORBIDDEN: &[&str] = &["winit", "windows", "cpal", "ash"];

/// What no library crate's production tree may contain, at any depth: a log subscriber. The shared
/// crates under `core/` and the client's crates under `pyreal/client/crates/` emit events through
/// the `tracing` facade and nothing else; which subscriber receives them, and where it writes, is
/// the binary's decision (`pyreal-client`).
pub const LIBRARY_NO_SUBSCRIBER: &[&str] = &["tracing-subscriber"];

/// `pyreal-plugin-api`'s allowed direct production dependencies: the interface as types needs the
/// game's identifiers and nothing else.
pub const PLUGIN_API_DIRECT: &[&str] = &["pyreal-primitives"];

/// `pyreal-plugin-package`'s allowed direct production dependencies: the interface and the formats
/// a package is made of. The client's plugin installs build it, so it stays this small.
pub const PLUGIN_PACKAGE_DIRECT: &[&str] =
    &["pyreal-plugin-api", "toml", "zip", "sha2", "ed25519-dalek"];

/// `pyreal-plugin-wasm`'s allowed direct production dependencies: the interface, reading and
/// rewriting modules, the host's loan to the imports and the interpreter.
pub const PLUGIN_WASM_DIRECT: &[&str] = &[
    "pyreal-plugin-api",
    "wasmparser",
    "wasm-encoder",
    "scoped-tls-hkt",
    "wasmi",
];

/// `pyreal-plugin-lua`'s allowed direct production dependencies: the interface and the Luau VM.
pub const PLUGIN_LUA_DIRECT: &[&str] = &["pyreal-plugin-api", "mlua"];

/// Platform crates no plugin crate reaches. (The Luau VM's bindings use the C library's types, so
/// `libc` itself is not one of them.)
const PLUGIN_FORBIDDEN: &[&str] = &["winit", "windows", "cpal", "ash"];

/// One package in a `cargo tree --prefix depth` listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub depth: usize,
    pub name: String,
}

/// Parse `cargo tree --prefix depth -f "{p}"` output: `<depth><name> v<version> ...` per line.
pub fn parse_tree(text: &str) -> Vec<TreeEntry> {
    text.lines()
        .filter_map(|line| {
            let digits = line.chars().take_while(char::is_ascii_digit).count();
            if digits == 0 {
                return None;
            }
            let depth = line[..digits].parse().ok()?;
            let name = line[digits..].split_whitespace().next()?.to_owned();
            Some(TreeEntry { depth, name })
        })
        .collect()
}

/// The contract rule over its production tree.
pub fn contract_violations(tree: &[TreeEntry]) -> Vec<String> {
    let mut out = Vec::new();
    for e in tree {
        if e.depth == 1 && !CONTRACT_DIRECT.contains(&e.name.as_str()) {
            out.push(format!(
                "pyreal-client-contract depends directly on `{}`; allowed: {}",
                e.name,
                CONTRACT_DIRECT.join(", ")
            ));
        }
        if e.name.starts_with("pyreal-") && !CONTRACT_DERE.contains(&e.name.as_str()) {
            out.push(format!(
                "pyreal-client-contract's tree contains `{}` (depth {})",
                e.name, e.depth
            ));
        }
        if CONTRACT_FORBIDDEN.contains(&e.name.as_str()) {
            out.push(format!(
                "pyreal-client-contract's tree contains `{}` (depth {})",
                e.name, e.depth
            ));
        }
    }
    dedup(out)
}

/// The primitives rule over its production tree (default features).
pub fn primitives_violations(tree: &[TreeEntry]) -> Vec<String> {
    let mut out = Vec::new();
    for e in tree {
        if e.depth == 1 && !PRIMITIVES_DIRECT.contains(&e.name.as_str()) {
            out.push(format!(
                "pyreal-primitives depends directly on `{}`; allowed: {}",
                e.name,
                PRIMITIVES_DIRECT.join(", ")
            ));
        }
        if e.depth > 0 && e.name.starts_with("pyreal-") {
            out.push(format!(
                "pyreal-primitives' tree contains `{}` (depth {})",
                e.name, e.depth
            ));
        }
        if PRIMITIVES_FORBIDDEN.contains(&e.name.as_str()) {
            out.push(format!(
                "pyreal-primitives' tree contains `{}` (depth {})",
                e.name, e.depth
            ));
        }
    }
    dedup(out)
}

/// The primitives rule over its production code (`src/`, paths relative to the crate): no I/O
/// facility named, and `unsafe`/`extern` only in the NLS arm, whose declaration is feature-gated.
pub fn primitives_code_violations(files: &[(PathBuf, String)]) -> Vec<String> {
    let mut out = code_violations(files, PRIMITIVES_NO_IO);
    let unix = |p: &Path| p.to_string_lossy().replace('\\', "/");
    for (file, text) in files {
        let name = unix(file);
        if name.ends_with(PRIMITIVES_UNSAFE_FILE) {
            continue;
        }
        for (i, line) in text.lines().enumerate() {
            let code = strip_comment(line);
            let is_decl =
                name.ends_with("src/text/mod.rs") && code.trim() == "#[allow(unsafe_code)]";
            if !is_decl
                && (names_ident(code, "unsafe")
                    || names_ident(code, "unsafe_code")
                    || code.contains("extern \""))
            {
                out.push(format!(
                    "{}:{}: unsafe code outside {PRIMITIVES_UNSAFE_FILE}: {}",
                    file.display(),
                    i + 1,
                    line.trim()
                ));
            }
        }
        if name.ends_with("src/text/mod.rs")
            && !text.replace("\r\n", "\n").contains(PRIMITIVES_UNSAFE_DECL)
        {
            out.push(format!(
                "{}: `mod windows` must be declared exactly as `{}`",
                file.display(),
                PRIMITIVES_UNSAFE_DECL.replace('\n', " ")
            ));
        }
    }
    out
}

/// A forbidden-name rule over a crate's tree.
pub fn tree_violations(krate: &str, tree: &[TreeEntry], forbidden: &[&str]) -> Vec<String> {
    let out = tree
        .iter()
        .filter(|e| forbidden.contains(&e.name.as_str()))
        .map(|e| format!("{krate}'s tree contains `{}` (depth {})", e.name, e.depth))
        .collect();
    dedup(out)
}

/// Code lines (comments stripped) in `files` that name any of `paths`, e.g. `pyreal_client_model`.
///
/// A name counts when it is a whole identifier: `ui_logic` does not match inside `pyreal_ui_logic`.
pub fn code_violations(files: &[(PathBuf, String)], paths: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for (file, text) in files {
        for (i, line) in text.lines().enumerate() {
            let code = strip_comment(line);
            for p in paths {
                if names_ident(code, p) {
                    out.push(format!(
                        "{}:{}: names `{p}`: {}",
                        file.display(),
                        i + 1,
                        line.trim()
                    ));
                }
            }
        }
    }
    out
}

/// Code lines (comments stripped) in `files` that name a path in any of `crates`: the whole
/// identifier followed by `::`.
pub fn path_violations(files: &[(PathBuf, String)], crates: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for (file, text) in files {
        for (i, line) in text.lines().enumerate() {
            let code = strip_comment(line);
            for c in crates {
                if names_path(code, c) {
                    out.push(format!(
                        "{}:{}: names `{c}::`: {}",
                        file.display(),
                        i + 1,
                        line.trim()
                    ));
                }
            }
        }
    }
    out
}

fn names_path(code: &str, krate: &str) -> bool {
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut from = 0;
    while let Some(at) = code[from..].find(krate) {
        let start = from + at;
        let end = start + krate.len();
        let before = code[..start].chars().next_back();
        if !before.is_some_and(is_ident) && code[end..].starts_with("::") {
            return true;
        }
        from = end;
    }
    false
}

fn strip_comment(line: &str) -> &str {
    line.find("//").map_or(line, |i| &line[..i])
}

fn names_ident(code: &str, ident: &str) -> bool {
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut from = 0;
    while let Some(at) = code[from..].find(ident) {
        let start = from + at;
        let end = start + ident.len();
        let before = code[..start].chars().next_back();
        let after = code[end..].chars().next();
        if !before.is_some_and(is_ident) && !after.is_some_and(is_ident) {
            return true;
        }
        from = end;
    }
    false
}

fn dedup(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v.dedup();
    v
}

fn cargo_tree(ws: &Path, krate: &str, edges: &str) -> Result<Vec<TreeEntry>, String> {
    let out = Command::new("cargo")
        .args([
            "tree",
            "-e",
            edges,
            "-p",
            krate,
            "--prefix",
            "depth",
            "-f",
            "{p}",
            "--offline",
        ])
        .current_dir(ws)
        .output()
        .map_err(|e| format!("failed to launch cargo tree: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo tree -p {krate} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(parse_tree(&String::from_utf8_lossy(&out.stdout)))
}

/// The packages under `pyreal/client/crates/`, by their manifests' names, sorted.
fn client_crates(ws: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(ws.join("pyreal/client/crates"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path().join("Cargo.toml")).ok())
        .filter_map(|m| {
            m.lines()
                .find_map(|l| l.trim().strip_prefix("name = \"")?.strip_suffix('"'))
                .map(str::to_owned)
        })
        .collect();
    out.sort();
    out
}

/// The package names of the crates directly under `dir` (relative to the workspace), sorted:
/// each one's `name = "..."` from its manifest.
fn crates_under(ws: &Path, dir: &str) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(ws.join(dir))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path().join("Cargo.toml")).ok())
        .filter_map(|m| {
            m.lines()
                .find_map(|l| l.trim().strip_prefix("name = \"")?.strip_suffix('"'))
                .map(str::to_owned)
        })
        .collect();
    out.sort();
    out
}

/// The library rule over each library crate's production tree (`trees`, one per crate): none of
/// [`LIBRARY_NO_SUBSCRIBER`].
pub fn library_log_violations(trees: &[(String, Vec<TreeEntry>)]) -> Vec<String> {
    let mut out = Vec::new();
    for (krate, tree) in trees {
        out.extend(tree_violations(krate, tree, LIBRARY_NO_SUBSCRIBER));
    }
    out
}

/// The runtime rule over its production tree: none of [`CORE_FORBIDDEN`], and none of
/// `client_crates` (the packages under `pyreal/client/crates/`).
pub fn core_violations(tree: &[TreeEntry], client_crates: &[String]) -> Vec<String> {
    let mut forbidden: Vec<&str> = CORE_FORBIDDEN.to_vec();
    forbidden.extend(client_crates.iter().map(String::as_str));
    tree_violations("pyreal-client-runtime", tree, &forbidden)
}

/// A plugin crate's rule over its production tree: its direct dependencies are within `direct`, and
/// the only `pyreal-*` crates anywhere in it are `pyreal-primitives`, `pyreal-plugin-api` and itself.
pub fn plugin_layer_violations(krate: &str, tree: &[TreeEntry], direct: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for e in tree {
        if e.depth == 1 && !direct.contains(&e.name.as_str()) {
            out.push(format!(
                "{krate} depends directly on `{}`; allowed: {}",
                e.name,
                direct.join(", ")
            ));
        }
        if e.depth > 0
            && e.name.starts_with("pyreal-")
            && !["pyreal-primitives", "pyreal-plugin-api", krate].contains(&e.name.as_str())
        {
            out.push(format!(
                "{krate}'s tree contains `{}` (depth {})",
                e.name, e.depth
            ));
        }
    }
    out.extend(tree_violations(krate, tree, PLUGIN_FORBIDDEN));
    dedup(out)
}

/// The plugin host's rule over its production tree: neither `pyreal-client` nor any crate under
/// `pyreal/client/crates/` (`client_crates`) appears at any depth.
pub fn plugin_host_violations(tree: &[TreeEntry], client_crates: &[String]) -> Vec<String> {
    let mut forbidden: Vec<&str> = vec!["pyreal-client"];
    forbidden.extend(client_crates.iter().map(String::as_str));
    tree_violations("pyreal-plugin-host", tree, &forbidden)
}

/// The client-crates rule: every crate in `crates` has none of [`PRESENTATION_FORBIDDEN`] in its
/// tree (`trees`, one per crate, every dependency kind), and each of
/// [`PRESENTATION_ON_CONTRACT`] among them depends directly on `pyreal-client-contract`.
pub fn presentation_violations(trees: &[(String, Vec<TreeEntry>)]) -> Vec<String> {
    let mut out = Vec::new();
    for (krate, tree) in trees {
        if !APPLICATION_HALVES.contains(&krate.as_str()) {
            out.extend(tree_violations(krate, tree, PRESENTATION_FORBIDDEN));
        }
        let on_contract = tree
            .iter()
            .any(|e| e.depth == 1 && e.name == "pyreal-client-contract");
        if PRESENTATION_ON_CONTRACT.contains(&krate.as_str()) && !on_contract {
            out.push(format!(
                "{krate} does not depend directly on pyreal-client-contract"
            ));
        }
    }
    dedup(out)
}

/// Rule 9 over the halves' trees (`trees`, one per half, every dependency kind): no direct
/// dependency on any of [`HALVES_FORBIDDEN`], and the scene not on the shell at any depth.
pub fn halves_violations(trees: &[(String, Vec<TreeEntry>)]) -> Vec<String> {
    let mut out = Vec::new();
    for (krate, tree) in trees {
        for e in tree {
            if e.depth == 1 && HALVES_FORBIDDEN.contains(&e.name.as_str()) {
                out.push(format!("{krate} depends directly on `{}`", e.name));
            }
        }
        if krate == "pyreal-scene" {
            out.extend(tree_violations(krate, tree, &["pyreal-client-shell"]));
        }
    }
    dedup(out)
}

fn crate_sources(ws: &Path, krate: &str) -> Vec<(PathBuf, String)> {
    let root = crate::util::crate_dir(ws, krate);
    let mut files = Vec::new();
    for sub in ["src", "tests", "benches", "examples"] {
        files.extend(crate::util::rust_sources(&root.join(sub)));
    }
    files.sort();
    files
        .into_iter()
        .filter_map(|f| {
            let text = std::fs::read_to_string(&f).ok()?;
            let rel = f.strip_prefix(ws).map(Path::to_path_buf).unwrap_or(f);
            Some((rel, text))
        })
        .collect()
}

/// A crate's production sources only (`src/`), for rules its tests are exempt from.
/// Whether `path` (relative to the workspace) is the retail front end's assembly.
fn is_front_end_assembly(path: &Path) -> bool {
    path.to_string_lossy()
        .replace('\\', "/")
        .ends_with(&format!("pyreal/client/crates/shell/{FRONT_END_ASSEMBLY}"))
}

fn crate_src(ws: &Path, krate: &str) -> Vec<(PathBuf, String)> {
    crate_sources(ws, krate)
        .into_iter()
        .filter(|(p, _)| p.components().any(|c| c.as_os_str() == "src"))
        .collect()
}

/// The retired dat-directory variable, spelled in two halves so this file does not name it.
const RETIRED_DAT_VAR: &str = concat!("AC_", "CLIENT_DIR");

/// The retired variable prefix, likewise in two halves: every variable the tests and tools read
/// is `PYREAL_*` (a few also by their old `DERETH_*` names).
const RETIRED_VAR_PREFIX: &str = concat!("DERE", "_");

/// Rule 8's first half: the code (comments stripped) of any workspace file outside `core/dat/`
/// that joins a retail dat file name onto a path. Where the dats are, and what the files are
/// called on disk, is `pyreal-dat`'s business (`locate_modern_dats`, `ModernDat::in_dir`, and
/// `pyreal_dat::testing` for the tests); plain name strings -- a table of file names, a message,
/// a retail string -- are not paths and are not reported.
#[must_use]
pub fn dat_path_violations(files: &[(PathBuf, String)]) -> Vec<String> {
    let join = regex::Regex::new(
        r#"(?:\.join|Path::new|PathBuf::from)\(\s*"client_[A-Za-z0-9_]*\.dat"|format!\(\s*"[^"]*[/\\]client_[A-Za-z0-9_]*\.dat""#,
    )
    .expect("the pattern compiles");
    let mut out = Vec::new();
    for (path, text) in files {
        let norm = path.to_string_lossy().replace('\\', "/");
        if norm.starts_with("core/dat/") || norm.contains("/core/dat/") {
            continue;
        }
        let code: String = text
            .lines()
            .map(strip_comment)
            .collect::<Vec<_>>()
            .join("\n");
        for m in join.find_iter(&code) {
            let line = code[..m.start()].matches('\n').count() + 1;
            out.push(format!(
                "{}:{line}: joins a retail dat file name onto a path (use pyreal_dat::ModernDat or pyreal_dat::testing)",
                path.display()
            ));
        }
    }
    dedup(out)
}

/// Rule 8's second half: any tracked workspace file naming the retired dat-directory
/// variable or a variable with the retired prefix. Comments count: a doc that tells a reader to
/// set a variable nothing reads is as wrong as code that reads it.
#[must_use]
pub fn env_name_violations(files: &[(PathBuf, String)]) -> Vec<String> {
    let names = regex::Regex::new(&format!(
        r"{RETIRED_DAT_VAR}|\b{RETIRED_VAR_PREFIX}[A-Z0-9]"
    ))
    .expect("the pattern compiles");
    let mut out = Vec::new();
    for (path, text) in files {
        for (i, line) in text.lines().enumerate() {
            if let Some(m) = names.find(line) {
                out.push(format!(
                    "{}:{}: names the retired variable `{}...`",
                    path.display(),
                    i + 1,
                    &line[m.start()..m.end()]
                ));
            }
        }
    }
    out
}

/// The files git tracks under `specs` (paths relative to `repo`), as `git ls-files -z` lists them.
///
/// # Errors
/// git could not be run, or `repo` is not a work tree.
fn git_ls_files(repo: &Path, specs: &[&Path]) -> Result<Vec<u8>, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["ls-files", "-z", "--"])
        .args(specs)
        .output()
        .map_err(|e| format!("git ls-files: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git ls-files in {}: {}",
            repo.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(out.stdout)
}

/// The text of every file a `git ls-files -z` listing names, relative to `repo`: a listed file
/// that is not there (deleted in the work tree), larger than 8 MiB or not UTF-8 is skipped, and a
/// file the listing does not name is never read, so an untracked local file is not the tree's.
fn listed_texts(repo: &Path, listing: &[u8]) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    for name in listing.split(|b| *b == 0).filter(|n| !n.is_empty()) {
        let rel = PathBuf::from(String::from_utf8_lossy(name).into_owned());
        let p = repo.join(&rel);
        if !std::fs::metadata(&p).is_ok_and(|m| m.is_file() && m.len() <= 8 << 20) {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&p) {
            out.push((rel, text));
        }
    }
    out
}

/// Every tracked text file under `dirs` (relative to `repo`).
///
/// # Errors
/// As [`git_ls_files`].
fn tracked_texts(repo: &Path, dirs: &[&Path]) -> Result<Vec<(PathBuf, String)>, String> {
    Ok(listed_texts(repo, &git_ls_files(repo, dirs)?))
}

/// Every seam check, as rows for a table.
pub fn reports() -> Vec<Report> {
    let ws = workspace_root();
    let row = |name: &str, result: Result<Vec<String>, String>, ok: String| match result {
        Ok(v) if v.is_empty() => Report::new(name, Outcome::Pass, ok),
        Ok(v) => {
            for line in &v {
                println!("seam: {line}");
            }
            Report::new(
                name,
                Outcome::Fail,
                format!("{} violation(s); see above", v.len()),
            )
        }
        Err(e) => {
            println!("seam: {e}");
            Report::new(name, Outcome::Fail, "could not read the dependency tree")
        }
    };

    let primitives =
        cargo_tree(&ws, "pyreal-primitives", "normal").map(|t| primitives_violations(&t));
    let primitives_code = Ok(primitives_code_violations(&crate_src(
        &ws,
        "pyreal-primitives",
    )));
    let contract =
        cargo_tree(&ws, "pyreal-client-contract", "normal").map(|t| contract_violations(&t));
    let ui_logic_tree = cargo_tree(&ws, "pyreal-ui-logic", "normal,build,dev")
        .map(|t| tree_violations("pyreal-ui-logic", &t, UI_LOGIC_FORBIDDEN));
    let ui_logic_code = Ok(code_violations(
        &crate_sources(&ws, "pyreal-ui-logic"),
        &[
            "pyreal_client_model",
            "pyreal_client_runtime",
            "pyreal_client",
        ],
    ));
    let crates = client_crates(&ws);
    let core_tree =
        cargo_tree(&ws, "pyreal-client-runtime", "normal").map(|t| core_violations(&t, &crates));
    let core_code = Ok(code_violations(
        &crate_sources(&ws, "pyreal-client-runtime"),
        CORE_NAMES_NOT,
    ));
    let presentation = if crates.len() < PRESENTATION_ON_CONTRACT.len() {
        Err(format!(
            "found {} crate(s) under pyreal/client/crates/; expected at least {}",
            crates.len(),
            PRESENTATION_ON_CONTRACT.len()
        ))
    } else {
        crates
            .iter()
            .map(|k| cargo_tree(&ws, k, "normal,build,dev").map(|t| (k.clone(), t)))
            .collect::<Result<Vec<_>, _>>()
            .map(|trees| presentation_violations(&trees))
    };
    let plugin_api = cargo_tree(&ws, "pyreal-plugin-api", "normal")
        .map(|t| plugin_layer_violations("pyreal-plugin-api", &t, PLUGIN_API_DIRECT));
    let plugin_package = cargo_tree(&ws, "pyreal-plugin-package", "normal")
        .map(|t| plugin_layer_violations("pyreal-plugin-package", &t, PLUGIN_PACKAGE_DIRECT));
    let plugin_wasm = cargo_tree(&ws, "pyreal-plugin-wasm", "normal")
        .map(|t| plugin_layer_violations("pyreal-plugin-wasm", &t, PLUGIN_WASM_DIRECT));
    let plugin_lua = cargo_tree(&ws, "pyreal-plugin-lua", "normal")
        .map(|t| plugin_layer_violations("pyreal-plugin-lua", &t, PLUGIN_LUA_DIRECT));
    let plugin_host = cargo_tree(&ws, "pyreal-plugin-host", "normal").map(|t| {
        let mut v = plugin_host_violations(&t, &crates);
        v.extend(code_violations(
            &crate_sources(&ws, "pyreal-plugin-host"),
            CORE_NAMES_NOT,
        ));
        v
    });
    let action_seam_files: Vec<(PathBuf, String)> = ACTION_SEAM_CRATES
        .iter()
        .flat_map(|k| crate_sources(&ws, k))
        .collect();
    // A crate whose sources were not found would pass vacuously; each must contribute files.
    let action_seam_code = match ACTION_SEAM_CRATES
        .iter()
        .find(|k| crate_sources(&ws, k).is_empty())
    {
        Some(k) => Err(format!("no sources found for {k}")),
        None => Ok(code_violations(&action_seam_files, DEVICE_NAMES_NOT)),
    };
    // Every library crate logs through the facade and installs nothing: core/*, and the client's
    // own crates under pyreal/client/crates/.
    let libraries: Vec<String> = crates_under(&ws, "core")
        .into_iter()
        .chain(crates_under(&ws, "pyreal/client/crates"))
        .collect();
    let library_log = if libraries.len() < 16 {
        Err(format!(
            "found {} library crate(s) under core/ and pyreal/client/crates/; expected at least 16",
            libraries.len()
        ))
    } else {
        libraries
            .iter()
            .map(|k| cargo_tree(&ws, k, "normal").map(|t| (k.clone(), t)))
            .collect::<Result<Vec<_>, _>>()
            .map(|trees| library_log_violations(&trees))
    };
    // Rule 9: the application's halves reach no platform.
    let halves_tree = APPLICATION_HALVES
        .iter()
        .map(|k| cargo_tree(&ws, k, "normal,build,dev").map(|t| ((*k).to_owned(), t)))
        .collect::<Result<Vec<_>, _>>()
        .map(|trees| halves_violations(&trees));
    let halves_code = match APPLICATION_HALVES
        .iter()
        .find(|k| crate_sources(&ws, k).is_empty())
    {
        Some(k) => Err(format!("no sources found for {k}")),
        None => Ok(path_violations(
            &APPLICATION_HALVES
                .iter()
                .flat_map(|k| crate_sources(&ws, k))
                .collect::<Vec<_>>(),
            HALVES_NAMES_NOT,
        )),
    };
    // Rule 10: every front end reaches the game through its context.
    let front_end = FRONT_ENDS
        .iter()
        .map(|krate| {
            let files: Vec<(PathBuf, String)> = crate_src(&ws, krate)
                .into_iter()
                .filter(|(p, _)| !is_front_end_assembly(p))
                .collect();
            if files.is_empty() {
                Err(format!("no sources found for {krate}"))
            } else {
                Ok(code_violations(&files, FRONT_END_NAMES_NOT))
            }
        })
        .try_fold(Vec::new(), |mut all, one| {
            all.extend(one?);
            Ok::<_, String>(all)
        });
    // Rule 8: every Rust file of the workspace, and every tracked text file in it.
    let workspace_rs: Vec<(PathBuf, String)> = crate::util::rust_sources(&ws)
        .into_iter()
        .filter_map(|p| {
            let text = std::fs::read_to_string(&p).ok()?;
            Some((
                p.strip_prefix(&ws).map(Path::to_path_buf).unwrap_or(p),
                text,
            ))
        })
        .collect();
    let dat_paths = if workspace_rs.len() < 100 {
        Err(format!(
            "found {} Rust file(s) in the workspace; expected hundreds",
            workspace_rs.len()
        ))
    } else {
        Ok(dat_path_violations(&workspace_rs))
    };
    // Tracked files only: what the tree says, not what a working copy happens to hold (a local,
    // untracked configuration file may name anything).
    let env_names = tracked_texts(&ws, &[Path::new(".")]).map(|t| env_name_violations(&t));
    // Rule 11: the high-fidelity presentation changes only pixels.
    vec![
        row(
            "seam: primitives deps",
            primitives,
            format!(
                "direct deps within {}; no pyreal-* or platform crate",
                PRIMITIVES_DIRECT.join(", ")
            ),
        ),
        row(
            "seam: primitives code",
            primitives_code,
            format!("no std I/O; unsafe only in {PRIMITIVES_UNSAFE_FILE}, behind host-nls"),
        ),
        row(
            "seam: pyreal-client-contract deps",
            contract,
            format!(
                "direct deps within {}; no other pyreal-*",
                CONTRACT_DIRECT.join(", ")
            ),
        ),
        row(
            "seam: ui-logic deps",
            ui_logic_tree,
            "no pyreal-client-model / pyreal-client-runtime / pyreal-client in any table"
                .to_owned(),
        ),
        row(
            "seam: ui-logic code",
            ui_logic_code,
            "names no pyreal_client_model / pyreal_client_runtime / pyreal_client".to_owned(),
        ),
        row(
            "seam: client-runtime deps",
            core_tree,
            format!(
                "nothing under pyreal/client/crates/; none of {}",
                CORE_FORBIDDEN.join(", ")
            ),
        ),
        row(
            "seam: client-runtime code",
            core_code,
            format!("names none of {}", CORE_NAMES_NOT.join(" / ")),
        ),
        row(
            "seam: plugin-api deps",
            plugin_api,
            format!(
                "direct deps within {}; no other pyreal-* or platform crate",
                PLUGIN_API_DIRECT.join(", ")
            ),
        ),
        row(
            "seam: plugin-package deps",
            plugin_package,
            format!(
                "direct deps within {}; no other pyreal-* or platform crate",
                PLUGIN_PACKAGE_DIRECT.join(", ")
            ),
        ),
        row(
            "seam: plugin-wasm deps",
            plugin_wasm,
            format!(
                "direct deps within {}; no other pyreal-* or platform crate",
                PLUGIN_WASM_DIRECT.join(", ")
            ),
        ),
        row(
            "seam: plugin-lua deps",
            plugin_lua,
            format!(
                "direct deps within {}; no other pyreal-* or platform crate",
                PLUGIN_LUA_DIRECT.join(", ")
            ),
        ),
        row(
            "seam: plugin-host names no client crate",
            plugin_host,
            "nothing under pyreal/client/ in its tree or its code".to_owned(),
        ),
        row(
            "seam: action seam code",
            action_seam_code,
            format!(
                "{} name no key, key map, input map or device message",
                ACTION_SEAM_CRATES.join(", ")
            ),
        ),
        row(
            "seam: client crates",
            presentation,
            format!(
                "pyreal/client/crates/* reach no pyreal-client-runtime; {} depend on the contract",
                PRESENTATION_ON_CONTRACT.join(", ")
            ),
        ),
        row(
            "seam: library logging",
            library_log,
            format!(
                "core/* and pyreal/client/crates/* reach none of {} (a binary installs it)",
                LIBRARY_NO_SUBSCRIBER.join(", ")
            ),
        ),
        row(
            "seam: application halves deps",
            halves_tree,
            format!(
                "{} depend directly on none of {}; the scene not on the shell",
                APPLICATION_HALVES.join(", "),
                HALVES_FORBIDDEN.join(", ")
            ),
        ),
        row(
            "seam: application halves code",
            halves_code,
            format!(
                "{} name none of {}",
                APPLICATION_HALVES.join(", "),
                HALVES_NAMES_NOT.join(" / ")
            ),
        ),
        row(
            "seam: front end -> context",
            front_end,
            format!(
                "{} src but {FRONT_END_ASSEMBLY} names none of {}",
                FRONT_ENDS.join(", "),
                FRONT_END_NAMES_NOT.join(", ")
            ),
        ),
        row(
            "seam: dat paths",
            dat_paths,
            "only pyreal-dat joins a retail dat file name onto a path".to_owned(),
        ),
        row(
            "seam: retired variables",
            env_names,
            "the workspace names no retired environment variable".to_owned(),
        ),
    ]
}

/// `cargo xtask seams`.
pub fn seams() -> i32 {
    print_table("xtask seams", &reports())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ORACLE: none needed -- these calibrate the checker against planted violations, because a
    // seam check that silently stops firing is worse than none.

    fn tree(rows: &[(usize, &str)]) -> Vec<TreeEntry> {
        rows.iter()
            .map(|&(depth, name)| TreeEntry {
                depth,
                name: name.to_owned(),
            })
            .collect()
    }

    #[test]
    fn client_crates_reject_the_runtime_and_require_the_contract() {
        let clean = vec![
            (
                "pyreal-ui-logic".to_owned(),
                tree(&[
                    (0, "pyreal-ui-logic"),
                    (1, "pyreal-client-contract"),
                    (2, "pyreal-primitives"),
                ]),
            ),
            (
                "pyreal-clipboard".to_owned(),
                tree(&[(0, "pyreal-clipboard")]),
            ),
        ];
        assert!(presentation_violations(&clean).is_empty());

        let runtime = vec![(
            "pyreal-engine".to_owned(),
            tree(&[
                (0, "pyreal-engine"),
                (1, "pyreal-client-contract"),
                (1, "pyreal-x"),
                (2, "pyreal-client-runtime"),
            ]),
        )];
        assert_eq!(presentation_violations(&runtime).len(), 1);

        let no_contract = vec![(
            "pyreal-ui-logic".to_owned(),
            tree(&[(0, "pyreal-ui-logic"), (1, "pyreal-primitives")]),
        )];
        assert_eq!(presentation_violations(&no_contract).len(), 1);
    }

    #[test]
    fn the_application_halves_sit_on_the_runtime_and_reach_no_platform() {
        let on_runtime = vec![(
            "pyreal-scene".to_owned(),
            tree(&[
                (0, "pyreal-scene"),
                (1, "pyreal-client-runtime"),
                (1, "pyreal-engine"),
                (2, "windows"),
            ]),
        )];
        assert!(presentation_violations(&on_runtime).is_empty());
        assert!(halves_violations(&on_runtime).is_empty());

        let platform = vec![(
            "pyreal-client-shell".to_owned(),
            tree(&[
                (0, "pyreal-client-shell"),
                (1, "winit"),
                (1, "pyreal-clipboard"),
            ]),
        )];
        assert_eq!(halves_violations(&platform).len(), 2);

        let upward = vec![(
            "pyreal-scene".to_owned(),
            tree(&[
                (0, "pyreal-scene"),
                (1, "pyreal-x"),
                (2, "pyreal-client-shell"),
            ]),
        )];
        assert_eq!(halves_violations(&upward).len(), 1);

        let code = vec![(
            PathBuf::from("src/a.rs"),
            "#[cfg(windows)]\nlet n = layout.windows.len();\nuse winit::event;\nlet h = windows::core::HSTRING::new();\n".to_owned(),
        )];
        assert_eq!(path_violations(&code, HALVES_NAMES_NOT).len(), 2);
    }

    #[test]
    fn a_front_end_names_neither_the_application_nor_its_internals() {
        let code = vec![(
            PathBuf::from("src/front_end.rs"),
            "fn step(cx: &mut Cx<'_, H>) {}\n\
             fn bad(core: &mut CoreApp<H>) {}\n\
             let i: &mut crate::interaction::Interaction = x;\n\
             use crate::interaction::TargetMode;\n\
             // the App is assembled elsewhere\n\
             let apply = AppState::default();\n"
                .to_owned(),
        )];
        assert_eq!(code_violations(&code, FRONT_END_NAMES_NOT).len(), 2);
        assert!(is_front_end_assembly(Path::new(
            "pyreal\\client\\crates\\shell\\src\\app.rs"
        )));
        assert!(!is_front_end_assembly(Path::new(
            "pyreal/client/crates/shell/src/front_end.rs"
        )));
    }

    #[test]
    fn the_action_seam_names_no_device_but_may_describe_one() {
        let clean = vec![(
            PathBuf::from("core/client-runtime/src/a.rs"),
            "//! The front end's input layer reads `WM_KEYDOWN`; this crate takes an `Action`.
             let a = Action::begin(ActionId(0x29)); // not an InputEvent
             let m = WindowMessage::new(msg::WM_ACTIVATEAPP, 1, 0);
             let n = input_events_seen; let k = keymap_file_name;
"
            .to_owned(),
        )];
        assert!(
            code_violations(&clean, DEVICE_NAMES_NOT).is_empty(),
            "{:?}",
            code_violations(&clean, DEVICE_NAMES_NOT)
        );
        let planted = vec![(
            PathBuf::from("core/client-runtime/src/b.rs"),
            "pub use pyreal_input as input;
             fn f(e: &InputEvent) {}
             let m = Win32Message::new(msg::WM_KEYDOWN, 0x57, 0, 0);
             if key.scan_code == 0x11 {}
"
            .to_owned(),
        )];
        let hits = code_violations(&planted, DEVICE_NAMES_NOT);
        assert_eq!(hits.len(), 5, "{hits:?}");
    }

    #[test]
    fn a_library_may_emit_events_but_may_not_carry_a_subscriber() {
        let facade = tree(&[
            (0, "pyreal-world-data"),
            (1, "tracing"),
            (2, "tracing-core"),
        ]);
        assert!(library_log_violations(&[("pyreal-world-data".to_owned(), facade)]).is_empty());
        let planted = tree(&[
            (0, "pyreal-audio"),
            (1, "pyreal-primitives"),
            (2, "tracing-subscriber"),
        ]);
        let v = library_log_violations(&[("pyreal-audio".to_owned(), planted)]);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].contains("tracing-subscriber"), "{v:?}");
    }

    #[test]
    fn the_plugin_crates_keep_to_their_layers() {
        let api = tree(&[
            (0, "pyreal-plugin-api"),
            (1, "pyreal-primitives"),
            (2, "thiserror"),
        ]);
        assert!(plugin_layer_violations("pyreal-plugin-api", &api, PLUGIN_API_DIRECT).is_empty());
        let planted = tree(&[
            (0, "pyreal-plugin-api"),
            (1, "pyreal-primitives"),
            (1, "serde"),
        ]);
        assert_eq!(
            plugin_layer_violations("pyreal-plugin-api", &planted, PLUGIN_API_DIRECT).len(),
            1
        );
        let lua = tree(&[
            (0, "pyreal-plugin-lua"),
            (1, "pyreal-plugin-api"),
            (2, "pyreal-primitives"),
            (1, "mlua"),
        ]);
        assert!(plugin_layer_violations("pyreal-plugin-lua", &lua, PLUGIN_LUA_DIRECT).is_empty());
        let reaching = tree(&[
            (0, "pyreal-plugin-lua"),
            (1, "pyreal-plugin-api"),
            (2, "pyreal-client-model"),
        ]);
        assert_eq!(
            plugin_layer_violations("pyreal-plugin-lua", &reaching, PLUGIN_LUA_DIRECT).len(),
            1
        );
        let host = tree(&[
            (0, "pyreal-plugin-host"),
            (1, "pyreal-client-runtime"),
            (2, "pyreal-ui-kit"),
        ]);
        assert_eq!(
            plugin_host_violations(&host, &["pyreal-ui-kit".to_owned()]).len(),
            1
        );
    }

    #[test]
    fn parses_depth_prefixed_tree_lines() {
        let text =
            "0pyreal-client-contract v0.0.0 (C:\\x)\n1pyreal-primitives v0.0.0 (C:\\y)\n2thiserror v2.0.20\n\
                    1glam v0.30.10\n\n";
        assert_eq!(
            parse_tree(text),
            tree(&[
                (0, "pyreal-client-contract"),
                (1, "pyreal-primitives"),
                (2, "thiserror"),
                (1, "glam")
            ])
        );
    }

    #[test]
    fn contract_allows_its_list_and_rejects_a_planted_dependency() {
        let clean = tree(&[
            (0, "pyreal-client-contract"),
            (1, "pyreal-primitives"),
            (2, "thiserror"),
            (2, "pxfm"),
            (1, "glam"),
            (1, "raw-window-handle"),
        ]);
        assert!(contract_violations(&clean).is_empty());

        let direct = tree(&[(0, "pyreal-client-contract"), (1, "serde")]);
        assert_eq!(contract_violations(&direct).len(), 1);

        let deep = tree(&[
            (0, "pyreal-client-contract"),
            (1, "pyreal-primitives"),
            (2, "pyreal-ui-kit"),
        ]);
        assert_eq!(contract_violations(&deep).len(), 1);

        let platform = tree(&[(0, "pyreal-client-contract"), (1, "glam"), (2, "winit")]);
        assert_eq!(contract_violations(&platform).len(), 1);
    }

    #[test]
    fn primitives_allow_their_list_and_reject_a_planted_dependency() {
        let clean = tree(&[
            (0, "pyreal-primitives"),
            (1, "libm"),
            (1, "pxfm"),
            (1, "thiserror"),
            (2, "thiserror-impl"),
        ]);
        assert!(primitives_violations(&clean).is_empty());
        let direct = tree(&[(0, "pyreal-primitives"), (1, "glam")]);
        assert_eq!(primitives_violations(&direct).len(), 1);
        let dere = tree(&[
            (0, "pyreal-primitives"),
            (1, "thiserror"),
            (2, "pyreal-dat"),
        ]);
        assert_eq!(primitives_violations(&dere).len(), 1);
        let platform = tree(&[(0, "pyreal-primitives"), (1, "pxfm"), (2, "windows-sys")]);
        assert_eq!(primitives_violations(&platform).len(), 1);
    }

    #[test]
    fn primitives_code_allows_unsafe_in_the_nls_arm_only_and_no_io_anywhere() {
        let decl = PRIMITIVES_UNSAFE_DECL.to_owned() + "\npub mod cp1252;\n";
        let clean = vec![
            (PathBuf::from("core/primitives/src/text/mod.rs"), decl.clone()),
            (
                PathBuf::from("core/primitives/src/text/windows.rs"),
                "extern \"system\" { fn GetACP() -> u32; }\npub fn acp() -> u32 { unsafe { GetACP() } }\n"
                    .to_owned(),
            ),
            (
                PathBuf::from("core/primitives/src/asset.rs"),
                "//! No std::fs here.\npub enum E { Io { source: std::io::Error } }\n".to_owned(),
            ),
        ];
        assert!(
            primitives_code_violations(&clean).is_empty(),
            "{:?}",
            primitives_code_violations(&clean)
        );

        let io = vec![(
            PathBuf::from("core/primitives/src/shape.rs"),
            "let f = std::fs::read(p);\n".to_owned(),
        )];
        assert_eq!(primitives_code_violations(&io).len(), 1);
        let stray = vec![(
            PathBuf::from("core/primitives/src/num/mod.rs"),
            "#[allow(unsafe_code)]\nfn f() { unsafe { g() } }\n".to_owned(),
        )];
        assert_eq!(primitives_code_violations(&stray).len(), 2);
        let ungated = vec![(
            PathBuf::from("core/primitives/src/text/mod.rs"),
            "#[allow(unsafe_code)]\nmod windows;\n".to_owned(),
        )];
        assert_eq!(primitives_code_violations(&ungated).len(), 1);
    }

    #[test]
    fn a_forbidden_name_anywhere_in_the_tree_is_a_violation() {
        let t = tree(&[
            (0, "pyreal-ui-logic"),
            (1, "pyreal-rules"),
            (2, "pyreal-client-model"),
        ]);
        assert_eq!(
            tree_violations("pyreal-ui-logic", &t, UI_LOGIC_FORBIDDEN).len(),
            1
        );
        let t = tree(&[
            (0, "pyreal-client-runtime"),
            (1, "pyreal-audio"),
            (2, "cpal"),
        ]);
        assert_eq!(
            tree_violations("pyreal-client-runtime", &t, CORE_FORBIDDEN).len(),
            1
        );
        let t = tree(&[(0, "pyreal-client-runtime"), (1, "windows-sys")]);
        assert!(tree_violations("pyreal-client-runtime", &t, CORE_FORBIDDEN).is_empty());
    }

    #[test]
    fn the_runtime_reaches_no_client_crate_and_no_platform() {
        let crates = vec![
            "pyreal-world-render".to_owned(),
            "pyreal-new-panel".to_owned(),
        ];
        let clean = tree(&[
            (0, "pyreal-client-runtime"),
            (1, "pyreal-landscape"),
            (1, "pyreal-physics"),
            (2, "pyreal-primitives"),
        ]);
        assert!(core_violations(&clean, &crates).is_empty());
        let drawing = tree(&[
            (0, "pyreal-client-runtime"),
            (1, "pyreal-landscape"),
            (2, "pyreal-world-render"),
        ]);
        assert_eq!(core_violations(&drawing, &crates).len(), 1);
        // A crate under pyreal/client/crates/ that the fixed list does not name is still caught.
        let unnamed = tree(&[(0, "pyreal-client-runtime"), (1, "pyreal-new-panel")]);
        assert_eq!(core_violations(&unnamed, &crates).len(), 1);
        let product = tree(&[(0, "pyreal-client-runtime"), (1, "pyreal-client")]);
        assert_eq!(core_violations(&product, &crates).len(), 1);
    }

    #[test]
    fn code_names_count_but_comments_and_longer_identifiers_do_not() {
        let files = vec![(
            PathBuf::from("a.rs"),
            "//! `pyreal_client_model::chat` is mentioned in a doc\n\
             let x = 1; // pyreal_client_model::World\n\
             use pyreal_ui_logic::panels;\n\
             use pyreal_client_model::World;\n"
                .to_owned(),
        )];
        let hits = code_violations(&files, &["pyreal_client_model", "dereth_ui"]);
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert!(hits[0].starts_with("a.rs:4:"));
        assert_eq!(code_violations(&files, &["pyreal_ui_logic"]).len(), 1);
        assert!(code_violations(&files, &["pyreal_client"]).is_empty());
    }

    #[test]
    fn only_pyreal_dat_joins_a_retail_file_name_onto_a_path() {
        let planted = vec![
            (
                PathBuf::from("pyreal/client/tests/dat/a.rs"),
                "let ok = dir\n    .join(\n        \"client_portal.dat\",\n    )\n    .exists();\n"
                    .to_owned(),
            ),
            (
                PathBuf::from("core/client-runtime/src/b.rs"),
                "let p = format!(\"{}/client_cell_1.dat\", dir);\n".to_owned(),
            ),
        ];
        let hits = dat_path_violations(&planted);
        assert_eq!(hits.len(), 2, "{hits:?}");
        assert!(
            hits[0].starts_with("core/client-runtime/src/b.rs:1:"),
            "{hits:?}"
        );
        assert!(
            hits[1].starts_with("pyreal/client/tests/dat/a.rs:2:"),
            "{hits:?}"
        );

        let clean = vec![
            (
                PathBuf::from("core/dat/src/locate.rs"),
                "dir.join(\"client_portal.dat\")".to_owned(),
            ),
            (
                PathBuf::from("core/client-runtime/src/ddd.rs"),
                "Self::Portal => \"client_portal.dat\",\n// dir.join(\"client_portal.dat\")\n\
                 let d = display_string(ID, &[\"client_local_English.dat\"]);\n"
                    .to_owned(),
            ),
        ];
        assert!(dat_path_violations(&clean).is_empty());
    }

    /// Calibration: in a real git work tree, a tracked file naming a retired variable is found
    /// and an untracked one beside it naming another is not.
    #[test]
    fn only_tracked_files_are_read_for_retired_variables() {
        let repo = std::env::temp_dir().join(format!("xtask-seams-tracked-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(repo.join("docs")).expect("temp repo");
        let git = |args: &[&str]| {
            let ok = Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(args)
                .status()
                .expect("git runs")
                .success();
            assert!(ok, "git {args:?}");
        };
        git(&["init", "-q"]);
        let tracked = format!("{RETIRED_VAR_PREFIX}CAPTURES\n");
        let untracked = format!("{RETIRED_VAR_PREFIX}GOLDEN\n");
        std::fs::write(repo.join("docs/tracked.toml"), &tracked).expect("write");
        std::fs::write(repo.join("docs/local.toml"), &untracked).expect("write");
        git(&["add", "docs/tracked.toml"]);
        let texts = tracked_texts(&repo, &[Path::new("docs"), Path::new("tools")]);
        std::fs::remove_dir_all(&repo).ok();
        let hits = env_name_violations(&texts.expect("a work tree"));
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert!(hits[0].starts_with("docs/tracked.toml:1:"), "{hits:?}");
        // The same two files read without the listing would both be found.
        let both = vec![
            (PathBuf::from("docs/tracked.toml"), tracked),
            (PathBuf::from("docs/local.toml"), untracked),
        ];
        assert_eq!(env_name_violations(&both).len(), 2);
    }

    #[test]
    fn a_retired_variable_name_is_found_in_code_and_in_prose() {
        let old = RETIRED_DAT_VAR;
        let prefixed = format!("{RETIRED_VAR_PREFIX}CAPTURES");
        let planted = vec![
            (
                PathBuf::from("tools/x.py"),
                format!("os.environ.get(\"{prefixed}\")\n"),
            ),
            (
                PathBuf::from("docs/README.md"),
                format!("line one\npoint `${old}` at the install\n"),
            ),
        ];
        let hits = env_name_violations(&planted);
        assert_eq!(hits.len(), 2, "{hits:?}");
        assert!(hits[1].starts_with("docs/README.md:2:"), "{hits:?}");
        let clean = vec![(
            PathBuf::from("tools/y.py"),
            "PYREAL_CAPTURES PYREAL_TEST_DAT_DIR dere_fixtures\n".to_owned(),
        )];
        assert!(env_name_violations(&clean).is_empty());
    }
}
