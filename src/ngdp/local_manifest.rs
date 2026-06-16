use super::{
    check_cancelled,
    install_plan::{checked_install_path, InstallItem},
    installfile::InstallEntry,
    InstallOptions, VersionInfo,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs::Metadata,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
    time::UNIX_EPOCH,
};
use tracing::{debug, warn};

const INSTALLED_MANIFEST_NAME: &str = ".ngdp-installed.json";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct InstalledManifest {
    version_name: String,
    build_id: String,
    region: String,
    locale: String,
    files: HashMap<String, InstalledFileRecord>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct InstalledFileRecord {
    content_key: String,
    encoding_key: String,
    size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    modified_ns: Option<u64>,
    verified: bool,
}

impl InstalledFileRecord {
    pub(super) fn from_entry(
        entry: &InstallEntry,
        encoding_key: &str,
        metadata: &Metadata,
        verified: bool,
    ) -> Self {
        installed_file_record(entry, encoding_key, metadata, verified)
    }
}

#[derive(Clone, Debug)]
pub(super) struct LocalInstallScan {
    pub(super) missing: Vec<InstallItem>,
    pub(super) records: HashMap<String, InstalledFileRecord>,
    pub(super) fast_hits: usize,
    pub(super) verified_hits: usize,
}

impl InstalledManifest {
    pub(super) fn for_version(
        version: &VersionInfo,
        options: &InstallOptions,
        files: HashMap<String, InstalledFileRecord>,
    ) -> Self {
        Self {
            version_name: version.version_name.clone(),
            build_id: version.build_id.clone(),
            region: options.region.to_string(),
            locale: options.locale.to_string(),
            files,
        }
    }

    pub(super) async fn load(out_dir: &Path) -> Result<Self> {
        let path = installed_manifest_path(out_dir);
        match tokio::fs::read(&path).await {
            Ok(data) => match serde_json::from_slice(&data) {
                Ok(manifest) => Ok(manifest),
                Err(error) => {
                    warn!(
                        path = %path.display(),
                        error = %error,
                        "installed file manifest is invalid; ignoring it"
                    );
                    Ok(Self::default())
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error)
                .with_context(|| format!("failed to read installed manifest {}", path.display())),
        }
    }

    pub(super) async fn save(&self, out_dir: &Path) -> Result<()> {
        tokio::fs::create_dir_all(out_dir).await?;
        let path = installed_manifest_path(out_dir);
        let temp = path.with_extension("json.tmp");
        let data = serde_json::to_vec_pretty(self)?;
        tokio::fs::write(&temp, data)
            .await
            .with_context(|| format!("failed to write installed manifest {}", temp.display()))?;
        tokio::fs::rename(&temp, &path)
            .await
            .with_context(|| format!("failed to update installed manifest {}", path.display()))?;
        Ok(())
    }

    pub(super) fn desired_paths(&self) -> HashSet<String> {
        self.files.keys().cloned().collect()
    }
}

pub(super) async fn scan_local_install(
    out_dir: &Path,
    entries: Vec<InstallItem>,
    manifest: &InstalledManifest,
    verify: bool,
    cancel: Option<&Arc<AtomicBool>>,
) -> Result<LocalInstallScan> {
    let mut missing = Vec::new();
    let mut records = HashMap::with_capacity(entries.len());
    let mut fast_hits = 0usize;
    let mut verified_hits = 0usize;

    for item in entries {
        check_cancelled(cancel)?;
        let target = checked_install_path(out_dir, &item.target_path)?;
        let metadata = match tokio::fs::metadata(&target).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(item);
                continue;
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to stat installed file {}", target.display())
                });
            }
        };

        if !metadata.is_file() || metadata.len() != u64::from(item.entry.size) {
            missing.push(item);
            continue;
        }

        let cached = manifest.files.get(&item.target_path);
        let cached_matches = cached.is_some_and(|record| {
            record.content_key == item.entry.content_key
                && record.encoding_key == item.encoding_key
                && record.size == u64::from(item.entry.size)
        });
        let modified_ns = metadata_modified_ns(&metadata);
        if cached_matches {
            let record = cached.expect("checked above");
            let modified_matches =
                record.modified_ns.is_some() && record.modified_ns == modified_ns;
            if (!verify || record.verified) && modified_matches {
                records.insert(item.target_path, record.clone());
                fast_hits += 1;
                continue;
            }
        }

        if !verify {
            records.insert(
                item.target_path,
                installed_file_record(&item.entry, &item.encoding_key, &metadata, false),
            );
            fast_hits += 1;
            continue;
        }

        let actual = file_md5_hex(&target).await?;
        if actual == item.entry.content_key {
            records.insert(
                item.target_path,
                installed_file_record(&item.entry, &item.encoding_key, &metadata, true),
            );
            verified_hits += 1;
        } else {
            debug!(
                path = %target.display(),
                expected = %item.entry.content_key,
                actual = %actual,
                "installed file content changed; scheduling download"
            );
            missing.push(item);
        }
    }

    Ok(LocalInstallScan {
        missing,
        records,
        fast_hits,
        verified_hits,
    })
}

pub(super) async fn cleanup_stale_installed_files(
    out_dir: &Path,
    previous_manifest: &InstalledManifest,
    desired_paths: &HashSet<String>,
) -> Result<()> {
    for target_path in previous_manifest.files.keys() {
        if desired_paths.contains(target_path) {
            continue;
        }
        let target = match checked_install_path(out_dir, target_path) {
            Ok(target) => target,
            Err(error) => {
                warn!(
                    path = %target_path,
                    error = %format!("{error:#}"),
                    "skipping unsafe stale NGDP-managed path"
                );
                continue;
            }
        };
        match tokio::fs::remove_file(&target).await {
            Ok(()) => {
                debug!(path = %target.display(), "removed stale NGDP-managed file");
                prune_empty_parents(out_dir, target.parent()).await;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                warn!(
                    path = %target.display(),
                    error = %error,
                    "failed to remove stale NGDP-managed file"
                );
            }
        }
    }
    Ok(())
}

async fn prune_empty_parents(root: &Path, mut current: Option<&Path>) {
    while let Some(path) = current {
        if path == root {
            break;
        }
        match tokio::fs::remove_dir(path).await {
            Ok(()) => current = path.parent(),
            Err(_) => break,
        }
    }
}

async fn file_md5_hex(path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .with_context(|| format!("failed to open installed file {}", path.display()))?;
    let mut context = md5::Context::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = tokio::io::AsyncReadExt::read(&mut file, &mut buffer)
            .await
            .with_context(|| format!("failed to read installed file {}", path.display()))?;
        if read == 0 {
            break;
        }
        context.consume(&buffer[..read]);
    }
    Ok(format!("{:x}", context.compute()))
}

fn installed_file_record(
    entry: &InstallEntry,
    encoding_key: &str,
    metadata: &Metadata,
    verified: bool,
) -> InstalledFileRecord {
    InstalledFileRecord {
        content_key: entry.content_key.clone(),
        encoding_key: encoding_key.to_string(),
        size: u64::from(entry.size),
        modified_ns: metadata_modified_ns(metadata),
        verified,
    }
}

fn metadata_modified_ns(metadata: &Metadata) -> Option<u64> {
    let modified = metadata.modified().ok()?;
    let duration = modified.duration_since(UNIX_EPOCH).ok()?;
    u64::try_from(duration.as_nanos()).ok()
}

fn installed_manifest_path(out_dir: &Path) -> PathBuf {
    out_dir.join(INSTALLED_MANIFEST_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ngdp::install_plan::installed_target_path;

    #[tokio::test]
    async fn scan_local_install_skips_manifest_verified_file_without_hashing() {
        let temp = tempfile::tempdir().unwrap();
        let target_path =
            installed_target_path("Hearthstone.app/Contents/Resources/Data/level0").unwrap();
        let target = temp.path().join(&target_path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, b"level").unwrap();

        let entry = InstallEntry {
            path: "Hearthstone.app/Contents/Resources/Data/level0".to_string(),
            content_key: format!("{:x}", md5::compute(b"level")),
            size: 5,
        };
        let metadata = std::fs::metadata(&target).unwrap();
        let record = installed_file_record(&entry, "encoding-key", &metadata, true);
        let manifest = InstalledManifest {
            files: HashMap::from([(target_path.clone(), record)]),
            ..InstalledManifest::default()
        };

        let scan = scan_local_install(
            temp.path(),
            vec![InstallItem {
                entry,
                encoding_key: "encoding-key".to_string(),
                target_path,
                has_archive: false,
            }],
            &manifest,
            true,
            None,
        )
        .await
        .unwrap();

        assert!(scan.missing.is_empty());
        assert_eq!(scan.fast_hits, 1);
        assert_eq!(scan.verified_hits, 0);
        assert_eq!(scan.records.len(), 1);
    }

    #[tokio::test]
    async fn scan_local_install_verifies_file_when_manifest_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        let target_path =
            installed_target_path("Hearthstone.app/Contents/Resources/Data/level0").unwrap();
        let target = temp.path().join(&target_path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, b"level").unwrap();

        let entry = InstallEntry {
            path: "Hearthstone.app/Contents/Resources/Data/level0".to_string(),
            content_key: format!("{:x}", md5::compute(b"level")),
            size: 5,
        };
        let scan = scan_local_install(
            temp.path(),
            vec![InstallItem {
                entry,
                encoding_key: "encoding-key".to_string(),
                target_path,
                has_archive: false,
            }],
            &InstalledManifest::default(),
            true,
            None,
        )
        .await
        .unwrap();

        assert!(scan.missing.is_empty());
        assert_eq!(scan.fast_hits, 0);
        assert_eq!(scan.verified_hits, 1);
        assert_eq!(scan.records.len(), 1);
    }
}
