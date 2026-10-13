//! Looks for a newer launcher release and fetches the matching package.
//!
//! Releases are read from the project's GitHub repository. Nothing is
//! installed from here: a downloaded package is handed to the desktop's
//! software installer, and package kinds that cannot be upgraded this way
//! (AppImage, Flatpak, Nix) get the release page instead.

use crate::host_env;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/DawnMagnet/hearthstone-linux-gui/releases/latest";
const USER_AGENT: &str = concat!("hearthstone-linux-gui/", env!("CARGO_PKG_VERSION"));

/// A published launcher release.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Release {
    /// Version as tagged, e.g. `v0.5.0`.
    #[serde(rename = "tag_name")]
    pub version: String,
    /// Release page for a manual download.
    #[serde(rename = "html_url")]
    pub page_url: String,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Asset {
    pub name: String,
    #[serde(rename = "browser_download_url")]
    pub url: String,
}

/// How this launcher was installed, as far as it matters for an upgrade.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageKind {
    Deb,
    Rpm,
    /// AppImage, Flatpak, Nix, Pacman or a local build: upgraded by hand or
    /// by its own package manager.
    Other,
}

impl PackageKind {
    /// Detects the package that owns the running executable.
    pub fn detect() -> Self {
        if host_env::is_active() || host_env::is_flatpak() {
            return Self::Other;
        }
        let Ok(exe) = std::env::current_exe() else {
            return Self::Other;
        };
        if owned_by(&exe, "dpkg-query", &["-S"]) {
            Self::Deb
        } else if owned_by(&exe, "rpm", &["-qf"]) {
            Self::Rpm
        } else {
            Self::Other
        }
    }

    fn asset_suffix(self) -> Option<&'static str> {
        match self {
            Self::Deb => Some("_amd64.deb"),
            Self::Rpm => Some(".x86_64.rpm"),
            Self::Other => None,
        }
    }
}

fn owned_by(exe: &Path, program: &str, args: &[&str]) -> bool {
    host_env::host_command(program)
        .args(args)
        .arg(exe)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

impl Release {
    /// The package of this release for `kind`, if the release has one.
    pub fn asset_for(&self, kind: PackageKind) -> Option<&Asset> {
        let suffix = kind.asset_suffix()?;
        self.assets
            .iter()
            .find(|asset| asset.name.ends_with(suffix))
    }

    /// Whether this release is newer than the running launcher.
    pub fn is_upgrade(&self) -> bool {
        is_newer(&self.version, env!("CARGO_PKG_VERSION"))
    }
}

/// Fetches the latest published release.
pub async fn latest_release() -> Result<Release> {
    let response = client()?
        .get(LATEST_RELEASE_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("failed to query the latest launcher release")?
        .error_for_status()
        .context("the release query was rejected")?;
    response
        .json()
        .await
        .context("the release query returned an unexpected response")
}

/// Downloads `asset` into `dir` and returns the file's path.
pub async fn download(asset: &Asset, dir: &Path) -> Result<PathBuf> {
    // The name comes from the network; keep only its final component.
    let name = Path::new(&asset.name)
        .file_name()
        .context("release asset has no file name")?;
    let bytes = client()?
        .get(&asset.url)
        .send()
        .await
        .with_context(|| format!("failed to download {}", asset.url))?
        .error_for_status()?
        .bytes()
        .await
        .with_context(|| format!("failed to download {}", asset.url))?;
    tokio::fs::create_dir_all(dir).await?;
    let path = dir.join(name);
    tokio::fs::write(&path, &bytes)
        .await
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .context("failed to set up the HTTP client")
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
    use super::{is_newer, Asset, PackageKind, Release};

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
    fn parses_release_and_picks_the_matching_package() {
        let release: Release = serde_json::from_str(
            r#"{
                "tag_name": "v0.5.0",
                "html_url": "https://example.invalid/releases/v0.5.0",
                "assets": [
                    {"name": "launcher-0.5.0-x86_64.AppImage", "browser_download_url": "https://example.invalid/a"},
                    {"name": "launcher_0.5.0_amd64.deb", "browser_download_url": "https://example.invalid/d"},
                    {"name": "launcher-0.5.0-1.x86_64.rpm", "browser_download_url": "https://example.invalid/r"}
                ]
            }"#,
        )
        .unwrap();

        assert_eq!(release.version, "v0.5.0");
        let name = |asset: Option<&Asset>| asset.map(|asset| asset.name.clone());
        assert_eq!(
            name(release.asset_for(PackageKind::Deb)).as_deref(),
            Some("launcher_0.5.0_amd64.deb")
        );
        assert_eq!(
            name(release.asset_for(PackageKind::Rpm)).as_deref(),
            Some("launcher-0.5.0-1.x86_64.rpm")
        );
        assert_eq!(release.asset_for(PackageKind::Other), None);
    }
}
