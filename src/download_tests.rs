use super::{
    assemble_chunks, chunk_download_dir, chunk_path, chunks_for_total, completed_chunk_bytes,
    content_range_total, download_file, download_range_to_vec, download_to_vec, file_len,
    partial_download_path, DownloadChunk, DownloadProgress, CHUNK_SIZE,
};
use crate::test_http::{Behavior, FakeServer};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_RANGE};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio_util::sync::CancellationToken;

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

fn pattern(len: usize) -> Vec<u8> {
    (0..len).map(|index| ((index * 31) % 251) as u8).collect()
}

fn http_client_progress() -> (Arc<AtomicU64>, Arc<dyn Fn(u64) + Send + Sync>) {
    let sum = Arc::new(AtomicU64::new(0));
    let seen = Arc::clone(&sum);
    let progress: Arc<dyn Fn(u64) + Send + Sync> = Arc::new(move |bytes| {
        seen.fetch_add(bytes, Ordering::Relaxed);
    });
    (sum, progress)
}

fn updates() -> (
    Arc<Mutex<Vec<DownloadProgress>>>,
    impl FnMut(DownloadProgress) + Send,
) {
    let saved = Arc::new(Mutex::new(Vec::new()));
    let slot = Arc::clone(&saved);
    (saved, move |update| slot.lock().unwrap().push(update))
}

async fn expect_err(result: anyhow::Result<impl std::fmt::Debug>) -> String {
    format!("{:#}", result.unwrap_err())
}

#[test]
fn chunks_exact_multiple() {
    let chunks = chunks_for_total(16 * 1024 * 1024);
    assert_eq!(chunks.len(), 2);
    assert_eq!(
        (chunks[0].index, chunks[0].start, chunks[0].end),
        (0, 0, CHUNK_SIZE - 1)
    );
    assert_eq!(
        (chunks[1].index, chunks[1].start, chunks[1].end),
        (1, CHUNK_SIZE, CHUNK_SIZE * 2 - 1)
    );
}

#[test]
fn chunks_with_remainder() {
    let total = 16 * 1024 * 1024 + 1;
    let chunks = chunks_for_total(total);
    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[2].start, 16 * 1024 * 1024);
    assert_eq!(chunks[2].end, 16 * 1024 * 1024);
    assert_eq!(chunks[2].start, chunks[2].end);
}

#[test]
fn chunks_zero_and_one_byte() {
    assert!(chunks_for_total(0).is_empty());
    let one = chunks_for_total(1);
    assert_eq!(one.len(), 1);
    assert_eq!((one[0].index, one[0].start, one[0].end), (0, 0, 0));
}

#[test]
fn content_range_total_parsing() {
    let header = |value: Option<&str>| {
        let mut headers = HeaderMap::new();
        if let Some(value) = value {
            headers.insert(CONTENT_RANGE, HeaderValue::from_str(value).unwrap());
        }
        headers
    };
    assert_eq!(
        content_range_total(&header(Some("bytes 100-199/1000"))),
        Some(1000)
    );
    assert_eq!(content_range_total(&header(Some("bytes 0-1/*"))), None);
    assert_eq!(content_range_total(&header(None)), None);
    assert_eq!(content_range_total(&header(Some("garbage"))), None);
}

#[test]
fn partial_path_keeps_extension() {
    assert_eq!(
        partial_download_path(Path::new("a/Unity.tar.xz")),
        Path::new("a/Unity.tar.xz.part")
    );
    assert_eq!(
        partial_download_path(Path::new("a/file")),
        Path::new("a/file.part")
    );
}

#[test]
fn chunk_dir_and_chunk_path() {
    let dir = chunk_download_dir(Path::new("x.tar.xz"));
    assert_eq!(dir, Path::new("x.tar.xz.parts"));
    assert_eq!(chunk_path(&dir, 7), Path::new("x.tar.xz.parts/000007.part"));
}

#[test]
fn progress_fraction() {
    let progress = |downloaded, total| DownloadProgress {
        downloaded,
        total,
        speed_bytes_per_second: 0.0,
        resumed: false,
    };
    assert_eq!(progress(1, None).fraction(), None);
    assert_eq!(progress(1, Some(0)).fraction(), None);
    assert_eq!(progress(50, Some(100)).fraction(), Some(0.5));
    assert_eq!(progress(150, Some(100)).fraction(), Some(1.0));
}

#[tokio::test]
async fn completed_chunk_bytes_counts_and_prunes() {
    let temp = tempfile::tempdir().unwrap();
    let chunks = chunks_for_total(CHUNK_SIZE * 2);
    let good = chunk_path(temp.path(), 0);
    let bad = chunk_path(temp.path(), 1);
    let file = std::fs::File::create(&good).unwrap();
    file.set_len(CHUNK_SIZE).unwrap();
    std::fs::write(&bad, [1u8; 10]).unwrap();

    let counted = completed_chunk_bytes(temp.path(), &chunks).await.unwrap();
    assert_eq!(counted, CHUNK_SIZE);
    assert!(good.exists());
    assert!(!bad.exists());
}

#[tokio::test]
async fn file_len_missing_is_zero() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("absent");
    assert_eq!(file_len(&missing).await.unwrap(), 0);
}

#[tokio::test]
async fn assemble_chunks_concatenates_in_index_order() {
    let temp = tempfile::tempdir().unwrap();
    let chunk_dir = temp.path().join("chunks");
    std::fs::create_dir(&chunk_dir).unwrap();
    let chunks = vec![
        DownloadChunk {
            index: 0,
            start: 0,
            end: 2,
        },
        DownloadChunk {
            index: 1,
            start: 3,
            end: 5,
        },
        DownloadChunk {
            index: 2,
            start: 6,
            end: 8,
        },
    ];
    std::fs::write(chunk_path(&chunk_dir, 2), b"CCC").unwrap();
    std::fs::write(chunk_path(&chunk_dir, 0), b"AAA").unwrap();
    std::fs::write(chunk_path(&chunk_dir, 1), b"BBB").unwrap();
    let destination = temp.path().join("out.bin");
    assemble_chunks(&chunk_dir, &chunks, &destination)
        .await
        .unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), b"AAABBBCCC");
    assert!(!partial_download_path(&destination).exists());
}

#[tokio::test]
async fn to_vec_single_small() {
    let body = pattern(1024);
    let behavior = Behavior {
        accept_ranges: false,
        ..Behavior::default()
    };
    let server = FakeServer::start(body.clone(), behavior);
    let (sum, progress) = http_client_progress();
    let downloaded = download_to_vec(&client(), server.url(), None, Some(progress))
        .await
        .unwrap();
    assert_eq!(downloaded, body);
    assert_eq!(sum.load(Ordering::Relaxed), 1024);
}

#[tokio::test]
async fn to_vec_parallel_large() {
    let body = pattern(16 * 1024 * 1024 + 123);
    let server = FakeServer::start(body.clone(), Behavior::default());
    let downloaded = download_to_vec(&client(), server.url(), None, None)
        .await
        .unwrap();
    assert_eq!(downloaded, body);
    let ranges = server
        .log()
        .into_iter()
        .filter(|entry| entry.method == "GET" && entry.range.is_some())
        .count();
    assert!(ranges >= 3, "range gets: {ranges}");
    assert!(
        server.max_inflight() <= 8,
        "inflight {}",
        server.max_inflight()
    );
}

#[tokio::test]
async fn to_vec_parallel_falls_back_when_range_ignored() {
    let body = pattern(16 * 1024 * 1024 + 123);
    let behavior = Behavior {
        ignore_range: true,
        ..Behavior::default()
    };
    let server = FakeServer::start(body.clone(), behavior);
    let (sum, progress) = http_client_progress();
    let downloaded = download_to_vec(&client(), server.url(), None, Some(progress))
        .await
        .unwrap();
    assert_eq!(downloaded, body);
    // A 200 response is rejected before the body is read, so the failed
    // parallel attempts add nothing. Progress matches the fallback transfer.
    assert_eq!(sum.load(Ordering::Relaxed), body.len() as u64);

    // B8 shows up when a range response is consumed and then rejected: those
    // bytes stay counted, and the single-connection fallback counts the body
    // again.
    let behavior = Behavior {
        range_body_limit: Some(1024),
        ..Behavior::default()
    };
    let server = FakeServer::start(body.clone(), behavior);
    let (sum, progress) = http_client_progress();
    let downloaded = download_to_vec(&client(), server.url(), None, Some(progress))
        .await
        .unwrap();
    assert_eq!(downloaded, body);
    let progress_bytes = sum.load(Ordering::Relaxed);
    assert!(
        progress_bytes > body.len() as u64,
        "expected short ranged reads to leave progress above the body (B8), got {progress_bytes}"
    );
}

#[tokio::test]
async fn to_vec_head_405_uses_single() {
    let body = pattern(64);
    let behavior = Behavior {
        head_status: 405,
        ..Behavior::default()
    };
    let server = FakeServer::start(body.clone(), behavior);
    let downloaded = download_to_vec(&client(), server.url(), None, None)
        .await
        .unwrap();
    assert_eq!(downloaded, body);
    assert!(server.log().iter().all(|entry| entry.range.is_none()));
}

#[tokio::test]
async fn to_vec_404_is_error() {
    let behavior = Behavior {
        head_status: 404,
        get_status: 404,
        ..Behavior::default()
    };
    let server = FakeServer::start(b"missing".to_vec(), behavior);
    assert!(download_to_vec(&client(), server.url(), None, None)
        .await
        .is_err());
}

#[tokio::test]
async fn range_exact_bytes() {
    let body = b"0123456789abcdefghij".to_vec();
    let server = FakeServer::start(body.clone(), Behavior::default());
    let downloaded = download_range_to_vec(&client(), server.url(), 10, 5, None, None)
        .await
        .unwrap();
    assert_eq!(downloaded, b"abcde");
    let ranges: Vec<_> = server
        .log()
        .into_iter()
        .filter_map(|entry| entry.range)
        .collect();
    assert!(
        ranges
            .iter()
            .any(|range| range.replace(' ', "") == "bytes=10-14"),
        "{ranges:?}"
    );
}

#[tokio::test]
async fn range_zero_size_sends_nothing() {
    let server = FakeServer::start(b"abc".to_vec(), Behavior::default());
    let downloaded = download_range_to_vec(&client(), server.url(), 0, 0, None, None)
        .await
        .unwrap();
    assert!(downloaded.is_empty());
    assert!(server.log().is_empty());
}

#[tokio::test]
async fn range_overflow_is_error() {
    let server = FakeServer::start(b"abc".to_vec(), Behavior::default());
    let message =
        expect_err(download_range_to_vec(&client(), server.url(), u64::MAX, 2, None, None).await)
            .await;
    assert!(message.contains("overflows"), "{message}");
    assert!(server.log().is_empty());
}

#[tokio::test]
async fn range_short_body_is_error() {
    let behavior = Behavior {
        range_body_limit: Some(3),
        ..Behavior::default()
    };
    let server = FakeServer::start(b"0123456789".to_vec(), behavior);
    let message =
        expect_err(download_range_to_vec(&client(), server.url(), 0, 5, None, None).await).await;
    assert!(
        message.contains("returned 3 bytes, expected 5"),
        "{message}"
    );
}

#[tokio::test]
async fn range_200_is_error() {
    let behavior = Behavior {
        ignore_range: true,
        ..Behavior::default()
    };
    let server = FakeServer::start(pattern(32), behavior);
    let message =
        expect_err(download_range_to_vec(&client(), server.url(), 1, 4, None, None).await).await;
    assert!(message.contains("did not honor"), "{message}");
}

#[tokio::test]
async fn file_single_fresh() {
    let body = pattern(1000);
    let behavior = Behavior {
        accept_ranges: false,
        ..Behavior::default()
    };
    let server = FakeServer::start(body.clone(), behavior);
    let temp = tempfile::tempdir().unwrap();
    let destination = temp.path().join("file.bin");
    let (saved, progress) = updates();
    download_file(
        &client(),
        server.url().as_str(),
        &destination,
        None,
        progress,
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), body);
    assert!(!partial_download_path(&destination).exists());
    let last = saved.lock().unwrap().last().cloned().unwrap();
    assert_eq!(last.downloaded, 1000);
    assert_eq!(last.total, Some(1000));
}

#[tokio::test]
async fn file_resumes_from_partial() {
    let body = pattern(1000);
    let server = FakeServer::start(body.clone(), Behavior::default());
    let temp = tempfile::tempdir().unwrap();
    let destination = temp.path().join("file.bin");
    std::fs::write(partial_download_path(&destination), &body[..400]).unwrap();
    let (saved, progress) = updates();
    download_file(
        &client(),
        server.url().as_str(),
        &destination,
        None,
        progress,
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), body);
    let log = server.log();
    assert!(log.iter().any(|entry| {
        entry
            .range
            .as_deref()
            .is_some_and(|range| range.replace(' ', "") == "bytes=400-")
    }));
    assert!(saved.lock().unwrap().iter().all(|update| update.resumed));
}

#[tokio::test]
async fn file_restarts_when_range_ignored() {
    let body = pattern(1000);
    let behavior = Behavior {
        ignore_range: true,
        ..Behavior::default()
    };
    let server = FakeServer::start(body.clone(), behavior);
    let temp = tempfile::tempdir().unwrap();
    let destination = temp.path().join("file.bin");
    std::fs::write(partial_download_path(&destination), &body[..400]).unwrap();
    download_file(&client(), server.url().as_str(), &destination, None, |_| {})
        .await
        .unwrap();
    let stored = std::fs::read(&destination).unwrap();
    assert_eq!(stored.len(), 1000);
    assert_eq!(stored, body);
}

#[tokio::test]
async fn file_416_restarts_from_zero() {
    let body = pattern(1000);
    let behavior = Behavior {
        reject_ranges: true,
        ..Behavior::default()
    };
    let server = FakeServer::start(body.clone(), behavior);
    let temp = tempfile::tempdir().unwrap();
    let destination = temp.path().join("file.bin");
    std::fs::write(partial_download_path(&destination), &body[..400]).unwrap();
    download_file(&client(), server.url().as_str(), &destination, None, |_| {})
        .await
        .unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), body);
    let gets: Vec<_> = server
        .log()
        .into_iter()
        .filter(|entry| entry.method == "GET")
        .collect();
    assert!(gets.iter().any(|entry| entry.range.is_some()));
    assert!(gets.iter().any(|entry| entry.range.is_none()));
}

#[tokio::test]
async fn file_partial_larger_than_total_discarded() {
    let body = pattern(1000);
    let server = FakeServer::start(body.clone(), Behavior::default());
    let temp = tempfile::tempdir().unwrap();
    let destination = temp.path().join("file.bin");
    std::fs::write(partial_download_path(&destination), vec![0xAB; 1200]).unwrap();
    download_file(&client(), server.url().as_str(), &destination, None, |_| {})
        .await
        .unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), body);
    let gets: Vec<_> = server
        .log()
        .into_iter()
        .filter(|entry| entry.method == "GET")
        .collect();
    assert!(gets.iter().all(|entry| entry.range.is_none()));
}

#[tokio::test]
async fn file_partial_equal_total_renamed_without_get() {
    let body = pattern(1000);
    let server = FakeServer::start(body.clone(), Behavior::default());
    let temp = tempfile::tempdir().unwrap();
    let destination = temp.path().join("file.bin");
    std::fs::write(partial_download_path(&destination), &body).unwrap();
    let (saved, progress) = updates();
    download_file(
        &client(),
        server.url().as_str(),
        &destination,
        None,
        progress,
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), body);
    let methods: Vec<_> = server.log().into_iter().map(|entry| entry.method).collect();
    assert!(!methods.is_empty());
    assert!(methods.iter().all(|method| method == "HEAD"), "{methods:?}");
    assert!(saved.lock().unwrap().iter().all(|update| update.resumed));
}

#[tokio::test]
async fn file_interrupted_then_resumed() {
    let body = pattern(1000);
    let behavior = Behavior {
        cut_first_get_after: Some(500),
        ..Behavior::default()
    };
    let server = FakeServer::start(body.clone(), behavior);
    let temp = tempfile::tempdir().unwrap();
    let destination = temp.path().join("file.bin");
    let first = download_file(&client(), server.url().as_str(), &destination, None, |_| {}).await;
    assert!(first.is_err());
    assert!(!destination.exists());
    let partial = partial_download_path(&destination);
    let kept = std::fs::metadata(&partial).unwrap().len();
    assert!(kept > 0 && kept <= 500, "partial length {kept}");
    assert_eq!(std::fs::read(&partial).unwrap(), body[..kept as usize]);

    server.clear_log();
    download_file(&client(), server.url().as_str(), &destination, None, |_| {})
        .await
        .unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), body);
    let ranges: Vec<_> = server
        .log()
        .into_iter()
        .filter_map(|entry| entry.range)
        .collect();
    assert!(
        ranges
            .iter()
            .any(|range| range.starts_with(&format!("bytes={kept}-"))),
        "{ranges:?}"
    );
}

#[tokio::test]
async fn file_existing_destination_skips_network() {
    // B7: an existing destination is treated as complete with no size or hash check.
    let server = FakeServer::start(pattern(1000), Behavior::default());
    let temp = tempfile::tempdir().unwrap();
    let destination = temp.path().join("file.bin");
    std::fs::write(&destination, b"already here").unwrap();
    let (saved, progress) = updates();
    download_file(
        &client(),
        server.url().as_str(),
        &destination,
        None,
        progress,
    )
    .await
    .unwrap();
    assert!(server.log().is_empty());
    let update = saved.lock().unwrap().last().cloned().unwrap();
    assert_eq!(update.downloaded, b"already here".len() as u64);
    assert_eq!(std::fs::read(&destination).unwrap(), b"already here");
}

#[tokio::test]
async fn file_parallel_fresh() {
    let body = pattern(16 * 1024 * 1024 + 1);
    let server = FakeServer::start(body.clone(), Behavior::default());
    let temp = tempfile::tempdir().unwrap();
    let destination = temp.path().join("file.bin");
    download_file(&client(), server.url().as_str(), &destination, None, |_| {})
        .await
        .unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), body);
    assert!(!chunk_download_dir(&destination).exists());
}

#[tokio::test]
async fn file_parallel_resume_skips_completed_chunks() {
    let body = pattern(16 * 1024 * 1024 + 1);
    let server = FakeServer::start(body.clone(), Behavior::default());
    let temp = tempfile::tempdir().unwrap();
    let destination = temp.path().join("file.bin");
    let chunk_dir = chunk_download_dir(&destination);
    std::fs::create_dir_all(&chunk_dir).unwrap();
    std::fs::write(chunk_path(&chunk_dir, 0), &body[..CHUNK_SIZE as usize]).unwrap();
    let (saved, progress) = updates();
    download_file(
        &client(),
        server.url().as_str(),
        &destination,
        None,
        progress,
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), body);
    let ranges: Vec<_> = server
        .log()
        .into_iter()
        .filter_map(|entry| entry.range)
        .collect();
    assert!(
        ranges.iter().all(|range| !range.starts_with("bytes=0-")),
        "{ranges:?}"
    );
    assert!(saved.lock().unwrap().iter().all(|update| update.resumed));
}

#[tokio::test]
async fn file_parallel_wrong_size_chunk_redownloaded() {
    let body = pattern(16 * 1024 * 1024 + 1);
    let server = FakeServer::start(body.clone(), Behavior::default());
    let temp = tempfile::tempdir().unwrap();
    let destination = temp.path().join("file.bin");
    let chunk_dir = chunk_download_dir(&destination);
    std::fs::create_dir_all(&chunk_dir).unwrap();
    std::fs::write(chunk_path(&chunk_dir, 1), [0u8; 10]).unwrap();
    download_file(&client(), server.url().as_str(), &destination, None, |_| {})
        .await
        .unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), body);
    let ranges: Vec<_> = server
        .log()
        .into_iter()
        .filter_map(|entry| entry.range)
        .collect();
    assert!(
        ranges
            .iter()
            .any(|range| range.starts_with("bytes=8388608-")),
        "{ranges:?}"
    );
}

#[tokio::test]
async fn file_parallel_chunk_failure_keeps_completed() {
    let body = pattern(16 * 1024 * 1024 + 1);
    // Let the other 8 MiB chunks finish writing before this range returns 500.
    let behavior = Behavior {
        fail_range_start_once: Some(16 * 1024 * 1024),
        fail_range_delay: Duration::from_millis(400),
        ..Behavior::default()
    };
    let server = FakeServer::start(body.clone(), behavior);
    let temp = tempfile::tempdir().unwrap();
    let destination = temp.path().join("file.bin");
    let first = download_file(&client(), server.url().as_str(), &destination, None, |_| {}).await;
    assert!(first.is_err());
    let chunk_dir = chunk_download_dir(&destination);
    assert!(chunk_path(&chunk_dir, 0).exists());
    assert!(chunk_path(&chunk_dir, 1).exists());
    assert!(!chunk_path(&chunk_dir, 2).exists());

    server.set_behavior(Behavior::default());
    server.clear_log();
    download_file(&client(), server.url().as_str(), &destination, None, |_| {})
        .await
        .unwrap();
    assert_eq!(std::fs::read(&destination).unwrap(), body);
    let ranges: Vec<_> = server
        .log()
        .into_iter()
        .filter_map(|entry| entry.range)
        .collect();
    assert!(
        ranges
            .iter()
            .all(|range| range.starts_with("bytes=16777216-")),
        "{ranges:?}"
    );
    assert!(!ranges.is_empty());
}

#[tokio::test]
async fn cancel_before_start() {
    let server = FakeServer::start(pattern(32), Behavior::default());
    let temp = tempfile::tempdir().unwrap();
    let destination = temp.path().join("file.bin");
    let token = CancellationToken::new();
    token.cancel();
    let message = expect_err(
        download_file(
            &client(),
            server.url().as_str(),
            &destination,
            Some(token),
            |_| {},
        )
        .await,
    )
    .await;
    assert!(message.contains("download cancelled"), "{message}");
    assert!(server.log().is_empty());
    assert!(!destination.exists());
}

#[tokio::test]
async fn cancel_mid_download_keeps_partial() {
    let behavior = Behavior {
        accept_ranges: false,
        pace: Some(Duration::from_millis(20)),
        ..Behavior::default()
    };
    let server = FakeServer::start(pattern(4096), behavior);
    let temp = tempfile::tempdir().unwrap();
    let destination = temp.path().join("file.bin");
    let token = CancellationToken::new();
    let flag = token.clone();
    let message = expect_err(
        download_file(
            &client(),
            server.url().as_str(),
            &destination,
            Some(token),
            move |_| flag.cancel(),
        )
        .await,
    )
    .await;
    assert!(
        message.contains("download cancelled") || message.contains("cancelled"),
        "{message}"
    );
    assert!(!destination.exists());
    assert!(partial_download_path(&destination).exists());
}

// IDLE_TIMEOUT is a 30s constant. Leaving it alone keeps this PR test-only;
// running the stall case would wait out that timer.
#[tokio::test]
#[ignore = "IDLE_TIMEOUT is hard-coded to 30s; stalling a connection waits that long"]
async fn stalled_connection_times_out() {
    let behavior = Behavior {
        stall_after_headers: true,
        accept_ranges: false,
        ..Behavior::default()
    };
    let server = FakeServer::start(pattern(64), behavior);
    let message = expect_err(download_to_vec(&client(), server.url(), None, None).await).await;
    assert!(message.contains("connection stalled"), "{message}");
}

#[tokio::test]
async fn file_zero_length() {
    let behavior = Behavior {
        accept_ranges: false,
        ..Behavior::default()
    };
    let server = FakeServer::start(Vec::new(), behavior);
    let temp = tempfile::tempdir().unwrap();
    let destination = temp.path().join("empty.bin");
    download_file(&client(), server.url().as_str(), &destination, None, |_| {})
        .await
        .expect("empty file should be created");
    assert_eq!(std::fs::read(&destination).unwrap(), b"");
    assert!(!partial_download_path(&destination).exists());
}

#[tokio::test]
async fn file_write_error_includes_path() {
    let behavior = Behavior {
        accept_ranges: false,
        ..Behavior::default()
    };
    let server = FakeServer::start(pattern(32), behavior);
    let temp = tempfile::tempdir().unwrap();
    let blocker = temp.path().join("not-a-directory");
    std::fs::write(&blocker, b"x").unwrap();
    let destination = blocker.join("file.bin");
    let message = expect_err(
        download_file(&client(), server.url().as_str(), &destination, None, |_| {}).await,
    )
    .await;
    assert!(message.contains("file.bin"), "{message}");
}

#[tokio::test]
async fn file_overlong_body_currently_accepted() {
    // B6: the single-connection completion check is `downloaded >= total`,
    // so a body longer than the Content-Range total is kept.
    let body = pattern(1000);
    let behavior = Behavior {
        content_range_total: Some(150),
        ..Behavior::default()
    };
    let server = FakeServer::start(body.clone(), behavior);
    let temp = tempfile::tempdir().unwrap();
    let destination = temp.path().join("file.bin");
    std::fs::write(partial_download_path(&destination), &body[..100]).unwrap();
    download_file(&client(), server.url().as_str(), &destination, None, |_| {})
        .await
        .unwrap();
    let stored = std::fs::read(&destination).unwrap();
    assert_eq!(stored, body);
    assert!(stored.len() > 150);
}
