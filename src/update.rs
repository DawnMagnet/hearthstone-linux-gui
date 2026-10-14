//! Looks for a newer launcher release.
//!
//! Releases are read from the project's GitHub repository. Nothing is
//! downloaded or installed from here: the user is pointed to the release
//! page and follows its instructions, because how a launcher is upgraded
//! depends on how it was installed (AppImage, DEB, RPM, Pacman, Flatpak, Nix,
//! a distribution's own package, ...).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/DawnMagnet/hearthstone-linux-gui/releases/latest";
const USER_AGENT: &str = concat!("hearthstone-linux-gui/", env!("CARGO_PKG_VERSION"));

/// How long a stored answer is reused. Anonymous GitHub API calls are limited
/// per IP address, and every start of the launcher asks.
const CACHE_TTL_SECS: u64 = 6 * 60 * 60;

/// Whether this build offers the update check at all. Packagers whose
/// distribution upgrades the launcher itself build with
/// `HEARTHSTONE_LINUX_NO_UPDATE_CHECK=1` to remove it.
pub const fn enabled() -> bool {
    option_env!("HEARTHSTONE_LINUX_NO_UPDATE_CHECK").is_none()
}

/// A published launcher release.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Release {
    /// Version as tagged, e.g. `v0.5.0`.
    #[serde(rename = "tag_name")]
    pub version: String,
    /// Release page with the download instructions.
    #[serde(rename = "html_url")]
    pub page_url: String,
}

impl Release {
    /// Whether this release is newer than the running launcher.
    pub fn is_upgrade(&self) -> bool {
        is_newer(&self.version, env!("CARGO_PKG_VERSION"))
    }
}

#[derive(Deserialize, Serialize)]
struct Stored {
    checked_at: u64,
    release: Release,
}

/// The latest published release. A stored answer in `cache_file` that is
/// younger than six hours is used instead of asking GitHub; when GitHub
/// cannot be reached an older stored answer is better than none.
pub async fn latest_release(cache_file: &Path) -> Result<Release> {
    let stored: Option<Stored> = std::fs::read_to_string(cache_file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    let now = unix_time();
    if let Some(stored) = stored.as_ref() {
        if now.saturating_sub(stored.checked_at) < CACHE_TTL_SECS {
            return Ok(stored.release.clone());
        }
    }

    match fetch_latest().await {
        Ok(release) => {
            store(cache_file, now, &release);
            Ok(release)
        }
        Err(error) => match stored {
            Some(stored) => {
                tracing::warn!(error = %format!("{error:#}"), "using the stored launcher release");
                Ok(stored.release)
            }
            None => Err(error),
        },
    }
}

async fn fetch_latest() -> Result<Release> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("failed to set up the HTTP client")?;
    client
        .get(LATEST_RELEASE_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("failed to query the latest launcher release")?
        .error_for_status()
        .context("the release query was rejected")?
        .json()
        .await
        .context("the release query returned an unexpected response")
}

fn store(cache_file: &Path, checked_at: u64, release: &Release) {
    let stored = Stored {
        checked_at,
        release: release.clone(),
    };
    let written = cache_file
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(cache_file, serde_json::to_string(&stored)?));
    if let Err(error) = written {
        tracing::debug!(%error, "could not store the launcher release");
    }
}

fn unix_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Compares two versions such as `v0.5.0` and `0.5.0-test.1`. With equal
/// numbers a final release is newer than a pre-release.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    let (candidate_numbers, candidate_pre) = parse_version(candidate);
    let (current_numbers, current_pre) = parse_version(current);
    match candidate_numbers.cmp(&current_numbers) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => current_pre && !candidate_pre,
    }
}

/// `(major, minor, patch)` and whether the version is a pre-release.
fn parse_version(version: &str) -> ([u64; 3], bool) {
    let version = version.trim().trim_start_matches(['v', 'V']);
    let (core, pre) = match version.split_once(['-', '~', '+']) {
        Some((core, _)) => (core, !version[core.len()..].starts_with('+')),
        None => (version, false),
    };
    let mut numbers = [0; 3];
    for (slot, part) in numbers.iter_mut().zip(core.split('.')) {
        *slot = part.parse().unwrap_or(0);
    }
    (numbers, pre)
}

#[cfg(test)]
mod tests {
    use super::{is_newer, latest_release, unix_time, Release, Stored, CACHE_TTL_SECS};

    #[test]
    fn compares_versions() {
        assert!(is_newer("v0.5.0", "0.2.3"));
        assert!(is_newer("0.2.10", "0.2.9"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(is_newer("v0.5.0", "0.5.0-test.1"));
        assert!(!is_newer("v0.5.0", "0.5.0"));
        assert!(!is_newer("v0.5.0-rc.1", "0.5.0"));
        assert!(!is_newer("v0.2.3", "0.5.0"));
        assert!(!is_newer("nonsense", "0.2.3"));
    }

    #[test]
    fn parses_the_release_and_ignores_other_fields() {
        let release: Release = serde_json::from_str(
            r#"{
                "tag_name": "v0.5.0",
                "html_url": "https://example.invalid/releases/v0.5.0",
                "assets": [{"name": "launcher.deb"}]
            }"#,
        )
        .unwrap();
        assert_eq!(release.version, "v0.5.0");
        assert_eq!(release.page_url, "https://example.invalid/releases/v0.5.0");
    }

    #[tokio::test]
    async fn a_recent_stored_answer_is_used_without_asking_github() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("update-check.json");
        let release = Release {
            version: "v9.9.9".into(),
            page_url: "https://example.invalid/releases/v9.9.9".into(),
        };
        let stored = Stored {
            checked_at: unix_time() - CACHE_TTL_SECS / 2,
            release: release.clone(),
        };
        std::fs::write(&cache, serde_json::to_string(&stored).unwrap()).unwrap();

        assert_eq!(latest_release(&cache).await.unwrap(), release);
    }
}
