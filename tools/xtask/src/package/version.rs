//! The release version Pyreal's client carries, and the release tags that name it.

use std::path::{Path, PathBuf};

/// The `[package] version` of one manifest: its literal, or `workspace` for `version.workspace = true`.
pub fn package_version(manifest: &str, workspace: Option<&str>) -> Option<String> {
    let table: toml::Table = toml::from_str(manifest).ok()?;
    let version = table.get("package")?.get("version")?;
    if let Some(v) = version.as_str() {
        return Some(v.to_owned());
    }
    let inherited = version.get("workspace").and_then(toml::Value::as_bool) == Some(true);
    inherited.then(|| workspace.map(str::to_owned)).flatten()
}

/// The `[workspace.package] version` of the root manifest's text.
pub fn workspace_version(root_manifest: &str) -> Option<String> {
    let table: toml::Table = toml::from_str(root_manifest).ok()?;
    table
        .get("workspace")?
        .get("package")?
        .get("version")?
        .as_str()
        .map(str::to_owned)
}

/// The one version every manifest in `versions` carries, with `what` naming the manifests in the
/// error.
pub fn shared_version_of(
    what: &str,
    versions: &[(String, Option<String>)],
) -> Result<String, String> {
    let first = versions
        .first()
        .ok_or_else(|| format!("no {what} manifests were read"))?;
    let Some(want) = &first.1 else {
        return Err(format!("{} has no [package] version", first.0));
    };
    let differing: Vec<String> = versions
        .iter()
        .filter(|(_, v)| v.as_ref() != Some(want))
        .map(|(name, v)| format!("{name}: {}", v.as_deref().unwrap_or("(none)")))
        .collect();
    if differing.is_empty() {
        Ok(want.clone())
    } else {
        Err(format!(
            "every {what} must carry the same version ({want} in {}); these differ: {}",
            first.0,
            differing.join(", ")
        ))
    }
}

/// The version every manifest in `manifests` carries, required to agree.
fn manifests_version(ws: &Path, manifests: &[PathBuf], what: &str) -> Result<String, String> {
    let root = ws.join("Cargo.toml");
    let root =
        std::fs::read_to_string(&root).map_err(|e| format!("reading {}: {e}", root.display()))?;
    let workspace = workspace_version(&root);
    let mut versions = Vec::new();
    for manifest in manifests {
        let text = std::fs::read_to_string(manifest)
            .map_err(|e| format!("reading {}: {e}", manifest.display()))?;
        let name = manifest
            .strip_prefix(ws)
            .unwrap_or(manifest)
            .display()
            .to_string()
            .replace('\\', "/");
        versions.push((name, package_version(&text, workspace.as_deref())));
    }
    shared_version_of(what, &versions)
}

/// The prefix of a Pyreal release tag: `v<version>`.
pub const PYREAL_TAG_PREFIX: &str = "v";

/// The crates that carry Pyreal's version: the client.
pub const PYREAL_MANIFESTS: &[&str] = &["pyreal/client/Cargo.toml"];

/// The manifests of the crates that carry Pyreal's version.
pub fn pyreal_manifests(ws: &Path) -> Vec<PathBuf> {
    PYREAL_MANIFESTS.iter().map(|m| ws.join(m)).collect()
}

/// Pyreal's version, read from its crates' manifests and required to agree.
pub fn pyreal_version(ws: &Path) -> Result<String, String> {
    manifests_version(ws, &pyreal_manifests(ws), "Pyreal crate (the client)")
}

/// A release version: `MAJOR.MINOR.PATCH`, optionally followed by a pre-release suffix
/// (`-rc.1`: dot-separated identifiers of letters, digits and hyphens). No build suffix (`+...`).
/// Returns the numeric core.
pub fn parse_release_version(v: &str) -> Result<[u64; 3], String> {
    let bad = || {
        format!(
            "`{v}` is not a release version: MAJOR.MINOR.PATCH, optionally with a pre-release suffix \
             such as `-rc.1` (no build suffix)"
        )
    };
    let (core, pre) = match v.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (v, None),
    };
    if let Some(pre) = pre {
        let ok = !pre.is_empty()
            && pre.split('.').all(|id| {
                !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            });
        if !ok {
            return Err(bad());
        }
    }
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 {
        return Err(bad());
    }
    let mut out = [0u64; 3];
    for (slot, part) in out.iter_mut().zip(&parts) {
        if part.is_empty()
            || !part.bytes().all(|b| b.is_ascii_digit())
            || (part.len() > 1 && part.starts_with('0'))
        {
            return Err(bad());
        }
        *slot = part.parse().map_err(|_| bad())?;
    }
    Ok(out)
}

/// Whether a release version is a pre-release (it carries a `-...` suffix).
pub fn is_prerelease(v: &str) -> bool {
    v.contains('-')
}

/// A product with releases of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Product {
    /// Pyreal, the client, released from `v*` tags.
    Pyreal,
}

impl Product {
    /// The prefix of the product's release tags.
    pub fn tag_prefix(self) -> &'static str {
        match self {
            Self::Pyreal => PYREAL_TAG_PREFIX,
        }
    }

    /// The product's release tag for `version`.
    pub fn tag_for(self, version: &str) -> String {
        format!("{}{version}", self.tag_prefix())
    }

    /// The product's version in the tree at `ws`.
    pub fn version(self, ws: &Path) -> Result<String, String> {
        match self {
            Self::Pyreal => pyreal_version(ws),
        }
    }
}
