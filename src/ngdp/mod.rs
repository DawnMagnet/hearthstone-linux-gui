pub mod archive;
pub mod blizini;
pub mod blte;
pub mod cdn;
pub mod configfile;
pub mod encoding;
mod install_exec;
mod install_plan;
pub mod installfile;
mod local_manifest;
pub mod psv;

use crate::{util, Locale, Region};
use anyhow::{Context, Result};
use cdn::RemoteCdn;
use configfile::{BuildConfig, CdnConfig};
use encoding::EncodingFile;
use install_exec::install_entries_parallel;
use install_plan::{installed_target_path, InstallItem};
use installfile::InstallFile;
use local_manifest::{cleanup_stale_installed_files, scan_local_install, InstalledManifest};
use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use tracing::{debug, info, trace, warn};

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
                .connect_timeout(Duration::from_secs(30))
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
                Ok(version) => {
                    info!(
                        region = %region,
                        source = %url,
                        version = %version.version_name,
                        build_id = %version.build_id,
                        "found latest version"
                    );
                    return Ok(version);
                }
                Err(error) => {
                    warn!(region = %region, source = %url, error = %format!("{error:#}"), "version metadata fetch failed");
                    errors.push(format!("{url}: {error:#}"));
                }
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
        debug!(region = %region, source = %url, bytes = text.len(), "read version metadata");
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
        self.install_latest_with_cancel(options, out_dir, &mut progress, None)
            .await
    }

    pub async fn install_latest_cancellable(
        &self,
        options: &InstallOptions,
        out_dir: &Path,
        mut progress: impl FnMut(ProgressUpdate) + Send,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<VersionInfo> {
        self.install_latest_with_cancel(options, out_dir, &mut progress, cancel)
            .await
    }

    async fn install_latest_with_cancel(
        &self,
        options: &InstallOptions,
        out_dir: &Path,
        progress: &mut (impl FnMut(ProgressUpdate) + Send),
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<VersionInfo> {
        check_cancelled(cancel.as_ref())?;
        info!(
            region = %options.region,
            locale = %options.locale,
            out_dir = %out_dir.display(),
            verify = options.verify,
            "starting NGDP install"
        );
        progress(ProgressUpdate::new(
            "Checking latest Hearthstone version",
            0.02,
        ));
        let version = self.latest_version(options.region).await?;
        check_cancelled(cancel.as_ref())?;

        let mut cdn = RemoteCdn::from_forced_url(self.http.clone(), options.region.default_cdn())?;
        if let Some(cache_dir) = &self.cache_dir {
            cdn = cdn.with_cache_dir(cache_dir);
        }
        if let Some(cancel) = &cancel {
            cdn = cdn.with_cancel_token(cancel.clone());
        }
        debug!(cdn = %options.region.default_cdn(), "configured CDN");

        progress(ProgressUpdate::new(
            format!("Fetching build config {}", version.build_config),
            0.06,
        ));
        let build_config = BuildConfig::parse(
            &cdn.fetch_config(&version.build_config, options.verify)
                .await?,
        )?;
        debug!(
            build = %version.build_config,
            build_name = %build_config.build_name,
            root = %build_config.root,
            install_content = %build_config.install.content_key,
            install_encoding = %build_config.install.encoding_key,
            encoding_content = %build_config.encoding.content_key,
            encoding_key = %build_config.encoding.encoding_key,
            "parsed build config"
        );
        check_cancelled(cancel.as_ref())?;

        progress(ProgressUpdate::new(
            format!("Fetching CDN config {}", version.cdn_config),
            0.10,
        ));
        let cdn_config = CdnConfig::parse(
            &cdn.fetch_config(&version.cdn_config, options.verify)
                .await?,
        )?;
        debug!(
            cdn_config = %version.cdn_config,
            archive_group = %cdn_config.archive_group,
            archive_count = cdn_config.archives.len(),
            "parsed CDN config"
        );
        check_cancelled(cancel.as_ref())?;

        progress(ProgressUpdate::new("Fetching encoding table", 0.14));
        let encoding = self
            .fetch_encoding(&cdn, &build_config, options.verify)
            .await?;
        check_cancelled(cancel.as_ref())?;

        progress(ProgressUpdate::new("Fetching install manifest", 0.18));
        let install = self
            .fetch_install_manifest(&cdn, &build_config, &encoding, options.verify)
            .await?;
        check_cancelled(cancel.as_ref())?;

        let tags = ["OSX", options.locale.as_str(), "Production"];
        let entries = install.filter_entries(&tags)?;
        info!(
            tags = ?tags,
            entries = entries.len(),
            "filtered install manifest"
        );
        progress(ProgressUpdate::new(
            format!("Checking {} local files", entries.len()),
            0.22,
        ));

        let previous_manifest = InstalledManifest::load(out_dir).await?;
        let mut pending = Vec::with_capacity(entries.len());
        for entry in entries {
            let encoding_key = encoding
                .find_by_content_key(&entry.content_key)
                .with_context(|| format!("encoding key not found for {}", entry.path))?;
            let Some(target_path) = installed_target_path(&entry.path) else {
                trace!(
                    path = %entry.path,
                    content_key = %entry.content_key,
                    encoding_key,
                    "skipping macOS-only install entry"
                );
                continue;
            };
            trace!(
                path = %entry.path,
                target_path = %target_path,
                content_key = %entry.content_key,
                encoding_key,
                "resolved install entry encoding key"
            );
            pending.push(InstallItem {
                encoding_key: encoding_key.to_string(),
                target_path,
                entry,
                has_archive: false,
            });
        }

        let mut install_scan = scan_local_install(
            out_dir,
            pending,
            &previous_manifest,
            options.verify,
            cancel.as_ref(),
        )
        .await?;
        info!(
            fast_hits = install_scan.fast_hits,
            verified_hits = install_scan.verified_hits,
            missing = install_scan.missing.len(),
            "checked local install files"
        );

        if install_scan.missing.is_empty() {
            progress(ProgressUpdate::new(
                "All Hearthstone files are already present",
                0.95,
            ));
            let manifest = InstalledManifest::for_version(&version, options, install_scan.records);
            let desired_paths = manifest.desired_paths();
            manifest.save(out_dir).await?;
            cleanup_stale_installed_files(out_dir, &previous_manifest, &desired_paths).await?;
            return Ok(version);
        }

        progress(ProgressUpdate::new(
            format!("Downloading {} changed files", install_scan.missing.len()),
            0.24,
        ));
        let archive_map =
            archive::ArchiveMap::load(&cdn, &cdn_config, options.verify, |message, fraction| {
                progress(ProgressUpdate::new(
                    message,
                    fraction.map(|value| 0.24 + value * 0.11),
                ))
            })
            .await?;
        check_cancelled(cancel.as_ref())?;

        for item in &mut install_scan.missing {
            item.has_archive = archive_map.contains(&item.encoding_key);
            trace!(
                path = %item.entry.path,
                target_path = %item.target_path,
                content_key = %item.entry.content_key,
                encoding_key = %item.encoding_key,
                in_archive = item.has_archive,
                "queued install entry"
            );
        }
        let work = std::mem::take(&mut install_scan.missing);

        let installed = install_entries_parallel(
            cdn,
            archive_map,
            work,
            out_dir,
            options.verify,
            progress,
            cancel.clone(),
        )
        .await?;
        for item in installed {
            install_scan.records.insert(item.target_path, item.record);
        }
        let manifest = InstalledManifest::for_version(&version, options, install_scan.records);
        let desired_paths = manifest.desired_paths();
        manifest.save(out_dir).await?;
        cleanup_stale_installed_files(out_dir, &previous_manifest, &desired_paths).await?;

        Ok(version)
    }
}

impl NgdpClient {
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
        debug!(encoding_key = %pair.encoding_key, bytes = encoded.len(), "fetched encoded encoding table");
        let decoded = blte::decode(&encoded, &pair.encoding_key, false)?;
        debug!(content_key = %pair.content_key, bytes = decoded.len(), "decoded encoding table");
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
        debug!(
            encoding_key = %encoding_key,
            bytes = encoded.len(),
            "fetched encoded install manifest"
        );
        let decoded = blte::decode(&encoded, &encoding_key, false)?;
        debug!(content_key = %install.content_key, bytes = decoded.len(), "decoded install manifest");
        InstallFile::parse(&decoded, &install.content_key, verify)
    }
}

fn check_cancelled(cancel: Option<&Arc<AtomicBool>>) -> Result<()> {
    if let Err(error) = util::check_cancelled(cancel, "installation cancelled") {
        warn!("NGDP install cancelled");
        return Err(error);
    }
    Ok(())
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
