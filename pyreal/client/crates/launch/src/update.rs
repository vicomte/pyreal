//! Whether a newer Pyreal is out: the latest release's `release.json` read from the website or
//! the repository ([`Feed`]), compared by SemVer's precedence ([`is_offered`]), and the answer
//! kept a day in the settings folder ([`UpdateCache`]). The client reads the file off
//! the frame thread; nothing here touches the network.

use std::io;
use std::path::Path;

use pyreal_plugin_api::manifest::PluginVersion;
use serde::{Deserialize, Serialize};

/// The file the last answer is kept in, in the settings folder.
pub const CACHE_FILE: &str = "update-check.json";

/// How long an answer stands before the release is read again: a day.
pub const MAX_AGE_SECS: u64 = 24 * 60 * 60;

/// The most of a `release.json` that is read.
pub const MAX_RELEASE_BYTES: usize = 256 * 1024;

/// The address of the latest release's `release.json` in the repository at `source`.
#[must_use]
pub fn release_json_url(source: &str) -> String {
    format!(
        "{}/releases/latest/download/release.json",
        source.trim_end_matches('/')
    )
}

/// A place the latest release is read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Feed {
    /// The website's copy of the latest `release.json`, at this address.
    Site(String),
    /// The latest release of the repository at this address.
    Repo(String),
}

impl Feed {
    /// The feeds a build reads, in order: its website's `release.json`, then its repository's.
    #[must_use]
    pub fn list(site: Option<&str>, repo: Option<&str>) -> Vec<Self> {
        fn named(u: Option<&str>) -> Option<&str> {
            u.map(str::trim).filter(|u| !u.is_empty())
        }
        let mut feeds = Vec::new();
        if let Some(site) = named(site) {
            feeds.push(Self::Site(site.to_owned()));
        }
        if let Some(repo) = named(repo) {
            feeds.push(Self::Repo(repo.trim_end_matches('/').to_owned()));
        }
        feeds
    }

    /// The address of the `release.json` read.
    #[must_use]
    pub fn url(&self) -> String {
        match self {
            Self::Site(url) => url.clone(),
            Self::Repo(repo) => release_json_url(repo),
        }
    }

    /// The release the `release.json` read from here describes.
    ///
    /// # Errors
    /// Text that is not a Pyreal `release.json` with a SemVer version.
    pub fn parse(&self, bytes: &[u8]) -> Result<Release, String> {
        match self {
            Self::Site(url) => parse_site_release(bytes, url),
            Self::Repo(repo) => parse_release(bytes, repo),
        }
    }
}

/// The latest release, as its `release.json` names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub version: String,
    /// The page the notice opens: the website's, or the release's in the repository.
    pub url: String,
    pub prerelease: bool,
}

/// The fields of `release.json` the check reads.
#[derive(Deserialize)]
struct Index {
    product: Option<String>,
    version: String,
    tag: Option<String>,
    #[serde(default)]
    prerelease: bool,
    /// The website's page for the release; only the website's copy names one.
    page: Option<String>,
}

impl Index {
    /// The index in `bytes`, when it is Pyreal's with a SemVer version.
    fn read(bytes: &[u8]) -> Result<(Self, bool), String> {
        let index: Self =
            serde_json::from_slice(bytes).map_err(|e| format!("not a release index: {e}"))?;
        if index.product.as_deref().is_some_and(|p| p != "pyreal") {
            return Err(format!("a release of {:?}", index.product));
        }
        let version = PluginVersion::parse(&index.version)
            .ok_or_else(|| format!("{:?} is not a version", index.version))?;
        let prerelease = index.prerelease || !version.pre().is_empty();
        Ok((index, prerelease))
    }
}

/// The release a `release.json` from the repository at `source` describes.
///
/// # Errors
/// Text that is not a Pyreal `release.json` with a SemVer version.
pub fn parse_release(bytes: &[u8], source: &str) -> Result<Release, String> {
    let (index, prerelease) = Index::read(bytes)?;
    let tag = index
        .tag
        .filter(|t| {
            !t.is_empty()
                && t.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-+".contains(&b))
        })
        .unwrap_or_else(|| format!("v{}", index.version));
    Ok(Release {
        url: format!("{}/releases/tag/{tag}", source.trim_end_matches('/')),
        prerelease,
        version: index.version,
    })
}

/// The release the website's `release.json` at `url` describes. Its page is the `page` it names
/// on the same site, else the site's front page.
///
/// # Errors
/// Text that is not a Pyreal `release.json` with a SemVer version.
pub fn parse_site_release(bytes: &[u8], url: &str) -> Result<Release, String> {
    let (index, prerelease) = Index::read(bytes)?;
    let site = origin(url);
    let page = index
        .page
        .filter(|p| {
            p.strip_prefix(site)
                .is_some_and(|rest| rest.starts_with('/'))
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b":/._-~#?=%".contains(&b))
        })
        .unwrap_or_else(|| format!("{site}/"));
    Ok(Release {
        url: page,
        prerelease,
        version: index.version,
    })
}

/// The scheme and host of `url`, without the path.
fn origin(url: &str) -> &str {
    let host = url.find("://").map_or(0, |i| i + 3);
    url[host..].find('/').map_or(url, |i| &url[..host + i])
}

/// Whether `latest` is offered to a build of version `current`: strictly newer, and no
/// pre-release to a stable build.
#[must_use]
pub fn is_offered(latest: &Release, current: &str) -> bool {
    let (Some(new), Some(now)) = (
        PluginVersion::parse(&latest.version),
        PluginVersion::parse(current),
    ) else {
        return false;
    };
    if (latest.prerelease || !new.pre().is_empty()) && now.pre().is_empty() {
        return false;
    }
    new.is_newer_than(&now)
}

/// The last check's answer, kept in [`CACHE_FILE`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateCache {
    /// When the release was last asked for, in seconds since the epoch, answered or not.
    pub checked_at: u64,
    /// The latest release as last read.
    pub latest: Option<Release>,
}

impl UpdateCache {
    /// Whether the last check is less than a day old at `now`. One from the future is not.
    #[must_use]
    pub fn is_fresh(&self, now: u64) -> bool {
        self.checked_at != 0 && now >= self.checked_at && now - self.checked_at < MAX_AGE_SECS
    }

    /// The release to tell a build of version `current` about, when it is offered.
    #[must_use]
    pub fn notice(&self, current: &str) -> Option<&Release> {
        self.latest.as_ref().filter(|r| is_offered(r, current))
    }

    /// The answer kept in `dir`, or none when there is no readable one.
    #[must_use]
    pub fn load(dir: &Path) -> Self {
        std::fs::read(dir.join(CACHE_FILE))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// Write the answer into `dir`, atomically.
    ///
    /// # Errors
    /// Any failure to create the folder or write the file.
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let json = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        let tmp = dir.join(format!("{CACHE_FILE}.tmp"));
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, dir.join(CACHE_FILE))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPO: &str = "https://github.com/jacobjohnston/pyreal";

    fn release(version: &str) -> Release {
        Release {
            version: version.into(),
            url: format!("{REPO}/releases/tag/v{version}"),
            prerelease: false,
        }
    }

    #[test]
    fn versions_compare_by_semver() {
        assert!(is_offered(&release("0.1.10"), "0.1.9"));
        assert!(is_offered(&release("0.2.0"), "0.1.99"));
        assert!(is_offered(&release("1.0.0"), "0.9.9"));
        assert!(
            !is_offered(&release("0.1.2"), "0.1.2"),
            "equal says nothing"
        );
        assert!(
            !is_offered(&release("0.1.1"), "0.1.2"),
            "older says nothing"
        );
        assert!(!is_offered(&release("0.1.9"), "0.1.10"));
        assert!(!is_offered(&release("nonsense"), "0.1.2"));
        assert!(!is_offered(&release("0.1.3"), "dev"));
    }

    #[test]
    fn a_pre_release_never_prompts_a_stable_build() {
        assert!(!is_offered(&release("0.2.0-rc.1"), "0.1.2"));
        let flagged = Release {
            prerelease: true,
            ..release("0.2.0")
        };
        assert!(!is_offered(&flagged, "0.1.2"));
        // A pre-release build hears of the release it leads to, and of later pre-releases.
        assert!(is_offered(&release("0.2.0"), "0.2.0-rc.1"));
        assert!(is_offered(&release("0.2.0-rc.2"), "0.2.0-rc.1"));
        assert!(!is_offered(&release("0.2.0-rc.1"), "0.2.0-rc.1"));
    }

    #[test]
    fn a_release_index_names_its_version_and_page() {
        let json = br#"{"schema":1,"product":"pyreal","version":"0.1.3","tag":"v0.1.3",
            "prerelease":false,"assets":[]}"#;
        let r = parse_release(json, &format!("{REPO}/")).unwrap();
        assert_eq!(r, release("0.1.3"));
        let pre = br#"{"product":"pyreal","version":"0.2.0-rc.1"}"#;
        let r = parse_release(pre, REPO).unwrap();
        assert!(r.prerelease);
        assert_eq!(r.url, format!("{REPO}/releases/tag/v0.2.0-rc.1"));
        // A tag that would leave the page's address is not used.
        let odd = br#"{"product":"pyreal","version":"0.1.3","tag":"../../x?y"}"#;
        assert_eq!(parse_release(odd, REPO).unwrap().url, release("0.1.3").url);
    }

    #[test]
    fn the_site_names_its_own_page() {
        const SITE: &str = "https://pyrealclient.app/release.json";
        let json = br#"{"product":"pyreal","version":"0.2.1","tag":"v0.2.1",
            "page":"https://pyrealclient.app/#download"}"#;
        let r = parse_site_release(json, SITE).unwrap();
        assert_eq!(r.version, "0.2.1");
        assert_eq!(r.url, "https://pyrealclient.app/#download");
        assert!(!r.prerelease);
        // With no page, or one elsewhere or odd, the notice opens the site's front page.
        for page in [
            "",
            r#","page":"https://example.com/x""#,
            r#","page":"https://pyrealclient.app.example.com/""#,
            r#","page":"https://pyrealclient.app/a&calc""#,
            r#","page":"https://pyrealclient.app/ x""#,
        ] {
            let json = format!(r#"{{"product":"pyreal","version":"0.2.1"{page}}}"#);
            let r = parse_site_release(json.as_bytes(), SITE).unwrap();
            assert_eq!(r.url, "https://pyrealclient.app/", "{page}");
        }
        let pre = br#"{"product":"pyreal","version":"0.3.0","prerelease":true}"#;
        assert!(parse_site_release(pre, SITE).unwrap().prerelease);
        assert!(parse_site_release(b"<html>Not found</html>", SITE).is_err());
    }

    #[test]
    fn the_site_is_read_before_the_repository() {
        const SITE: &str = "https://pyrealclient.app/release.json";
        let both = Feed::list(Some(SITE), Some(&format!("{REPO}/")));
        assert_eq!(
            both,
            [Feed::Site(SITE.into()), Feed::Repo(REPO.into())],
            "the site first"
        );
        assert_eq!(both[0].url(), SITE);
        assert_eq!(
            both[1].url(),
            format!("{REPO}/releases/latest/download/release.json")
        );
        assert_eq!(Feed::list(None, Some(REPO)), [Feed::Repo(REPO.into())]);
        assert_eq!(Feed::list(Some(SITE), None), [Feed::Site(SITE.into())]);
        assert!(Feed::list(None, None).is_empty());
        assert!(Feed::list(Some(" "), Some("")).is_empty());
        let json = br#"{"product":"pyreal","version":"0.2.1","tag":"v0.2.1"}"#;
        assert_eq!(
            both[0].parse(json).unwrap().url,
            "https://pyrealclient.app/"
        );
        assert_eq!(
            both[1].parse(json).unwrap().url,
            format!("{REPO}/releases/tag/v0.2.1")
        );
    }

    #[test]
    fn garbage_is_refused() {
        assert!(parse_release(b"<html>rate limited</html>", REPO).is_err());
        assert!(parse_release(br#"{"version":"latest"}"#, REPO).is_err());
        assert!(parse_release(br#"{"product":"other","version":"9.0.0"}"#, REPO).is_err());
        assert!(parse_release(br#"{"tag_name":"v0.1.3"}"#, REPO).is_err());
    }

    #[test]
    fn the_answer_is_kept_a_day() {
        let c = UpdateCache {
            checked_at: 1000,
            ..UpdateCache::default()
        };
        assert!(c.is_fresh(1000));
        assert!(c.is_fresh(1000 + MAX_AGE_SECS - 1));
        assert!(!c.is_fresh(1000 + MAX_AGE_SECS));
        assert!(!c.is_fresh(999), "a check from the future is not fresh");
        assert!(!UpdateCache::default().is_fresh(5), "never checked");
    }

    #[test]
    fn the_notice_names_the_newer_release() {
        let c = UpdateCache {
            checked_at: 1,
            latest: Some(release("0.1.3")),
        };
        assert_eq!(c.notice("0.1.2").map(|r| r.version.as_str()), Some("0.1.3"));
        assert!(c.notice("0.1.3").is_none());
        assert!(UpdateCache::default().notice("0.1.2").is_none());
    }

    #[test]
    fn the_cache_is_saved_and_read_back() {
        let dir = std::env::temp_dir().join(format!("pyreal-update-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(UpdateCache::load(&dir), UpdateCache::default());
        let c = UpdateCache {
            checked_at: 7,
            latest: Some(release("0.1.3")),
        };
        c.save(&dir).unwrap();
        assert_eq!(UpdateCache::load(&dir), c);
        std::fs::write(dir.join(CACHE_FILE), b"not json").unwrap();
        assert_eq!(UpdateCache::load(&dir), UpdateCache::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_release_is_read_from_the_latest_download() {
        assert_eq!(
            release_json_url(&format!("{REPO}/")),
            format!("{REPO}/releases/latest/download/release.json")
        );
    }
}
