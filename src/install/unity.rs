use crate::{
    download::{self, DownloadProgress},
    util,
};
use anyhow::{Context, Result};
use serde::Deserialize;
use sha1::Digest as _;
use std::{
    fs::File,
    io::{Read, Result as IoResult, Write},
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

const UNITY_ENGINE: &str =
    "Editor/Data/PlaybackEngines/LinuxStandaloneSupport/Variations/linux64_player_nondevelopment_mono";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnityProgressPhase {
    Downloading,
    Extracting,
}

pub async fn ensure_unity_player(game_dir: &Path, cache_dir: &Path) -> Result<String> {
    ensure_unity_player_with_progress(game_dir, cache_dir, None, |_| {}).await
}

#[derive(Clone, Debug)]
pub struct UnityDownloadProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub speed_bytes_per_second: f64,
    pub resumed: bool,
    pub phase: UnityProgressPhase,
}

impl UnityDownloadProgress {
    pub fn fraction(&self) -> Option<f64> {
        let total = self.total?;
        if total == 0 {
            return None;
        }
        let phase_fraction = (self.downloaded as f64 / total as f64).clamp(0.0, 1.0);
        Some(match self.phase {
            UnityProgressPhase::Downloading => phase_fraction * 0.9,
            UnityProgressPhase::Extracting => 0.9 + phase_fraction * 0.1,
        })
    }
}

pub async fn ensure_unity_player_with_progress(
    game_dir: &Path,
    cache_dir: &Path,
    cancel: Option<CancellationToken>,
    mut progress: impl FnMut(UnityDownloadProgress) + Send,
) -> Result<String> {
    let unity_version = detect_unity_version(&game_dir.join("Bin/Hearthstone_Data/level0"))?;
    info!(version = %unity_version, "detected required Unity version");
    let marker = game_dir.join(".unity");
    if marker.exists()
        && std::fs::read_to_string(&marker).unwrap_or_default().trim() == unity_version
        && unity_player_files_exist(game_dir)
    {
        debug!(version = %unity_version, "Unity player already installed");
        return Ok(unity_version);
    }

    std::fs::create_dir_all(cache_dir)?;
    let unity_root = cache_dir.join(&unity_version).join(UNITY_ENGINE);
    if !unity_root.join("LinuxPlayer").exists() {
        info!(version = %unity_version, cache_dir = %cache_dir.display(), "Unity player cache miss");
        download_unity(&unity_version, cache_dir, cancel.as_ref(), &mut progress).await?;
    }

    copy_unity_files(&unity_root, game_dir)?;
    util::write_file_atomic(&marker, unity_version.as_bytes())?;
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

async fn download_unity(
    version: &str,
    cache_dir: &Path,
    cancel: Option<&CancellationToken>,
    progress: &mut (impl FnMut(UnityDownloadProgress) + Send),
) -> Result<()> {
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
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .build()?;
    download::download_file(&client, &url, &archive_path, cancel.cloned(), |update| {
        progress(UnityDownloadProgress::from(update));
    })
    .await?;
    debug!(archive = %archive_path.display(), "extracting Unity archive");
    extract_unity_archive_with_progress(&archive_path, &cache_dir.join(version), progress)?;
    Ok(())
}

impl From<DownloadProgress> for UnityDownloadProgress {
    fn from(progress: DownloadProgress) -> Self {
        Self {
            downloaded: progress.downloaded,
            total: progress.total,
            speed_bytes_per_second: progress.speed_bytes_per_second,
            resumed: progress.resumed,
            phase: UnityProgressPhase::Downloading,
        }
    }
}

async fn fetch_unity_archive_hash(version: &str) -> Result<String> {
    debug!(version = %version, "fetching Unity archive hash");
    let text = reqwest::Client::new()
        .get("https://services.api.unity.com/unity/editor/release/v1/releases")
        .query(&[
            ("version", version),
            ("limit", "1"),
            ("order", "RELEASE_DATE_DESC"),
        ])
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    unity_archive_hash_from_release_api(&text, version)
}

#[derive(Debug, Deserialize)]
struct UnityReleaseApiResponse {
    results: Vec<UnityRelease>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnityRelease {
    version: String,
    short_revision: Option<String>,
    unity_hub_deep_link: Option<String>,
}

fn unity_archive_hash_from_release_api(text: &str, version: &str) -> Result<String> {
    let response: UnityReleaseApiResponse =
        serde_json::from_str(text).context("Unity release API returned an unexpected response")?;
    let release = response
        .results
        .into_iter()
        .find(|release| release.version == version)
        .with_context(|| format!("Unity {version} was not found in Unity release archive"))?;

    if let Some(hash) = release.short_revision {
        ensure_unity_archive_hash(&hash)?;
        return Ok(hash);
    }

    let needle = format!("unityhub://{version}/");
    let deep_link = release
        .unity_hub_deep_link
        .with_context(|| format!("Unity {version} release did not include a download hash"))?;
    let hash = deep_link
        .strip_prefix(&needle)
        .with_context(|| format!("Unity {version} release had an unexpected Hub link"))?;
    let hash: String = hash
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric())
        .collect();
    ensure_unity_archive_hash(&hash)?;
    Ok(hash)
}

fn ensure_unity_archive_hash(hash: &str) -> Result<()> {
    anyhow::ensure!(!hash.is_empty(), "Unity archive hash was empty");
    anyhow::ensure!(
        hash.chars().all(|ch| ch.is_ascii_alphanumeric()),
        "Unity archive hash contained unexpected characters"
    );
    Ok(())
}

fn extract_unity_archive_with_progress(
    archive_path: &Path,
    destination: &Path,
    progress: &mut (impl FnMut(UnityDownloadProgress) + Send),
) -> Result<()> {
    std::fs::create_dir_all(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    let file = File::open(archive_path)
        .with_context(|| format!("failed to read {}", archive_path.display()))?;
    let total = file.metadata().ok().map(|metadata| metadata.len());
    progress(UnityDownloadProgress {
        downloaded: 0,
        total,
        speed_bytes_per_second: 0.0,
        resumed: false,
        phase: UnityProgressPhase::Extracting,
    });
    let reader = ProgressReader::new(file, total, progress);
    let decoder = xz2::read::XzDecoder::new(reader);
    extract_unity_tree(decoder, destination)?;
    if let Some(total) = total {
        progress(UnityDownloadProgress {
            downloaded: total,
            total: Some(total),
            speed_bytes_per_second: 0.0,
            resumed: false,
            phase: UnityProgressPhase::Extracting,
        });
    }
    Ok(())
}

/// Extracts the Unity player files, rejecting entries that would leave
/// `destination`. This is the `Entry::unpack_in` path check, plus a size and
/// content-hash check when the archive header or pax metadata provides them,
/// and an atomic replace so a crash cannot leave a short file at the final path.
fn extract_unity_tree<R: Read>(reader: R, destination: &Path) -> Result<()> {
    std::fs::create_dir_all(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    let mut archive = tar::Archive::new(reader);
    for entry in archive
        .entries()
        .with_context(|| format!("failed to read Unity archive for {}", destination.display()))?
    {
        let mut entry = entry.with_context(|| {
            format!(
                "failed to read Unity archive entry for {}",
                destination.display()
            )
        })?;
        let raw_path = entry.path().with_context(|| {
            format!(
                "invalid path in Unity archive for {}",
                destination.display()
            )
        })?;
        let relative = normalize_archive_path(&raw_path).with_context(|| {
            format!(
                "Unity archive entry {} escapes {}",
                raw_path.display(),
                destination.display()
            )
        })?;
        if !should_extract(&relative) {
            continue;
        }
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            place_inside(destination, &relative, true)?;
            continue;
        }
        if !kind.is_file() {
            anyhow::bail!(
                "Unity archive entry {} is not a regular file under {}",
                relative.display(),
                destination.display()
            );
        }
        let expected_size = entry.size();
        let hashes = expected_content_hashes(&mut entry, &relative)?;
        write_extracted_file(&mut entry, destination, &relative, expected_size, &hashes)?;
    }
    Ok(())
}

fn normalize_archive_path(path: &Path) -> Result<PathBuf> {
    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                anyhow::bail!("archive entry {} escapes the destination", path.display());
            }
        }
    }
    Ok(relative)
}

/// Resolves `relative` under `root` without following a symlink out of `root`.
fn place_inside(root: &Path, relative: &Path, last_is_dir: bool) -> Result<PathBuf> {
    let root = std::fs::canonicalize(root)
        .with_context(|| format!("failed to resolve {}", root.display()))?;
    if relative.as_os_str().is_empty() {
        anyhow::bail!("Unity archive entry escapes {}", root.display());
    }
    let mut current = root.clone();
    let components: Vec<_> = relative.components().collect();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(part) = component else {
            anyhow::bail!(
                "Unity archive entry {} escapes {}",
                relative.display(),
                root.display()
            );
        };
        let next = current.join(part);
        let is_last = index + 1 == components.len();
        match std::fs::symlink_metadata(&next) {
            Ok(meta) if meta.file_type().is_symlink() => {
                anyhow::bail!(
                    "Unity archive entry {} escapes {} through {}",
                    relative.display(),
                    root.display(),
                    next.display()
                );
            }
            Ok(meta) if meta.is_dir() => current = next,
            Ok(_) if is_last && !last_is_dir => current = next,
            Ok(_) => {
                anyhow::bail!("failed to create {}: not a directory", next.display());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if !is_last || last_is_dir {
                    std::fs::create_dir(&next)
                        .with_context(|| format!("failed to create {}", next.display()))?;
                }
                current = next;
            }
            Err(error) => {
                return Err(error).with_context(|| format!("failed to inspect {}", next.display()));
            }
        }
    }
    anyhow::ensure!(
        current.starts_with(&root),
        "Unity archive entry {} escapes {}",
        relative.display(),
        root.display()
    );
    Ok(current)
}

fn write_extracted_file<R: Read>(
    entry: &mut tar::Entry<'_, R>,
    destination: &Path,
    relative: &Path,
    expected_size: u64,
    hashes: &[ExpectedHash],
) -> Result<()> {
    let target = place_inside(destination, relative, false)?;
    let parent = target.parent().unwrap_or(destination);
    let temp = crate::util::temporary_sibling(&target);
    let mode = archive_mode(entry.header());
    let mut created = false;
    let result = (|| -> Result<()> {
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .with_context(|| format!("failed to write {}", target.display()))?;
        created = true;
        let mut hasher = HashState::new(hashes);
        let mut written = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = entry.read(&mut buffer).with_context(|| {
                format!("failed to read Unity archive entry {}", relative.display())
            })?;
            if read == 0 {
                break;
            }
            output
                .write_all(&buffer[..read])
                .with_context(|| format!("failed to write {}", target.display()))?;
            hasher.update(&buffer[..read]);
            written = written.saturating_add(read as u64);
        }
        anyhow::ensure!(
            written == expected_size,
            "extracted {written} bytes for {}, expected {expected_size}",
            target.display()
        );
        hasher.verify(&target, hashes)?;
        set_file_mode(&temp, mode)?;
        output
            .sync_all()
            .with_context(|| format!("failed to sync {}", target.display()))?;
        drop(output);
        std::fs::rename(&temp, &target)
            .with_context(|| format!("failed to replace {}", target.display()))?;
        crate::util::sync_dir(parent)?;
        Ok(())
    })();
    if created && result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn archive_mode(header: &tar::Header) -> u32 {
    let mode = header.mode().unwrap_or(0o644) & 0o777;
    if mode == 0 {
        0o644
    } else {
        mode
    }
}

#[cfg(unix)]
fn set_file_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .with_context(|| format!("failed to set permissions on {}", path.display()))
}

#[cfg(not(unix))]
fn set_file_mode(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum HashKind {
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

impl HashKind {
    fn hex_len(self) -> usize {
        match self {
            Self::Md5 => 32,
            Self::Sha1 => 40,
            Self::Sha256 => 64,
            Self::Sha512 => 128,
        }
    }
}

struct ExpectedHash {
    kind: HashKind,
    hex: String,
}

struct HashState {
    md5: Option<md5::Context>,
    sha1: Option<sha1::Sha1>,
    sha256: Option<sha2::Sha256>,
    sha512: Option<sha2::Sha512>,
}

impl HashState {
    fn new(expected: &[ExpectedHash]) -> Self {
        Self {
            md5: expected
                .iter()
                .any(|hash| matches!(hash.kind, HashKind::Md5))
                .then(md5::Context::new),
            sha1: expected
                .iter()
                .any(|hash| matches!(hash.kind, HashKind::Sha1))
                .then(sha1::Sha1::new),
            sha256: expected
                .iter()
                .any(|hash| matches!(hash.kind, HashKind::Sha256))
                .then(sha2::Sha256::new),
            sha512: expected
                .iter()
                .any(|hash| matches!(hash.kind, HashKind::Sha512))
                .then(sha2::Sha512::new),
        }
    }

    fn update(&mut self, data: &[u8]) {
        if let Some(context) = &mut self.md5 {
            context.consume(data);
        }
        if let Some(hasher) = &mut self.sha1 {
            hasher.update(data);
        }
        if let Some(hasher) = &mut self.sha256 {
            hasher.update(data);
        }
        if let Some(hasher) = &mut self.sha512 {
            hasher.update(data);
        }
    }

    fn verify(self, path: &Path, expected: &[ExpectedHash]) -> Result<()> {
        for hash in expected {
            let actual = match hash.kind {
                HashKind::Md5 => self
                    .md5
                    .as_ref()
                    .map(|context| format!("{:x}", context.clone().compute()))
                    .unwrap_or_default(),
                HashKind::Sha1 => self
                    .sha1
                    .as_ref()
                    .map(|hasher| hex::encode(hasher.clone().finalize()))
                    .unwrap_or_default(),
                HashKind::Sha256 => self
                    .sha256
                    .as_ref()
                    .map(|hasher| hex::encode(hasher.clone().finalize()))
                    .unwrap_or_default(),
                HashKind::Sha512 => self
                    .sha512
                    .as_ref()
                    .map(|hasher| hex::encode(hasher.clone().finalize()))
                    .unwrap_or_default(),
            };
            anyhow::ensure!(
                actual.eq_ignore_ascii_case(&hash.hex),
                "hash mismatch for {}: expected {}, got {actual}",
                path.display(),
                hash.hex
            );
        }
        Ok(())
    }
}

fn expected_content_hashes<R: Read>(
    entry: &mut tar::Entry<'_, R>,
    relative: &Path,
) -> Result<Vec<ExpectedHash>> {
    let Some(extensions) = entry
        .pax_extensions()
        .with_context(|| format!("failed to read archive metadata for {}", relative.display()))?
    else {
        return Ok(Vec::new());
    };
    let mut hashes = Vec::new();
    for extension in extensions {
        let extension = extension
            .with_context(|| format!("invalid archive metadata for {}", relative.display()))?;
        let key = String::from_utf8_lossy(extension.key_bytes());
        let Some(kind) = classify_hash_key(&key) else {
            continue;
        };
        let value = std::str::from_utf8(extension.value_bytes())
            .with_context(|| format!("invalid hash for {}", relative.display()))?;
        let hex = parse_hash_hex(value)
            .with_context(|| format!("invalid hash for {}", relative.display()))?;
        anyhow::ensure!(
            hex.len() == kind.hex_len(),
            "invalid hash length for {}",
            relative.display()
        );
        hashes.push(ExpectedHash { kind, hex });
    }
    Ok(hashes)
}

fn classify_hash_key(key: &str) -> Option<HashKind> {
    let name = key.rsplit(['.', '/']).next().unwrap_or(key).trim();
    if name.eq_ignore_ascii_case("md5") {
        Some(HashKind::Md5)
    } else if name.eq_ignore_ascii_case("sha1") || name.eq_ignore_ascii_case("sha-1") {
        Some(HashKind::Sha1)
    } else if name.eq_ignore_ascii_case("sha256") || name.eq_ignore_ascii_case("sha-256") {
        Some(HashKind::Sha256)
    } else if name.eq_ignore_ascii_case("sha512") || name.eq_ignore_ascii_case("sha-512") {
        Some(HashKind::Sha512)
    } else {
        None
    }
}

fn parse_hash_hex(value: &str) -> Result<String> {
    let value = value.trim();
    let hex = match value.split_once(':') {
        Some((_, rest)) => rest.trim(),
        None => value,
    };
    let hex = hex.strip_prefix("0x").unwrap_or(hex);
    anyhow::ensure!(
        !hex.is_empty() && hex.chars().all(|ch| ch.is_ascii_hexdigit()),
        "invalid archive hash `{value}`"
    );
    Ok(hex.to_ascii_lowercase())
}

struct ProgressReader<'a, R, F>
where
    F: FnMut(UnityDownloadProgress) + Send,
{
    inner: R,
    total: Option<u64>,
    progress: &'a mut F,
    read: u64,
    window_read: u64,
    window_start: Instant,
    last_emit: Instant,
}

impl<'a, R, F> ProgressReader<'a, R, F>
where
    F: FnMut(UnityDownloadProgress) + Send,
{
    fn new(inner: R, total: Option<u64>, progress: &'a mut F) -> Self {
        let now = Instant::now();
        Self {
            inner,
            total,
            progress,
            read: 0,
            window_read: 0,
            window_start: now,
            last_emit: now,
        }
    }
}

impl<R, F> Read for ProgressReader<'_, R, F>
where
    R: Read,
    F: FnMut(UnityDownloadProgress) + Send,
{
    fn read(&mut self, buffer: &mut [u8]) -> IoResult<usize> {
        let read = self.inner.read(buffer)?;
        if read == 0 {
            return Ok(0);
        }

        let read = read as u64;
        self.read = self.read.saturating_add(read);
        self.window_read = self.window_read.saturating_add(read);
        let elapsed_since_emit = self.last_emit.elapsed();
        if elapsed_since_emit >= Duration::from_millis(500) {
            let elapsed = self.window_start.elapsed().as_secs_f64().max(0.001);
            (self.progress)(UnityDownloadProgress {
                downloaded: self.read,
                total: self.total,
                speed_bytes_per_second: self.window_read as f64 / elapsed,
                resumed: false,
                phase: UnityProgressPhase::Extracting,
            });
            self.window_read = 0;
            self.window_start = Instant::now();
            self.last_emit = Instant::now();
        }

        Ok(read as usize)
    }
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
    std::fs::create_dir_all(&bin).with_context(|| format!("failed to create {}", bin.display()))?;
    util::copy_file_atomic(
        &unity_root.join("LinuxPlayer"),
        &bin.join("Hearthstone.x86_64"),
    )?;
    util::copy_file_atomic(
        &unity_root.join("UnityPlayer.so"),
        &bin.join("UnityPlayer.so"),
    )?;
    util::copy_dir_all(
        &unity_root.join("Data/MonoBleedingEdge"),
        &data.join("MonoBleedingEdge"),
        true,
    )?;
    util::make_executable(&bin.join("Hearthstone.x86_64"))?;
    Ok(())
}

fn unity_player_files_exist(game_dir: &Path) -> bool {
    let bin = game_dir.join("Bin");
    let data = bin.join("Hearthstone_Data");
    bin.join("Hearthstone.x86_64").exists()
        && bin.join("UnityPlayer.so").exists()
        && data
            .join("MonoBleedingEdge/x86_64/libmonobdwgc-2.0.so")
            .exists()
        && data.join("MonoBleedingEdge/etc/mono/config").exists()
}

#[cfg(test)]
mod tests {
    use super::{extract_unity_tree, unity_archive_hash_from_release_api, UNITY_ENGINE};
    use std::io::Cursor;

    #[test]
    fn reads_unity_hash_from_short_revision() {
        let response = r#"{
            "results": [
                {
                    "version": "2022.3.62f2",
                    "shortRevision": "7670c08855a9",
                    "unityHubDeepLink": "unityhub://2022.3.62f2/7670c08855a9"
                }
            ]
        }"#;

        let hash = unity_archive_hash_from_release_api(response, "2022.3.62f2").unwrap();

        assert_eq!(hash, "7670c08855a9");
    }

    #[test]
    fn falls_back_to_unity_hub_deep_link() {
        let response = r#"{
            "results": [
                {
                    "version": "2022.3.62f2",
                    "unityHubDeepLink": "unityhub://2022.3.62f2/7670c08855a9"
                }
            ]
        }"#;

        let hash = unity_archive_hash_from_release_api(response, "2022.3.62f2").unwrap();

        assert_eq!(hash, "7670c08855a9");
    }

    #[test]
    fn rejects_unexpected_unity_hash_characters() {
        let response = r#"{
            "results": [
                {
                    "version": "2022.3.62f2",
                    "shortRevision": "7670c08855a9/"
                }
            ]
        }"#;

        let error = unity_archive_hash_from_release_api(response, "2022.3.62f2")
            .expect_err("hash should be rejected");

        assert!(
            error
                .to_string()
                .contains("Unity archive hash contained unexpected characters"),
            "{error:#}"
        );
    }

    fn player(name: &str) -> String {
        format!("{UNITY_ENGINE}/{name}")
    }

    fn sha256_hex(data: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(data))
    }

    fn archive_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, data) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_mode(0o644);
            header.set_size(data.len() as u64);
            header.set_cksum();
            builder.append_data(&mut header, path, *data).unwrap();
        }
        builder.into_inner().unwrap()
    }

    fn write_octal(slot: &mut [u8], value: u64) {
        slot.fill(b'0');
        slot[slot.len() - 1] = 0;
        let rendered = format!("{value:o}");
        let digits = rendered.as_bytes();
        let end = slot.len() - 1;
        let start = end - digits.len();
        slot[start..end].copy_from_slice(digits);
    }

    fn finish_header(header: &mut [u8; 512]) {
        header[148..156].fill(b' ');
        let sum = header.iter().enumerate().fold(0u32, |sum, (index, byte)| {
            if (148..156).contains(&index) {
                sum + u32::from(b' ')
            } else {
                sum + u32::from(*byte)
            }
        });
        write_octal(&mut header[148..156], u64::from(sum));
    }

    /// Builds a tar entry without the writer's refusal to store `..` or absolute paths.
    fn push_raw_entry(out: &mut Vec<u8>, path: &str, data: &[u8]) {
        let bytes = path.as_bytes();
        if bytes.len() > 100 {
            let mut long = [0u8; 512];
            let marker = b"././@LongLink";
            long[..marker.len()].copy_from_slice(marker);
            write_octal(&mut long[100..108], 0o644);
            write_octal(&mut long[108..116], 0);
            write_octal(&mut long[116..124], 0);
            write_octal(&mut long[124..136], (bytes.len() + 1) as u64);
            write_octal(&mut long[136..148], 0);
            long[156] = b'L';
            long[257..262].copy_from_slice(b"ustar");
            long[263..265].copy_from_slice(b"00");
            finish_header(&mut long);
            out.extend_from_slice(&long);
            out.extend_from_slice(bytes);
            out.push(0);
            let used = bytes.len() + 1;
            out.extend(std::iter::repeat_n(0u8, (512 - (used % 512)) % 512));
        }
        let mut header = [0u8; 512];
        let name_len = bytes.len().min(100);
        header[..name_len].copy_from_slice(&bytes[..name_len]);
        write_octal(&mut header[100..108], 0o644);
        write_octal(&mut header[108..116], 0);
        write_octal(&mut header[116..124], 0);
        write_octal(&mut header[124..136], data.len() as u64);
        write_octal(&mut header[136..148], 0);
        header[156] = b'0';
        header[257..262].copy_from_slice(b"ustar");
        header[263..265].copy_from_slice(b"00");
        finish_header(&mut header);
        out.extend_from_slice(&header);
        out.extend_from_slice(data);
        out.extend(std::iter::repeat_n(0u8, (512 - (data.len() % 512)) % 512));
    }

    #[test]
    fn extracts_selected_files_and_checks_size_and_hash() {
        let player_bytes = b"linux-player";
        let mono = b"mono-lib";
        let player_hash = sha256_hex(player_bytes);
        let mut builder = tar::Builder::new(Vec::new());
        builder
            .append_pax_extensions([("SHA256", player_hash.as_bytes())])
            .unwrap();
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o755);
        header.set_size(player_bytes.len() as u64);
        header.set_cksum();
        builder
            .append_data(&mut header, player("LinuxPlayer"), player_bytes.as_slice())
            .unwrap();
        let mut plain = tar::Header::new_gnu();
        plain.set_mode(0o644);
        plain.set_size(mono.len() as u64);
        plain.set_cksum();
        builder
            .append_data(
                &mut plain,
                player("Data/MonoBleedingEdge/x86_64/lib.so"),
                mono.as_slice(),
            )
            .unwrap();
        let mut skipped = tar::Header::new_gnu();
        skipped.set_mode(0o644);
        skipped.set_size(4);
        skipped.set_cksum();
        builder
            .append_data(&mut skipped, "Editor/README", b"skip".as_slice())
            .unwrap();
        let archive = builder.into_inner().unwrap();

        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("unity");
        extract_unity_tree(Cursor::new(archive), &destination).unwrap();

        let extracted = destination.join(player("LinuxPlayer"));
        assert_eq!(std::fs::read(&extracted).unwrap(), player_bytes);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(
                std::fs::metadata(&extracted).unwrap().permissions().mode() & 0o111,
                0
            );
        }
        assert_eq!(
            std::fs::read(destination.join(player("Data/MonoBleedingEdge/x86_64/lib.so"))).unwrap(),
            mono
        );
        assert!(!destination.join("Editor/README").exists());
        assert!(!temp
            .path()
            .join(format!(".tmp-{}", std::process::id()))
            .exists());
    }

    #[test]
    fn rejects_hash_mismatch_without_leaving_the_file() {
        let bytes = b"linux-player";
        let wrong_hash = sha256_hex(b"other");
        let mut builder = tar::Builder::new(Vec::new());
        builder
            .append_pax_extensions([("SHA256", wrong_hash.as_bytes())])
            .unwrap();
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o644);
        header.set_size(bytes.len() as u64);
        header.set_cksum();
        builder
            .append_data(&mut header, player("LinuxPlayer"), bytes.as_slice())
            .unwrap();
        let archive = builder.into_inner().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("unity");
        let error = extract_unity_tree(Cursor::new(archive), &destination).unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("hash mismatch"), "{message}");
        assert!(message.contains("LinuxPlayer"), "{message}");
        assert!(!destination.join(player("LinuxPlayer")).exists());
    }

    #[test]
    fn rejects_short_entry_without_leaving_the_file() {
        let path = player("LinuxPlayer");
        let archive = archive_with(&[(path.as_str(), &[b'a'; 100])]);
        let mut reader = tar::Archive::new(Cursor::new(archive.clone()));
        let entry = reader.entries().unwrap().next().unwrap().unwrap();
        let start = entry.raw_file_position() as usize;
        let truncated = archive[..start + 40].to_vec();
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("unity");
        let error = extract_unity_tree(Cursor::new(truncated), &destination).unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("expected 100"), "{message}");
        assert!(message.contains("LinuxPlayer"), "{message}");
        assert!(!destination.join(&path).exists());
    }

    #[test]
    fn rejects_paths_that_escape_the_destination() {
        let mut prefixed = format!("{UNITY_ENGINE}/Data/MonoBleedingEdge");
        for _ in 0..9 {
            prefixed.push_str("/..");
        }
        prefixed.push_str("/escaped");
        let cases = [
            "../escaped".to_string(),
            prefixed,
            "/etc/passwd".to_string(),
        ];
        for path in cases {
            let mut archive = Vec::new();
            push_raw_entry(&mut archive, path.as_str(), b"nope");
            let temp = tempfile::tempdir().unwrap();
            let destination = temp.path().join("unity");
            let error = extract_unity_tree(Cursor::new(archive), &destination).unwrap_err();
            let message = format!("{error:#}");
            assert!(message.contains("escapes"), "{path}: {message}");
            assert!(!temp.path().join("escaped").exists(), "{path}");
            assert!(!destination.join("escaped").exists(), "{path}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_and_symlink_parents_inside_the_extract_set() {
        let link = player("Data/MonoBleedingEdge/link");
        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        builder.append_link(&mut header, &link, "/tmp").unwrap();
        let archive = builder.into_inner().unwrap();

        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("unity");
        let error = extract_unity_tree(Cursor::new(archive), &destination).unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("not a regular file"), "{message}");
        assert!(!destination.join(&link).exists());

        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&destination).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, destination.join("Editor")).unwrap();
        let player_path = player("LinuxPlayer");
        let archive = archive_with(&[(player_path.as_str(), b"player".as_slice())]);
        let error = extract_unity_tree(Cursor::new(archive), &destination).unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("escapes"), "{message}");
        assert!(!outside.join("LinuxPlayer").exists());
        assert!(std::fs::read_dir(&outside).unwrap().next().is_none());
    }
}
