use super::{
    archive, blte,
    cdn::RemoteCdn,
    check_cancelled,
    install_plan::{checked_install_path, InstallItem},
    local_manifest::InstalledFileRecord,
    verify_md5, ProgressUpdate,
};
use crate::{phrase, util};
use anyhow::{Context, Result};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::mpsc, task::JoinSet};
use tokio_util::sync::CancellationToken;
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
    cancel: Option<CancellationToken>,
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
    // Shown again when a file completes, so the status line keeps its speed
    // instead of dropping it between measurements.
    let mut last_speed = 0.0;
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
                    if elapsed > Duration::ZERO {
                        last_speed = speed_window_bytes as f64 / elapsed.as_secs_f64();
                    }
                    emit_install_progress(
                        progress,
                        completed_files,
                        total_files,
                        completed_bytes,
                        in_flight_bytes,
                        total_bytes,
                        last_speed,
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
                    last_speed,
                );
            }
        }
    }

    progress(ProgressUpdate::new(phrase!("ngdp.installed"), 0.95));
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
    progress(ProgressUpdate::new(
        phrase!(
            "ngdp.downloading",
            files = completed_files,
            total_files = total_files,
            done = util::format_bytes(visible_bytes as f64),
            total = util::format_bytes(total_bytes as f64),
            speed = util::format_speed_suffix(speed_bytes_per_second)
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
    let metadata = tokio::task::spawn_blocking(move || -> Result<std::fs::Metadata> {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, &decoded)?;
        std::fs::metadata(&target)
            .with_context(|| format!("failed to stat installed file {}", target.display()))
    })
    .await
    .map_err(|err| anyhow::anyhow!(err))??;
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
        let path_owned = path.to_string();
        let encoding_key_owned = encoding_key.to_string();
        let content_key_owned = content_key.to_string();
        let decoded = tokio::task::spawn_blocking(move || {
            decode_install_entry(
                &encoded,
                &encoding_key_owned,
                &content_key_owned,
                &path_owned,
                verify,
            )
            .map(|decoded| (decoded, encoded))
        })
        .await
        .map_err(|err| anyhow::anyhow!(err))?;
        if let Ok((decoded, encoded)) = decoded {
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
    let content_key = content_key.to_string();
    tokio::task::spawn_blocking(move || {
        verify_md5("installed file", &decoded, &content_key, verify)?;
        Ok(decoded)
    })
    .await
    .map_err(|err| anyhow::anyhow!(err))?
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

#[cfg(test)]
mod tests {
    use super::super::archive::ArchiveMap;
    use super::super::bytes::md5_hex;
    use super::super::cdn::RemoteCdn;
    use super::super::installfile::InstallEntry;
    use super::super::test_support::{blte_plain, blte_zlib};
    use super::{
        decode_install_entry, emit_install_progress, fetch_install_entry, install_entries_parallel,
        install_one_entry, InstallItem,
    };
    use std::path::Path;
    use tokio_util::sync::CancellationToken;

    fn ek(byte: u8) -> String {
        format!("{byte:02x}").repeat(16)
    }

    fn offline_cdn(cache: &Path) -> RemoteCdn {
        RemoteCdn::from_forced_url(reqwest::Client::new(), "http://127.0.0.1:9/tpr/hs")
            .unwrap()
            .with_cache_dir(cache)
    }

    fn item(
        path: &str,
        target: &str,
        encoding_key: &str,
        payload: &[u8],
        has_archive: bool,
    ) -> InstallItem {
        InstallItem {
            entry: InstallEntry {
                path: path.to_string(),
                content_key: md5_hex(payload),
                size: u32::try_from(payload.len()).unwrap(),
            },
            encoding_key: encoding_key.to_string(),
            target_path: target.to_string(),
            has_archive,
        }
    }

    #[test]
    fn decode_plain_blte_with_md5() {
        let encoded = blte_plain(b"hello");
        let decoded =
            decode_install_entry(&encoded, &ek(1), &md5_hex(b"hello"), "a.txt", true).unwrap();
        assert_eq!(decoded, b"hello");
    }

    #[test]
    fn decode_zlib_blte() {
        let payload = b"hello".repeat(100);
        let encoded = blte_zlib(&payload);
        let decoded =
            decode_install_entry(&encoded, &ek(1), &md5_hex(&payload), "a.txt", true).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn decode_md5_mismatch_mentions_path() {
        let encoded = blte_plain(b"hello");
        let error = decode_install_entry(&encoded, &ek(1), &"0".repeat(32), "Data/a.txt", true)
            .unwrap_err();
        assert!(format!("{error:#}").contains("failed to verify Data/a.txt"));
    }

    #[test]
    fn decode_bad_blte_magic_errors() {
        let error =
            decode_install_entry(b"BLTX....", &ek(1), &md5_hex(b"hello"), "Data/a.txt", false)
                .unwrap_err();
        assert!(format!("{error:#}").contains("failed to decode Data/a.txt"));
    }

    #[test]
    fn decode_verify_false_skips_md5() {
        let encoded = blte_plain(b"hello");
        let decoded =
            decode_install_entry(&encoded, &ek(1), &"0".repeat(32), "a.txt", false).unwrap();
        assert_eq!(decoded, b"hello");
    }

    #[tokio::test]
    async fn fetch_uses_cached_loose_data_offline() {
        let temp = tempfile::tempdir().unwrap();
        let cdn = offline_cdn(temp.path());
        let key = ek(1);
        let encoded = blte_plain(b"hello");
        cdn.cache_data(&key, &encoded).await;
        let decoded = fetch_install_entry(
            &cdn,
            &ArchiveMap::default(),
            &key,
            &md5_hex(b"hello"),
            "a.txt",
            true,
            true,
        )
        .await
        .unwrap();
        assert_eq!(decoded, b"hello");
        assert!(cdn
            .read_data_cache_unverified(&key)
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn fetch_corrupt_cache_removed_then_archive_miss_errors() {
        let temp = tempfile::tempdir().unwrap();
        let cdn = offline_cdn(temp.path());
        let key = ek(2);
        cdn.cache_data(&key, b"garbage").await;
        let error = fetch_install_entry(
            &cdn,
            &ArchiveMap::default(),
            &key,
            &md5_hex(b"hello"),
            "a.txt",
            true,
            true,
        )
        .await
        .unwrap_err();
        assert!(format!("{error:#}").contains("archive data missing for a.txt"));
        assert!(cdn
            .read_data_cache_unverified(&key)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn fetch_cache_miss_and_archive_miss_errors() {
        let temp = tempfile::tempdir().unwrap();
        let cdn = offline_cdn(temp.path());
        let error = fetch_install_entry(
            &cdn,
            &ArchiveMap::default(),
            &ek(3),
            &md5_hex(b"hello"),
            "a.txt",
            true,
            true,
        )
        .await
        .unwrap_err();
        assert!(format!("{error:#}").contains("archive data missing for a.txt"));
    }

    #[tokio::test]
    async fn parallel_installs_files_and_reports_progress() {
        let temp = tempfile::tempdir().unwrap();
        let cdn = offline_cdn(temp.path());
        let payloads = [b"one".to_vec(), b"two!".to_vec(), b"three".to_vec()];
        let mut items = Vec::new();
        for (index, payload) in payloads.iter().enumerate() {
            let key = ek(index as u8);
            cdn.cache_data(&key, &blte_plain(payload)).await;
            items.push(item(
                &format!("p{index}"),
                &format!("f{index}"),
                &key,
                payload,
                true,
            ));
        }
        let mut fractions = Vec::new();
        let installed = install_entries_parallel(
            cdn,
            ArchiveMap::default(),
            items.clone(),
            temp.path(),
            true,
            &mut |update| {
                if let Some(fraction) = update.fraction {
                    fractions.push(fraction);
                }
            },
            None,
        )
        .await
        .unwrap();
        assert_eq!(installed.len(), 3);
        for (index, item) in items.iter().enumerate() {
            let result = installed
                .iter()
                .find(|result| result.target_path == item.target_path)
                .unwrap();
            assert_eq!(result.bytes, u64::from(item.entry.size));
            assert_eq!(
                std::fs::read(temp.path().join(&item.target_path)).unwrap(),
                payloads[index]
            );
        }
        assert!(fractions.windows(2).all(|pair| pair[1] >= pair[0]));
        assert!(fractions
            .iter()
            .all(|fraction| (0.35..=0.95).contains(fraction)));
        assert_eq!(fractions.last().copied(), Some(0.95));
    }

    #[tokio::test]
    async fn parallel_more_items_than_concurrency() {
        let temp = tempfile::tempdir().unwrap();
        let cdn = offline_cdn(temp.path());
        let mut items = Vec::new();
        for index in 0..20u8 {
            let payload = vec![index];
            let key = ek(index);
            cdn.cache_data(&key, &blte_plain(&payload)).await;
            items.push(item(
                &format!("p{index}"),
                &format!("f{index}"),
                &key,
                &payload,
                true,
            ));
        }
        let installed = install_entries_parallel(
            cdn,
            ArchiveMap::default(),
            items,
            temp.path(),
            true,
            &mut |_| {},
            None,
        )
        .await
        .unwrap();
        assert_eq!(installed.len(), 20);
        let names: std::collections::BTreeSet<_> = installed
            .iter()
            .map(|result| result.target_path.clone())
            .collect();
        assert_eq!(names.len(), 20);
        for index in 0..20u8 {
            assert_eq!(
                std::fs::read(temp.path().join(format!("f{index}"))).unwrap(),
                vec![index]
            );
        }
    }

    #[tokio::test]
    async fn parallel_empty_list() {
        let temp = tempfile::tempdir().unwrap();
        let mut fractions = Vec::new();
        let installed = install_entries_parallel(
            offline_cdn(temp.path()),
            ArchiveMap::default(),
            Vec::new(),
            temp.path(),
            false,
            &mut |update| fractions.push(update.fraction),
            None,
        )
        .await
        .unwrap();
        assert!(installed.is_empty());
        assert_eq!(fractions, vec![Some(0.95)]);
    }

    #[tokio::test]
    async fn rejects_parent_dir_target_path() {
        let temp = tempfile::tempdir().unwrap();
        let out_dir = temp.path().join("out");
        std::fs::create_dir(&out_dir).unwrap();
        let cdn = offline_cdn(temp.path());
        let key = ek(1);
        cdn.cache_data(&key, &blte_plain(b"hello")).await;
        let error = install_one_entry(
            cdn,
            ArchiveMap::default(),
            item("a.txt", "../escape", &key, b"hello", true),
            out_dir,
            true,
        )
        .await
        .unwrap_err();
        assert!(format!("{error:#}").contains("unsafe install path"));
        assert!(!temp.path().join("escape").exists());
    }

    #[tokio::test]
    async fn rejects_absolute_target_path() {
        let temp = tempfile::tempdir().unwrap();
        let abs = std::env::temp_dir().join(format!("hs-abs-test-{}", std::process::id()));
        let cdn = offline_cdn(temp.path());
        let key = ek(1);
        cdn.cache_data(&key, &blte_plain(b"hello")).await;
        let error = install_one_entry(
            cdn,
            ArchiveMap::default(),
            item("a.txt", abs.to_str().unwrap(), &key, b"hello", true),
            temp.path().to_path_buf(),
            true,
        )
        .await
        .unwrap_err();
        assert!(format!("{error:#}").contains("must be relative"));
        assert!(!abs.exists());
    }

    #[tokio::test]
    async fn cancelled_before_start_returns_error() {
        let temp = tempfile::tempdir().unwrap();
        let out_dir = temp.path().join("out");
        std::fs::create_dir(&out_dir).unwrap();
        let token = CancellationToken::new();
        token.cancel();
        let error = install_entries_parallel(
            offline_cdn(temp.path()),
            ArchiveMap::default(),
            vec![item("a.txt", "a.txt", &ek(1), b"hello", true)],
            &out_dir,
            false,
            &mut |_| {},
            Some(token),
        )
        .await
        .unwrap_err();
        assert!(format!("{error:#}").contains("installation cancelled"));
        assert!(std::fs::read_dir(&out_dir).unwrap().next().is_none());
    }

    #[tokio::test]
    async fn one_bad_entry_fails_batch() {
        // B11: a failed entry returns the whole batch, and files that already
        // finished are left on disk. There is no rollback.
        let temp = tempfile::tempdir().unwrap();
        let cdn = offline_cdn(temp.path());
        let mut items = Vec::new();
        for index in 0..4u8 {
            let payload = vec![index];
            let key = ek(index);
            cdn.cache_data(&key, &blte_plain(&payload)).await;
            items.push(item(
                &format!("p{index}"),
                &format!("f{index}"),
                &key,
                &payload,
                true,
            ));
        }
        // The bad entry is uncached loose data. Fetching it from the closed
        // port retries for several seconds, so the four cached writes finish
        // and are still on disk when the batch returns (B11). A corrupt cache
        // hit fails too quickly for those writes to land.
        items.push(InstallItem {
            entry: InstallEntry {
                path: "bad".to_string(),
                content_key: md5_hex(b"unused"),
                size: 6,
            },
            encoding_key: ek(9),
            target_path: "bad".to_string(),
            has_archive: false,
        });
        let out_dir = temp.path().join("out");
        std::fs::create_dir(&out_dir).unwrap();
        install_entries_parallel(
            cdn,
            ArchiveMap::default(),
            items,
            &out_dir,
            true,
            &mut |_| {},
            None,
        )
        .await
        .unwrap_err();
        for index in 0..4u8 {
            assert_eq!(
                std::fs::read(out_dir.join(format!("f{index}"))).unwrap(),
                vec![index],
                "completed install was not left on disk"
            );
        }
        assert!(!out_dir.join("bad").exists());
    }

    #[tokio::test]
    async fn creates_nested_parent_dirs() {
        let temp = tempfile::tempdir().unwrap();
        let cdn = offline_cdn(temp.path());
        let key = ek(4);
        cdn.cache_data(&key, &blte_plain(b"nested")).await;
        let target = "Bin/Hearthstone_Data/Resources/x";
        install_one_entry(
            cdn,
            ArchiveMap::default(),
            item("x", target, &key, b"nested", true),
            temp.path().to_path_buf(),
            true,
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(temp.path().join(target)).unwrap(), b"nested");
    }

    #[test]
    fn emit_progress_clamps_to_total() {
        let mut fraction = None;
        emit_install_progress(
            &mut |update| fraction = update.fraction,
            1,
            2,
            80,
            40,
            100,
            0.0,
        );
        let over = fraction.unwrap();
        assert!((over - 0.95).abs() < 1e-9, "{over}");
        emit_install_progress(
            &mut |update| fraction = update.fraction,
            0,
            0,
            0,
            0,
            100,
            0.0,
        );
        assert_eq!(fraction, Some(0.35));
    }

    #[tokio::test]
    async fn overwrites_existing_target() {
        let temp = tempfile::tempdir().unwrap();
        let cdn = offline_cdn(temp.path());
        let key = ek(5);
        let target = temp.path().join("a.txt");
        std::fs::write(&target, b"old").unwrap();
        cdn.cache_data(&key, &blte_plain(b"new")).await;
        install_one_entry(
            cdn,
            ArchiveMap::default(),
            item("a.txt", "a.txt", &key, b"new", true),
            temp.path().to_path_buf(),
            false,
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(target).unwrap(), b"new");
    }

    // B4: RemoteCdn::from_forced_url rebuilds `scheme://host` and drops the
    // port, so this cannot target the fake server on 127.0.0.1:<port>.
    #[tokio::test]
    #[ignore = "B4: from_forced_url drops the URL port, so a local fake CDN is unreachable"]
    async fn loose_fetch_from_fake_cdn() {
        let server = crate::test_http::FakeServer::start(blte_plain(b"hello"), Default::default());
        let temp = tempfile::tempdir().unwrap();
        let base = format!("http://{}/tpr/hs", server.addr());
        let cdn = RemoteCdn::from_forced_url(reqwest::Client::new(), &base)
            .unwrap()
            .with_cache_dir(temp.path());
        let key = ek(6);
        let decoded = fetch_install_entry(
            &cdn,
            &ArchiveMap::default(),
            &key,
            &md5_hex(b"hello"),
            "a.txt",
            true,
            false,
        )
        .await
        .unwrap();
        assert_eq!(decoded, b"hello");

        server.set_status(404);
        let missing = fetch_install_entry(
            &cdn,
            &ArchiveMap::default(),
            &ek(7),
            &md5_hex(b"hello"),
            "missing.txt",
            false,
            false,
        )
        .await
        .unwrap_err();
        assert!(format!("{missing:#}").contains("loose data missing"));
    }
}
