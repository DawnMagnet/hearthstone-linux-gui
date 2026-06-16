use super::{
    archive, blte,
    cdn::RemoteCdn,
    check_cancelled,
    install_plan::{checked_install_path, InstallItem},
    local_manifest::InstalledFileRecord,
    verify_md5, ProgressUpdate,
};
use crate::util;
use anyhow::{Context, Result};
use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};
use tokio::{sync::mpsc, task::JoinSet};
use tracing::{trace, warn};

const INSTALL_FILE_CONCURRENCY: usize = 8;

#[derive(Clone, Debug)]
pub(super) struct InstalledEntryResult {
    pub(super) bytes: u64,
    pub(super) target_path: String,
    pub(super) record: InstalledFileRecord,
}

pub(super) async fn install_entries_parallel(
    cdn: RemoteCdn,
    archive_map: archive::ArchiveMap,
    entries: Vec<InstallItem>,
    out_dir: &Path,
    verify: bool,
    progress: &mut (impl FnMut(ProgressUpdate) + Send),
    cancel: Option<Arc<AtomicBool>>,
) -> Result<Vec<InstalledEntryResult>> {
    let total_files = entries.len();
    let total_bytes = entries
        .iter()
        .map(|item| u64::from(item.entry.size))
        .sum::<u64>()
        .max(1);
    let (byte_sender, mut byte_receiver) = mpsc::unbounded_channel::<u64>();
    let install_cdn = cdn.with_progress_callback(Arc::new(move |bytes| {
        let _ = byte_sender.send(bytes);
    }));
    let mut pending = entries.into_iter();
    let mut tasks = JoinSet::new();
    let mut active = 0usize;
    let mut completed_files = 0usize;
    let mut completed_bytes = 0u64;
    let mut in_flight_bytes = 0u64;
    let mut speed_window_start = Instant::now();
    let mut speed_window_bytes = 0u64;
    let mut last_progress = Instant::now() - Duration::from_secs(1);
    let mut installed = Vec::with_capacity(total_files);

    loop {
        while active < INSTALL_FILE_CONCURRENCY {
            let Some(item) = pending.next() else {
                break;
            };
            check_cancelled(cancel.as_ref())?;
            active += 1;
            let cdn = install_cdn.clone();
            let archive_map = archive_map.clone();
            let out_dir = out_dir.to_path_buf();
            tasks.spawn(
                async move { install_one_entry(cdn, archive_map, item, out_dir, verify).await },
            );
        }

        if active == 0 {
            break;
        }

        tokio::select! {
            Some(bytes) = byte_receiver.recv() => {
                in_flight_bytes = in_flight_bytes.saturating_add(bytes);
                speed_window_bytes = speed_window_bytes.saturating_add(bytes);
                let elapsed = speed_window_start.elapsed();
                if last_progress.elapsed() >= Duration::from_millis(250) {
                    let speed = if elapsed > Duration::ZERO {
                        speed_window_bytes as f64 / elapsed.as_secs_f64()
                    } else {
                        0.0
                    };
                    emit_install_progress(
                        progress,
                        completed_files,
                        total_files,
                        completed_bytes,
                        in_flight_bytes,
                        total_bytes,
                        speed,
                    );
                    if elapsed >= Duration::from_secs(1) {
                        speed_window_start = Instant::now();
                        speed_window_bytes = 0;
                    }
                    last_progress = Instant::now();
                }
            }
            Some(result) = tasks.join_next() => {
                active -= 1;
                let result = result??;
                let installed_bytes = result.bytes;
                completed_files += 1;
                completed_bytes = completed_bytes.saturating_add(installed_bytes);
                in_flight_bytes = in_flight_bytes.saturating_sub(installed_bytes);
                installed.push(result);
                emit_install_progress(
                    progress,
                    completed_files,
                    total_files,
                    completed_bytes,
                    in_flight_bytes,
                    total_bytes,
                    0.0,
                );
            }
        }
    }

    progress(ProgressUpdate::new("Installed Hearthstone files", 0.95));
    Ok(installed)
}

fn emit_install_progress(
    progress: &mut (impl FnMut(ProgressUpdate) + Send),
    completed_files: usize,
    total_files: usize,
    completed_bytes: u64,
    in_flight_bytes: u64,
    total_bytes: u64,
    speed_bytes_per_second: f64,
) {
    let visible_bytes = completed_bytes
        .saturating_add(in_flight_bytes)
        .min(total_bytes);
    let fraction = visible_bytes as f64 / total_bytes as f64;
    let speed = if speed_bytes_per_second > 0.0 {
        format!(" at {}/s", util::format_bytes(speed_bytes_per_second))
    } else {
        String::new()
    };
    progress(ProgressUpdate::new(
        format!(
            "Downloading Hearthstone: {}/{} files, {}/{}{speed}",
            completed_files,
            total_files,
            util::format_bytes(visible_bytes as f64),
            util::format_bytes(total_bytes as f64)
        ),
        0.35 + fraction * 0.60,
    ));
}

async fn install_one_entry(
    cdn: RemoteCdn,
    archive_map: archive::ArchiveMap,
    item: InstallItem,
    out_dir: PathBuf,
    verify: bool,
) -> Result<InstalledEntryResult> {
    trace!(
        path = %item.entry.path,
        target_path = %item.target_path,
        content_key = %item.entry.content_key,
        encoding_key = %item.encoding_key,
        size = item.entry.size,
        "installing entry"
    );
    let decoded = fetch_install_entry(
        &cdn,
        &archive_map,
        &item.encoding_key,
        &item.entry.content_key,
        &item.entry.path,
        verify,
        item.has_archive,
    )
    .await?;
    let target = checked_install_path(&out_dir, &item.target_path)?;
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&target, decoded).await?;
    let metadata = tokio::fs::metadata(&target)
        .await
        .with_context(|| format!("failed to stat installed file {}", target.display()))?;
    Ok(InstalledEntryResult {
        bytes: u64::from(item.entry.size),
        target_path: item.target_path,
        record: InstalledFileRecord::from_entry(&item.entry, &item.encoding_key, &metadata, verify),
    })
}

async fn fetch_install_entry(
    cdn: &RemoteCdn,
    archive_map: &archive::ArchiveMap,
    encoding_key: &str,
    content_key: &str,
    path: &str,
    verify: bool,
    has_archive: bool,
) -> Result<Vec<u8>> {
    trace!(
        path = %path,
        encoding_key = %encoding_key,
        content_key = %content_key,
        has_archive = has_archive,
        "fetching install entry"
    );
    let loose_data = if has_archive {
        cdn.read_data_cache_unverified(encoding_key).await?
    } else {
        cdn.fetch_data_optional_unverified(encoding_key).await?
    };

    if let Some(encoded) = loose_data {
        if let Ok(decoded) = decode_install_entry(&encoded, encoding_key, content_key, path, verify)
        {
            trace!(
                path = %path,
                encoding_key = %encoding_key,
                bytes = decoded.len(),
                "decoded loose data"
            );
            cdn.cache_data(encoding_key, &encoded).await;
            return Ok(decoded);
        }
        warn!(
            path = %path,
            encoding_key = %encoding_key,
            content_key = %content_key,
            "loose data existed but failed decode/verification; removing cache and trying archive"
        );
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
    trace!(
        path = %path,
        encoding_key = %encoding_key,
        bytes = decoded.len(),
        "decoded archive data"
    );
    verify_md5("installed file", &decoded, content_key, verify)?;
    Ok(decoded)
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
