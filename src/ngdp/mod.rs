pub mod archive;
pub mod blizini;
pub mod blte;
pub mod cdn;
pub mod configfile;
pub mod encoding;
pub mod installfile;
pub mod psv;

use crate::{Locale, Region};
use anyhow::{Context, Result};
use cdn::RemoteCdn;
use configfile::{BuildConfig, CdnConfig};
use encoding::EncodingFile;
use installfile::InstallFile;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct VersionInfo {
    pub region: String,
    pub build_config: String,
    pub cdn_config: String,
    pub build_id: String,
    pub version_name: String,
    pub product_config: Option<String>,
}

#[derive(Clone, Debug)]
pub struct InstallOptions {
    pub region: Region,
    pub locale: Locale,
    pub verify: bool,
}

#[derive(Clone, Debug)]
pub struct ProgressUpdate {
    pub message: String,
    pub fraction: Option<f64>,
}

impl ProgressUpdate {
    pub fn new(message: impl Into<String>, fraction: impl Into<Option<f64>>) -> Self {
        Self {
            message: message.into(),
            fraction: fraction.into().map(|value| value.clamp(0.0, 1.0)),
        }
    }
}

pub struct NgdpClient {
    http: reqwest::Client,
    cache_dir: Option<PathBuf>,
}

impl Default for NgdpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl NgdpClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(30))
                .build()
                .expect("reqwest client"),
            cache_dir: None,
        }
    }

    pub fn with_cache_dir(mut self, cache_dir: impl Into<PathBuf>) -> Self {
        self.cache_dir = Some(cache_dir.into());
        self
    }

    pub async fn latest_version(&self, region: Region) -> Result<VersionInfo> {
        let mut errors = Vec::new();

        for url in version_urls() {
            match self.fetch_latest_version(region, &url).await {
                Ok(version) => return Ok(version),
                Err(error) => errors.push(format!("{url}: {error:#}")),
            }
        }

        anyhow::bail!(
            "could not fetch Hearthstone version metadata for region {region}; tried: {}",
            errors.join("; ")
        )
    }

    async fn fetch_latest_version(&self, region: Region, url: &str) -> Result<VersionInfo> {
        let text = self
            .http
            .get(url)
            .send()
            .await
            .with_context(|| format!("request failed for {url}"))?
            .error_for_status()
            .with_context(|| format!("server rejected {url}"))?
            .text()
            .await
            .with_context(|| format!("failed to read response from {url}"))?;
        let psv = psv::PsvFile::parse(&text)?;

        psv.rows
            .into_iter()
            .find(|row| row.get("Region").map(String::as_str) == Some(region.as_str()))
            .map(|row| VersionInfo {
                region: row.get("Region").cloned().unwrap_or_default(),
                build_config: row.get("BuildConfig").cloned().unwrap_or_default(),
                cdn_config: row.get("CDNConfig").cloned().unwrap_or_default(),
                build_id: row.get("BuildId").cloned().unwrap_or_default(),
                version_name: row.get("VersionsName").cloned().unwrap_or_default(),
                product_config: row.get("ProductConfig").cloned(),
            })
            .with_context(|| format!("no version entry found for region {region} in {url}"))
    }

    pub async fn install_latest(
        &self,
        options: &InstallOptions,
        out_dir: &Path,
        mut progress: impl FnMut(ProgressUpdate) + Send,
    ) -> Result<VersionInfo> {
        progress(ProgressUpdate::new(
            "Checking latest Hearthstone version",
            0.02,
        ));
        let version = self.latest_version(options.region).await?;
        let mut cdn = RemoteCdn::from_forced_url(self.http.clone(), options.region.default_cdn())?;
        if let Some(cache_dir) = &self.cache_dir {
            cdn = cdn.with_cache_dir(cache_dir);
        }

        progress(ProgressUpdate::new(
            format!("Fetching build config {}", version.build_config),
            0.06,
        ));
        let build_config = BuildConfig::parse(
            &cdn.fetch_config(&version.build_config, options.verify)
                .await?,
        )?;

        progress(ProgressUpdate::new(
            format!("Fetching CDN config {}", version.cdn_config),
            0.10,
        ));
        let cdn_config = CdnConfig::parse(
            &cdn.fetch_config(&version.cdn_config, options.verify)
                .await?,
        )?;

        progress(ProgressUpdate::new("Fetching encoding table", 0.14));
        let encoding = self
            .fetch_encoding(&cdn, &build_config, options.verify)
            .await?;

        progress(ProgressUpdate::new("Fetching install manifest", 0.18));
        let install = self
            .fetch_install_manifest(&cdn, &build_config, &encoding, options.verify)
            .await?;

        let tags = ["OSX", options.locale.as_str(), "Production"];
        let entries = install.filter_entries(&tags)?;
        progress(ProgressUpdate::new(
            format!("Installing {} files", entries.len()),
            0.22,
        ));

        let archive_map =
            archive::ArchiveMap::load(&cdn, &cdn_config, options.verify, |message, fraction| {
                progress(ProgressUpdate::new(
                    message,
                    fraction.map(|value| 0.22 + value * 0.13),
                ))
            })
            .await?;

        for (idx, entry) in entries.iter().enumerate() {
            if idx % 10 == 0 || idx + 1 == entries.len() {
                let file_fraction = if entries.is_empty() {
                    1.0
                } else {
                    (idx + 1) as f64 / entries.len() as f64
                };
                progress(ProgressUpdate::new(
                    format!(
                        "Downloading and installing file {}/{}",
                        idx + 1,
                        entries.len()
                    ),
                    0.35 + file_fraction * 0.60,
                ));
            }

            let encoding_key = encoding
                .find_by_content_key(&entry.content_key)
                .with_context(|| format!("encoding key not found for {}", entry.path))?;
            let decoded = self
                .fetch_install_entry(
                    &cdn,
                    &archive_map,
                    encoding_key,
                    &entry.content_key,
                    &entry.path,
                    options.verify,
                    archive_map.contains(encoding_key),
                )
                .await?;
            if let Some(transfer) = cdn.last_transfer_label() {
                let file_fraction = if entries.is_empty() {
                    1.0
                } else {
                    (idx + 1) as f64 / entries.len() as f64
                };
                progress(ProgressUpdate::new(
                    format!(
                        "Downloaded and installed file {}/{} ({transfer})",
                        idx + 1,
                        entries.len()
                    ),
                    0.35 + file_fraction * 0.60,
                ));
            }

            let target = out_dir.join(&entry.path);
            if let Some(parent) = target.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(target, decoded).await?;
        }

        Ok(version)
    }

    async fn fetch_install_entry(
        &self,
        cdn: &RemoteCdn,
        archive_map: &archive::ArchiveMap,
        encoding_key: &str,
        content_key: &str,
        path: &str,
        verify: bool,
        has_archive: bool,
    ) -> Result<Vec<u8>> {
        if let Some(encoded) = cdn.fetch_data_optional_unverified(encoding_key).await? {
            if let Ok(decoded) =
                decode_install_entry(&encoded, encoding_key, content_key, path, verify)
            {
                cdn.cache_data(encoding_key, &encoded).await;
                return Ok(decoded);
            }
            cdn.remove_data_cache(encoding_key).await;
        }

        anyhow::ensure!(
            has_archive,
            "loose data missing or invalid for {path} ({encoding_key})"
        );
        let decoded = archive_map
            .fetch_file(cdn, encoding_key, verify)
            .await
            .with_context(|| format!("archive data missing for {path}"))?;
        verify_md5("installed file", &decoded, content_key, verify)?;
        Ok(decoded)
    }

    async fn fetch_encoding(
        &self,
        cdn: &RemoteCdn,
        build_config: &BuildConfig,
        verify: bool,
    ) -> Result<EncodingFile> {
        let pair = &build_config.encoding;
        anyhow::ensure!(
            !pair.content_key.is_empty(),
            "build config has no encoding content key"
        );
        anyhow::ensure!(
            !pair.encoding_key.is_empty(),
            "build config has no encoding data key"
        );
        let encoded = cdn.fetch_data(&pair.encoding_key, false).await?;
        let decoded = blte::decode(&encoded, &pair.encoding_key, false)?;
        EncodingFile::parse(&decoded, &pair.content_key, verify)
    }

    async fn fetch_install_manifest(
        &self,
        cdn: &RemoteCdn,
        build_config: &BuildConfig,
        encoding: &EncodingFile,
        verify: bool,
    ) -> Result<InstallFile> {
        let install = &build_config.install;
        let encoding_key = if !install.encoding_key.is_empty() {
            install.encoding_key.clone()
        } else {
            encoding
                .find_by_content_key(&install.content_key)
                .context("install manifest encoding key not found")?
                .to_string()
        };
        let encoded = cdn.fetch_data(&encoding_key, false).await?;
        let decoded = blte::decode(&encoded, &encoding_key, false)?;
        InstallFile::parse(&decoded, &install.content_key, verify)
    }
}

fn decode_install_entry(
    encoded: &[u8],
    encoding_key: &str,
    content_key: &str,
    path: &str,
    verify: bool,
) -> Result<Vec<u8>> {
    let decoded = blte::decode(encoded, encoding_key, false)
        .with_context(|| format!("failed to decode {path}"))?;
    verify_md5("installed file", &decoded, content_key, verify)
        .with_context(|| format!("failed to verify {path}"))?;
    Ok(decoded)
}

fn version_urls() -> Vec<String> {
    [Region::Us, Region::Eu, Region::Kr, Region::Cn]
        .into_iter()
        .map(|candidate| format!("{}/versions", candidate.remote_url()))
        .collect()
}

pub(crate) fn verify_md5(name: &str, data: &[u8], expected: &str, verify: bool) -> Result<()> {
    if verify {
        let actual = format!("{:x}", md5::compute(data));
        anyhow::ensure!(
            actual == expected,
            "{name} failed md5 verification: expected {expected}, got {actual}"
        );
    }
    Ok(())
}

pub(crate) fn partition_hash(hash: &str) -> Result<String> {
    anyhow::ensure!(hash.len() >= 4, "invalid hash `{hash}`");
    Ok(format!("{}/{}/{}", &hash[0..2], &hash[2..4], hash))
}

pub(crate) fn read_cstr(cursor: &mut std::io::Cursor<&[u8]>) -> Result<String> {
    use std::io::Read;
    let mut out = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if cursor.read(&mut byte)? == 0 || byte[0] == 0 {
            break;
        }
        out.push(byte[0]);
    }
    String::from_utf8(out).context("invalid utf-8 c-string")
}

pub(crate) fn read_be_u24(bytes: &[u8]) -> u32 {
    ((bytes[0] as u32) << 16) | ((bytes[1] as u32) << 8) | bytes[2] as u32
}
