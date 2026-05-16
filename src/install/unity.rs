use anyhow::{Context, Result};
use serde_json::json;
use std::{fs::File, path::Path};
use tracing::{debug, info};

const UNITY_ENGINE: &str =
    "Editor/Data/PlaybackEngines/LinuxStandaloneSupport/Variations/linux64_player_nondevelopment_mono";

pub async fn ensure_unity_player(game_dir: &Path, cache_dir: &Path) -> Result<String> {
    let unity_version = detect_unity_version(&game_dir.join("Bin/Hearthstone_Data/level0"))?;
    info!(version = %unity_version, "detected required Unity version");
    let marker = game_dir.join(".unity");
    if marker.exists()
        && std::fs::read_to_string(&marker).unwrap_or_default().trim() == unity_version
        && game_dir.join("Bin/Hearthstone.x86_64").exists()
    {
        debug!(version = %unity_version, "Unity player already installed");
        return Ok(unity_version);
    }

    std::fs::create_dir_all(cache_dir)?;
    let unity_root = cache_dir.join(&unity_version).join(UNITY_ENGINE);
    if !unity_root.join("LinuxPlayer").exists() {
        info!(version = %unity_version, cache_dir = %cache_dir.display(), "Unity player cache miss");
        download_unity(&unity_version, cache_dir).await?;
    }

    copy_unity_files(&unity_root, game_dir)?;
    std::fs::write(marker, &unity_version)?;
    Ok(unity_version)
}

fn detect_unity_version(level0: &Path) -> Result<String> {
    let data =
        std::fs::read(level0).with_context(|| format!("failed to read {}", level0.display()))?;
    let mut best = Vec::new();
    for byte in data {
        if byte.is_ascii_graphic() || byte == b' ' {
            best.push(byte);
        } else if !best.is_empty() {
            let text = String::from_utf8_lossy(&best);
            if looks_like_unity_version(&text) {
                return Ok(text.trim().to_string());
            }
            best.clear();
        }
    }
    anyhow::bail!(
        "could not determine Unity version from {}",
        level0.display()
    )
}

fn looks_like_unity_version(value: &str) -> bool {
    let value = value.trim();
    value.len() >= 6
        && value.contains('.')
        && value.bytes().any(|byte| byte == b'f' || byte == b'p')
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_digit())
}

async fn download_unity(version: &str, cache_dir: &Path) -> Result<()> {
    let hash = fetch_unity_archive_hash(version).await?;
    let url = format!(
        "https://download.unity3d.com/download_unity/{hash}/LinuxEditorInstaller/Unity.tar.xz"
    );
    let archive_path = cache_dir.join(format!("{version}.tar.xz"));
    info!(
        version = %version,
        url = %url,
        archive = %archive_path.display(),
        "downloading Unity archive"
    );
    let bytes = reqwest::get(&url)
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    tokio::fs::write(&archive_path, bytes).await?;
    debug!(archive = %archive_path.display(), "extracting Unity archive");
    extract_unity_archive(&archive_path, &cache_dir.join(version))?;
    Ok(())
}

async fn fetch_unity_archive_hash(version: &str) -> Result<String> {
    debug!(version = %version, "fetching Unity archive hash");
    let body = json!({
        "operationName": "GetRelease",
        "variables": {
            "version": version,
            "limit": 300
        },
        "query": "query GetRelease($limit: Int, $skip: Int, $version: String!, $stream: [UnityReleaseStream!]) { getUnityReleases(limit: $limit skip: $skip stream: $stream version: $version entitlements: [XLTS]) { totalCount edges { node { version entitlements releaseDate unityHubDeepLink stream __typename } __typename } __typename } }"
    });
    let text = reqwest::Client::new()
        .post("https://services.unity.com/graphql")
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let needle = format!("unityhub://{version}/");
    let start = text
        .find(&needle)
        .with_context(|| format!("Unity {version} was not found in Unity release archive"))?
        + needle.len();
    let hash: String = text[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric())
        .collect();
    anyhow::ensure!(!hash.is_empty(), "Unity archive hash was empty");
    Ok(hash)
}

fn extract_unity_archive(archive_path: &Path, destination: &Path) -> Result<()> {
    std::fs::create_dir_all(destination)?;
    let file = File::open(archive_path)?;
    let decoder = xz2::read::XzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        if should_extract(&path) {
            let target = destination.join(path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            entry.unpack(target)?;
        }
    }
    Ok(())
}

fn should_extract(path: &Path) -> bool {
    let path = path.to_string_lossy();
    path == format!("{UNITY_ENGINE}/LinuxPlayer")
        || path == format!("{UNITY_ENGINE}/UnityPlayer.so")
        || path.starts_with(&format!("{UNITY_ENGINE}/Data/MonoBleedingEdge/"))
}

fn copy_unity_files(unity_root: &Path, game_dir: &Path) -> Result<()> {
    info!(unity_root = %unity_root.display(), game_dir = %game_dir.display(), "copying Unity player files");
    let bin = game_dir.join("Bin");
    let data = bin.join("Hearthstone_Data");
    std::fs::create_dir_all(&bin)?;
    std::fs::copy(
        unity_root.join("LinuxPlayer"),
        bin.join("Hearthstone.x86_64"),
    )?;
    std::fs::copy(
        unity_root.join("UnityPlayer.so"),
        bin.join("UnityPlayer.so"),
    )?;
    copy_dir(
        &unity_root.join("Data/MonoBleedingEdge"),
        &data.join("MonoBleedingEdge"),
    )?;
    make_executable(&bin.join("Hearthstone.x86_64"))?;
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    if to.exists() {
        std::fs::remove_dir_all(to)?;
    }
    for entry in walkdir::WalkDir::new(from) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(from)?;
        let target = to.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(perms.mode() | 0o755);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}
