//! The build hash: the short commit `build.py` names its `dist/` folder by, which the client's
//! build script compiles in and `cargo xtask package` hands on. One rule for both, kept here.

/// The variable the hash is handed on in: `build.py` sets it, the packaging passes it on, and
/// the client's build script reads it before asking git.
pub const ENV: &str = "PYREAL_BUILD_HASH";

/// The `git` arguments that print the hash, as `build.py` runs them.
pub const GIT_ARGS: [&str; 3] = ["rev-parse", "--short=8", "HEAD"];

/// The hash: `given` when the build was handed one, else what `git` printed, else none. Only a
/// run of hex digits counts, so an empty variable or a failed git leaves the build without one.
pub fn pick(given: Option<&str>, git: impl FnOnce() -> Option<String>) -> Option<String> {
    let clean = |s: &str| {
        let s = s.trim();
        (!s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit())).then(|| s.to_ascii_lowercase())
    };
    given
        .and_then(clean)
        .or_else(|| git().as_deref().and_then(clean))
}
