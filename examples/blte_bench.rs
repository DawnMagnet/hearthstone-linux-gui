//! Throwaway baseline for NGDP BLTE decode on the install path.
//!
//! ```text
//! cargo run --release --no-default-features --example blte_bench
//! ```
//!
//! Samples are synthesized in process (no network, no Blizzard credentials).
//! Set `BLTE_SAMPLE_DIR` to reuse an existing sample directory so a second
//! flate2 backend inflates the same bytes. Set `BLTE_BACKEND` to label the
//! active backend in the report. Hashes are written to `BLTE_HASH_FILE`
//! (default `/tmp/blte-sample-hashes.txt`).
//!
//! The official timings call `hearthstone_linux::ngdp::blte::decode`. The
//! direct and parallel functions are estimates only. They are checked against
//! the same plaintext.

use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use sha2::{Digest, Sha256};
use std::fs;
use std::hint::black_box;
use std::io::{Read, Write};
use std::mem::MaybeUninit;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const RUNS: usize = 9;
const CHUNK: usize = 256 * 1024;
const LARGE_LEN: usize = 32 * 1024 * 1024;
const SMALL_COUNT: usize = 4096;
const SMALL_LEN: usize = 4 * 1024;
const MIXED_LEN: usize = 8 * 1024 * 1024;
const INSTALL_CONCURRENCY: usize = 8;
const STAMP: &str = "blte-bench-v1";
const SAMPLE_DIR: &str = "/dev/shm/blte-bench-samples";
const INSTALL_DIR: &str = "/dev/shm/blte-bench-install";

struct FileSample {
    rel: String,
    encoded: Vec<u8>,
    plaintext: Vec<u8>,
    encoding_key: String,
    content_key: String,
    chunks: usize,
    z_chunks: usize,
    n_chunks: usize,
}

struct Set {
    name: &'static str,
    files: Vec<FileSample>,
}

struct Timed {
    label: &'static str,
    sample: &'static str,
    median: Duration,
    stddev: Duration,
    min: Duration,
    max: Duration,
    decoded_bytes: u64,
}

#[derive(Clone, Copy, Default)]
struct Stages {
    parse: Duration,
    md5_frame: Duration,
    md5_blocks: Duration,
    inflate: Duration,
    n_copy: Duration,
    extend: Duration,
}

fn main() {
    if cfg!(debug_assertions) {
        eprintln!("refusing a debug build; use cargo run --release --example blte_bench");
        std::process::exit(2);
    }
    if std::env::var_os("BLTE_QA").is_some() {
        qa_main();
        return;
    }

    let backend = std::env::var("BLTE_BACKEND").unwrap_or_else(|_| "miniz_oxide".to_string());
    let sample_dir = std::env::var("BLTE_SAMPLE_DIR").unwrap_or_else(|_| SAMPLE_DIR.to_string());
    let sample_dir = PathBuf::from(sample_dir);
    let hash_path = std::env::var("BLTE_HASH_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp/blte-sample-hashes.txt"));

    print_env(&backend);
    assert_mount_is_tmpfs(Path::new("/dev/shm"));

    eprintln!("preparing samples in {}", sample_dir.display());
    let ready = sample_dir_ready(&sample_dir);
    if !ready {
        let _ = fs::remove_dir_all(&sample_dir);
        fs::create_dir_all(&sample_dir).unwrap();
    }
    let large = load_or_build_large(&sample_dir, ready);
    let mixed = load_or_build_mixed(&sample_dir, ready);
    let small = load_or_build_small(&sample_dir, ready);
    if !ready {
        fs::write(sample_dir.join("STAMP"), STAMP).unwrap();
    }
    let sets = [
        Set {
            name: "A",
            files: vec![large],
        },
        Set {
            name: "B",
            files: small,
        },
        Set {
            name: "mixed",
            files: vec![mixed],
        },
    ];

    write_hashes(&hash_path, &sets);
    for set in &sets {
        for file in &set.files {
            let decoded =
                hearthstone_linux::ngdp::blte::decode(&file.encoded, &file.encoding_key, false)
                    .unwrap_or_else(|error| panic!("roundtrip {}: {error:#}", file.rel));
            assert_same(&decoded, &file.plaintext, &file.rel);
        }
        let file = &set.files[0];
        eprintln!(
            "sample {} files={} decoded_bytes={} encoded_bytes={} chunks={} Z={} N={}",
            set.name,
            set.files.len(),
            decoded_bytes(set),
            set.files.iter().map(|f| f.encoded.len()).sum::<usize>(),
            file.chunks,
            file.z_chunks,
            file.n_chunks
        );
    }

    let mut rows = Vec::new();
    for set in &sets {
        rows.push(bench_decode(set, false));
        rows.push(bench_decode_md5(set));
    }
    // Archive fetch passes verify into blte::decode. Install's loose path does not.
    rows.push(bench_decode(&sets[0], true));
    rows.push(bench_decode(&sets[1], true));

    let install_root = PathBuf::from(INSTALL_DIR);
    let _ = fs::remove_dir_all(&install_root);
    fs::create_dir_all(&install_root).unwrap();
    rows.push(bench_install(&sets[0], &install_root));
    rows.push(bench_install(&sets[1], &install_root));
    let _ = fs::remove_dir_all(&install_root);

    rows.push(bench_fn("prealloc_header", &sets[0], |file| {
        decode_prealloc(&file.encoded)
    }));
    rows.push(bench_fn("direct_into_buf", &sets[0], |file| {
        decode_direct(&file.encoded, false)
    }));
    rows.push(bench_fn("parallel_chunks", &sets[0], |file| {
        decode_direct(&file.encoded, true)
    }));
    rows.push(bench_fn("direct_known_size", &sets[1], |file| {
        decode_known_size(&file.encoded, file.plaintext.len())
    }));
    rows.push(bench_parallel_files(&sets[1]));
    rows.push(bench_memcpy(&sets[0]));
    rows.push(bench_memcpy(&sets[1]));
    rows.push(bench_std_write(&sets[0]));
    rows.push(bench_std_write(&sets[1]));
    rows.push(bench_tokio_write(&sets[0], true));
    rows.push(bench_tokio_write(&sets[0], false));
    rows.push(bench_tokio_write(&sets[1], true));

    let stage_false_a = bench_stages(&sets[0], false);
    let stage_true_a = bench_stages(&sets[0], true);
    let stage_false_b = bench_stages(&sets[1], false);
    let stage_mixed = bench_stages(&sets[2], false);

    probe_blocks_worker(&sets[0].files[0]);
    probe_saturates_runtime(&sets[0].files[0]);

    println!("BEGIN_TIMINGS");
    for row in &rows {
        print_timed(row);
    }
    println!("END_TIMINGS");
    println!("BEGIN_STAGES");
    print_stages("A", false, stage_false_a);
    print_stages("A", true, stage_true_a);
    print_stages("B", false, stage_false_b);
    print_stages("mixed", false, stage_mixed);
    println!("END_STAGES");
    eprintln!("wrote hashes to {}", hash_path.display());
}

fn print_env(backend: &str) {
    let rustc = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .unwrap_or_else(|| "unknown".into());
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .unwrap_or_default();
    println!("ENV rustc={}", rustc.trim());
    println!("ENV cpu={}", cpu_model());
    println!(
        "ENV cores={}",
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0)
    );
    println!("ENV backend={backend}");
    println!("ENV commit={}", commit.trim());
    println!("ENV profile=release opt-level=2 codegen-units=16 lto=off panic=abort strip=symbols");
    println!("ENV runs={RUNS} unit=MiB/s=decoded_bytes/1048576/median_seconds");
    println!("ENV install_concurrency={INSTALL_CONCURRENCY}");
    println!("ENV tokio_runtime=Runtime::new multi-thread default workers");
}

fn cpu_model() -> String {
    let Ok(text) = fs::read_to_string("/proc/cpuinfo") else {
        return "unknown".into();
    };
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("model name") {
            return rest.trim().trim_start_matches(':').trim().to_string();
        }
    }
    "unknown".into()
}

fn assert_mount_is_tmpfs(path: &Path) {
    let mounts = fs::read_to_string("/proc/mounts").unwrap();
    let wanted = path.to_string_lossy();
    let tmpfs = mounts.lines().any(|line| {
        let mut parts = line.split_whitespace();
        let _dev = parts.next();
        let mount = parts.next().unwrap_or("");
        let kind = parts.next().unwrap_or("");
        mount == wanted && kind == "tmpfs"
    });
    if !tmpfs {
        eprintln!("{wanted} is not a tmpfs mount");
        std::process::exit(2);
    }
}

fn decoded_bytes(set: &Set) -> usize {
    set.files.iter().map(|file| file.plaintext.len()).sum()
}

fn sample_dir_ready(dir: &Path) -> bool {
    fs::read_to_string(dir.join("STAMP")).ok().as_deref() == Some(STAMP)
        && dir.join("A.blte").is_file()
        && dir.join("mixed.blte").is_file()
        && dir.join(format!("B/{:04}.blte", SMALL_COUNT - 1)).is_file()
}

fn load_or_build_large(dir: &Path, ready: bool) -> FileSample {
    let plain = compressible(LARGE_LEN, 0xB17E_0001);
    let encoded = load_or_encode(dir, ready, "A.blte", || {
        let parts: Vec<(u8, &[u8])> = plain.chunks(CHUNK).map(|chunk| (b'Z', chunk)).collect();
        encode_blocks(&parts)
    });
    finish(
        "large.bin",
        encoded,
        plain,
        LARGE_LEN / CHUNK,
        LARGE_LEN / CHUNK,
        0,
    )
}

fn load_or_build_mixed(dir: &Path, ready: bool) -> FileSample {
    let plain = compressible(MIXED_LEN, 0xB17E_0002);
    let encoded = load_or_encode(dir, ready, "mixed.blte", || {
        let parts: Vec<(u8, &[u8])> = plain
            .chunks(CHUNK)
            .enumerate()
            .map(|(index, chunk)| (if index % 2 == 0 { b'Z' } else { b'N' }, chunk))
            .collect();
        encode_blocks(&parts)
    });
    let chunks = MIXED_LEN / CHUNK;
    finish("mixed.bin", encoded, plain, chunks, chunks / 2, chunks / 2)
}

fn load_or_build_small(dir: &Path, ready: bool) -> Vec<FileSample> {
    let mut files = Vec::with_capacity(SMALL_COUNT);
    for index in 0..SMALL_COUNT {
        let plain = compressible(SMALL_LEN, 0xB17E_1000u64.wrapping_add(index as u64));
        let name = format!("B/{index:04}.blte");
        let encoded = load_or_encode(dir, ready, &name, || encode_single(b'Z', &plain));
        let rel = format!("s/{:02x}/{index:04x}.bin", index >> 8);
        files.push(finish(&rel, encoded, plain, 1, 1, 0));
    }
    files
}

fn finish(
    rel: &str,
    encoded: Vec<u8>,
    plaintext: Vec<u8>,
    chunks: usize,
    z_chunks: usize,
    n_chunks: usize,
) -> FileSample {
    let encoding_key = encoding_key_of(&encoded);
    let content_key = format!("{:x}", md5::compute(&plaintext));
    // Fail before the timed section if the keys do not match what decode checks.
    let checked = hearthstone_linux::ngdp::blte::decode(&encoded, &encoding_key, true)
        .unwrap_or_else(|error| panic!("verify=true roundtrip {rel}: {error:#}"));
    assert_same(&checked, &plaintext, rel);
    let actual = format!("{:x}", md5::compute(&checked));
    if actual != content_key {
        panic!("content md5 mismatch for {rel}");
    }
    FileSample {
        rel: rel.to_string(),
        encoded,
        plaintext,
        encoding_key,
        content_key,
        chunks,
        z_chunks,
        n_chunks,
    }
}

fn load_or_encode(dir: &Path, ready: bool, name: &str, build: impl FnOnce() -> Vec<u8>) -> Vec<u8> {
    let path = dir.join(name);
    if ready {
        return fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    }
    let encoded = build();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, &encoded).unwrap();
    encoded
}

fn compressible(len: usize, seed: u64) -> Vec<u8> {
    let mut dst = vec![0u8; len];
    let mut state = seed | 1;
    let mut offset = 0usize;
    while offset + 64 <= dst.len() {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        dst[offset..offset + 4].copy_from_slice(b"HSDB");
        dst[offset + 4..offset + 8].copy_from_slice(b"CARD");
        dst[offset + 8..offset + 12].copy_from_slice(&(state as u32).to_le_bytes());
        dst[offset + 12..offset + 28].copy_from_slice(b"Data/Win/cardxml");
        let kind = ((state >> 40) as u8) & 7;
        dst[offset + 28] = kind;
        dst[offset + 29..offset + 32].copy_from_slice(&[0, 0, 1]);
        dst[offset + 32..offset + 64].fill(kind);
        offset += 64;
    }
    while offset < dst.len() {
        dst[offset] = 0;
        offset += 1;
    }
    dst
}

fn encode_single(kind: u8, plain: &[u8]) -> Vec<u8> {
    let chunk = encode_chunk(kind, plain);
    let mut out = Vec::with_capacity(8 + chunk.len());
    out.extend_from_slice(b"BLTE");
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&chunk);
    out
}

fn encode_blocks(parts: &[(u8, &[u8])]) -> Vec<u8> {
    struct Encoded {
        bytes: Vec<u8>,
        decoded_len: usize,
        sum: [u8; 16],
    }
    let blocks: Vec<Encoded> = parts
        .iter()
        .map(|(kind, plain)| {
            let bytes = encode_chunk(*kind, plain);
            let sum = md5::compute(&bytes).0;
            Encoded {
                decoded_len: plain.len(),
                bytes,
                sum,
            }
        })
        .collect();
    let header_size = 12 + 24 * blocks.len();
    let mut out =
        Vec::with_capacity(header_size + blocks.iter().map(|b| b.bytes.len()).sum::<usize>());
    out.extend_from_slice(b"BLTE");
    out.extend_from_slice(&(header_size as u32).to_be_bytes());
    out.push(0x0f);
    let count = blocks.len() as u32;
    out.push((count >> 16) as u8);
    out.push((count >> 8) as u8);
    out.push(count as u8);
    for block in &blocks {
        out.extend_from_slice(&(block.bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(&(block.decoded_len as u32).to_be_bytes());
        out.extend_from_slice(&block.sum);
    }
    assert_eq!(out.len(), header_size);
    for block in &blocks {
        out.extend_from_slice(&block.bytes);
    }
    out
}

fn encode_chunk(kind: u8, plain: &[u8]) -> Vec<u8> {
    match kind {
        b'N' => {
            let mut out = Vec::with_capacity(1 + plain.len());
            out.push(b'N');
            out.extend_from_slice(plain);
            out
        }
        b'Z' => {
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(plain).expect("zlib encode");
            let compressed = encoder.finish().expect("zlib finish");
            let mut out = Vec::with_capacity(1 + compressed.len());
            out.push(b'Z');
            out.extend_from_slice(&compressed);
            out
        }
        other => panic!("unsupported chunk kind {other}"),
    }
}

fn encoding_key_of(data: &[u8]) -> String {
    assert!(data.len() >= 8 && &data[0..4] == b"BLTE");
    let header_size = u32::from_be_bytes(data[4..8].try_into().unwrap()) as usize;
    let hashed = if header_size == 0 {
        data
    } else {
        &data[..header_size]
    };
    format!("{:x}", md5::compute(hashed))
}

fn write_hashes(path: &Path, sets: &[Set]) {
    let mut body = String::new();
    for set in sets {
        let mut encoded_hash = Sha256::new();
        let mut decoded_hash = Sha256::new();
        for (index, file) in set.files.iter().enumerate() {
            let enc = sha256(&file.encoded);
            let dec = sha256(&file.plaintext);
            encoded_hash.update(&file.encoded);
            decoded_hash.update(&file.plaintext);
            body.push_str(&format!(
                "FILE sample={} index={index} rel={} chunks={} z={} n={} encoded_bytes={} decoded_bytes={} encoded_sha256={enc} decoded_sha256={dec}\n",
                set.name,
                file.rel,
                file.chunks,
                file.z_chunks,
                file.n_chunks,
                file.encoded.len(),
                file.plaintext.len(),
            ));
        }
        println!(
            "HASH sample={} files={} decoded_bytes={} encoded_bytes={} encoded_sha256={} decoded_sha256={}",
            set.name,
            set.files.len(),
            decoded_bytes(set),
            set.files.iter().map(|file| file.encoded.len()).sum::<usize>(),
            hex_encode(&encoded_hash.finalize()),
            hex_encode(&decoded_hash.finalize()),
        );
    }
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(path, body).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
}

fn sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex_encode(&hasher.finalize())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
    }
    out
}

fn assert_same(got: &[u8], expected: &[u8], what: &str) {
    if got == expected {
        return;
    }
    eprintln!(
        "byte mismatch {what}: got {} bytes, expected {}",
        got.len(),
        expected.len()
    );
    let n = got.len().min(expected.len());
    for index in 0..n {
        if got[index] != expected[index] {
            eprintln!(
                "first difference at {index}: {:02x} vs {:02x}",
                got[index], expected[index]
            );
            break;
        }
    }
    std::process::exit(1);
}

fn time_runs(mut once: impl FnMut()) -> Vec<Duration> {
    once();
    let mut times = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let start = Instant::now();
        once();
        times.push(start.elapsed());
    }
    times
}

fn summarize(
    label: &'static str,
    sample: &'static str,
    times: Vec<Duration>,
    decoded_bytes: u64,
) -> Timed {
    let mut ordered = times.clone();
    ordered.sort();
    let n = ordered.len();
    let median = ordered[n / 2];
    let mean = ordered.iter().map(|d| d.as_secs_f64()).sum::<f64>() / n as f64;
    let var = ordered
        .iter()
        .map(|d| {
            let delta = d.as_secs_f64() - mean;
            delta * delta
        })
        .sum::<f64>()
        / (n as f64 - 1.0);
    Timed {
        label,
        sample,
        median,
        stddev: Duration::from_secs_f64(var.sqrt()),
        min: ordered[0],
        max: ordered[n - 1],
        decoded_bytes,
    }
}

fn print_timed(row: &Timed) {
    let median_s = row.median.as_secs_f64();
    let mib_s = (row.decoded_bytes as f64 / 1_048_576.0) / median_s;
    println!(
        "STAT label={} sample={} median_ms={:.3} stddev_ms={:.3} min_ms={:.3} max_ms={:.3} mib_s={:.1} decoded_bytes={}",
        row.label,
        row.sample,
        ms(row.median),
        ms(row.stddev),
        ms(row.min),
        ms(row.max),
        mib_s,
        row.decoded_bytes
    );
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn bench_decode(set: &Set, verify: bool) -> Timed {
    let label = if verify { "decode_verify" } else { "decode" };
    let bytes = decoded_bytes(set) as u64;
    let times = time_runs(|| {
        for file in &set.files {
            let out =
                hearthstone_linux::ngdp::blte::decode(&file.encoded, &file.encoding_key, verify)
                    .unwrap_or_else(|error| panic!("{label} {}: {error:#}", file.rel));
            black_box(out);
        }
    });
    // One equality check outside the timer, on the same function.
    for file in &set.files {
        let out = hearthstone_linux::ngdp::blte::decode(&file.encoded, &file.encoding_key, verify)
            .unwrap();
        assert_same(&out, &file.plaintext, &file.rel);
    }
    summarize(label, set.name, times, bytes)
}

fn bench_decode_md5(set: &Set) -> Timed {
    let bytes = decoded_bytes(set) as u64;
    let times = time_runs(|| {
        for file in &set.files {
            let out =
                hearthstone_linux::ngdp::blte::decode(&file.encoded, &file.encoding_key, false)
                    .unwrap();
            let actual = format!("{:x}", md5::compute(&out));
            assert!(actual == file.content_key, "content md5");
            black_box(out);
        }
    });
    summarize("decode_md5", set.name, times, bytes)
}

fn bench_install(set: &Set, root: &Path) -> Timed {
    let shared: Vec<Arc<SharedFile>> = set
        .files
        .iter()
        .map(|file| {
            Arc::new(SharedFile {
                rel: file.rel.clone(),
                encoded: Arc::<[u8]>::from(file.encoded.as_slice()),
                encoding_key: Arc::<str>::from(file.encoding_key.as_str()),
                content_key: Arc::<str>::from(file.content_key.as_str()),
            })
        })
        .collect();
    let bytes = decoded_bytes(set) as u64;
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let dest = root.join(set.name);
    let _ = fs::remove_dir_all(&dest);
    fs::create_dir_all(&dest).unwrap();
    rt.block_on(install_all(&shared, &dest));
    verify_written(set, &dest);
    let times = time_runs(|| {
        rt.block_on(install_all(&shared, &dest));
    });
    verify_written(set, &dest);
    summarize("install8", set.name, times, bytes)
}

struct SharedFile {
    rel: String,
    encoded: Arc<[u8]>,
    encoding_key: Arc<str>,
    content_key: Arc<str>,
}

async fn install_all(files: &[Arc<SharedFile>], root: &Path) {
    let mut pending = files.iter().cloned();
    let mut tasks = tokio::task::JoinSet::new();
    let mut active = 0usize;
    loop {
        while active < INSTALL_CONCURRENCY {
            let Some(file) = pending.next() else {
                break;
            };
            active += 1;
            let root = root.to_path_buf();
            tasks.spawn(async move {
                install_one(file, root).await;
            });
            if pending.len() == 0 && active >= files.len() {
                break;
            }
        }
        if active == 0 {
            break;
        }
        tasks.join_next().await.expect("join").expect("task");
        active -= 1;
    }
}

async fn install_one(file: Arc<SharedFile>, root: PathBuf) {
    // BLTE_LEGACY_INSTALL selects master's inline decode plus tokio::fs::write
    // so the same harness can time d1e996b. The default matches install_one_entry
    // after the offload.
    if std::env::var_os("BLTE_LEGACY_INSTALL").is_some() {
        let decoded =
            hearthstone_linux::ngdp::blte::decode(&file.encoded, &file.encoding_key, false)
                .unwrap_or_else(|error| panic!("install decode {}: {error:#}", file.rel));
        let actual = format!("{:x}", md5::compute(&decoded));
        assert!(actual == file.content_key.as_ref(), "install content md5");
        let target = root.join(&file.rel);
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .unwrap_or_else(|error| panic!("mkdir {}: {error}", parent.display()));
        }
        tokio::fs::write(&target, decoded)
            .await
            .unwrap_or_else(|error| panic!("write {}: {error}", target.display()));
        tokio::fs::metadata(&target)
            .await
            .unwrap_or_else(|error| panic!("stat {}: {error}", target.display()));
        return;
    }
    // Mirrors install_one_entry after the offload: decode, content md5, and
    // std::fs::write run inside spawn_blocking, not on the async worker.
    let target = root.join(&file.rel);
    tokio::task::spawn_blocking(move || {
        let decoded =
            hearthstone_linux::ngdp::blte::decode(&file.encoded, &file.encoding_key, false)
                .unwrap_or_else(|error| panic!("install decode {}: {error:#}", file.rel));
        let actual = format!("{:x}", md5::compute(&decoded));
        assert!(actual == file.content_key.as_ref(), "install content md5");
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .unwrap_or_else(|error| panic!("mkdir {}: {error}", parent.display()));
        }
        std::fs::write(&target, &decoded)
            .unwrap_or_else(|error| panic!("write {}: {error}", target.display()));
        std::fs::metadata(&target)
            .unwrap_or_else(|error| panic!("stat {}: {error}", target.display()));
    })
    .await
    .unwrap_or_else(|error| panic!("install task {error}"));
}

fn verify_written(set: &Set, root: &Path) {
    for file in &set.files {
        let got = fs::read(root.join(&file.rel))
            .unwrap_or_else(|error| panic!("readback {}: {error}", file.rel));
        assert_same(&got, &file.plaintext, &file.rel);
    }
}

fn bench_fn(
    label: &'static str,
    set: &Set,
    mut decode: impl FnMut(&FileSample) -> Vec<u8>,
) -> Timed {
    let bytes = decoded_bytes(set) as u64;
    let times = time_runs(|| {
        for file in &set.files {
            let out = decode(file);
            black_box(&out);
        }
    });
    for file in &set.files {
        let out = decode(file);
        assert_same(&out, &file.plaintext, &file.rel);
    }
    summarize(label, set.name, times, bytes)
}

fn bench_parallel_files(set: &Set) -> Timed {
    let threads = std::thread::available_parallelism().unwrap().get();
    let bytes = decoded_bytes(set) as u64;
    let times = time_runs(|| {
        let chunk = set.files.len().div_ceil(threads);
        std::thread::scope(|scope| {
            for group in set.files.chunks(chunk) {
                scope.spawn(move || {
                    for file in group {
                        let out = hearthstone_linux::ngdp::blte::decode(
                            &file.encoded,
                            &file.encoding_key,
                            false,
                        )
                        .unwrap();
                        assert_eq!(out.len(), file.plaintext.len());
                        black_box(out);
                    }
                });
            }
        });
    });
    summarize("parallel_files", set.name, times, bytes)
}

fn bench_memcpy(set: &Set) -> Timed {
    let bytes = decoded_bytes(set) as u64;
    let times = time_runs(|| {
        for file in &set.files {
            black_box(file.plaintext.clone());
        }
    });
    summarize("memcpy_plaintext", set.name, times, bytes)
}

fn bench_std_write(set: &Set) -> Timed {
    let bytes = decoded_bytes(set) as u64;
    let root = PathBuf::from(format!("{INSTALL_DIR}-std-{}", set.name));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let times = time_runs(|| {
        for file in &set.files {
            let path = root.join(&file.rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, &file.plaintext).unwrap();
        }
    });
    let _ = fs::remove_dir_all(&root);
    summarize("std_fs_write", set.name, times, bytes)
}

fn bench_tokio_write(set: &Set, owned_vec: bool) -> Timed {
    let bytes = decoded_bytes(set) as u64;
    let label = if owned_vec {
        "tokio_fs_write_vec"
    } else {
        "tokio_fs_write_slice"
    };
    let root = PathBuf::from(format!("{INSTALL_DIR}-tokio-{}-{}", set.name, label));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    for file in &set.files {
        if let Some(parent) = root.join(&file.rel).parent() {
            fs::create_dir_all(parent).unwrap();
        }
    }
    let rt = tokio::runtime::Runtime::new().unwrap();
    // Clone outside the clock. The owned-Vec path then moves that Vec into
    // tokio::fs::write, which is what install_one_entry does.
    let mut times = Vec::with_capacity(RUNS);
    for run in 0..=RUNS {
        let clones: Vec<(PathBuf, Vec<u8>)> = set
            .files
            .iter()
            .map(|file| (root.join(&file.rel), file.plaintext.clone()))
            .collect();
        let start = Instant::now();
        rt.block_on(async {
            let mut pending = clones.into_iter();
            let mut tasks = tokio::task::JoinSet::new();
            let mut active = 0usize;
            loop {
                while active < INSTALL_CONCURRENCY {
                    let Some((path, owned)) = pending.next() else {
                        break;
                    };
                    active += 1;
                    tasks.spawn(async move {
                        if owned_vec {
                            tokio::fs::write(&path, owned).await.unwrap();
                        } else {
                            tokio::fs::write(&path, owned.as_slice()).await.unwrap();
                        }
                    });
                }
                if active == 0 {
                    break;
                }
                tasks.join_next().await.unwrap().unwrap();
                active -= 1;
            }
        });
        if run > 0 {
            times.push(start.elapsed());
        }
    }
    let _ = fs::remove_dir_all(&root);
    summarize(label, set.name, times, bytes)
}

fn bench_stages(set: &Set, verify: bool) -> Stages {
    let mut samples = Vec::with_capacity(RUNS);
    // Warmup.
    let _ = stages_once(set, verify);
    for _ in 0..RUNS {
        samples.push(stages_once(set, verify));
    }
    median_stages(&samples)
}

fn stages_once(set: &Set, verify: bool) -> Stages {
    let mut total = Stages::default();
    for file in &set.files {
        let (out, stages) = decode_staged(&file.encoded, &file.encoding_key, verify);
        assert_same(&out, &file.plaintext, &file.rel);
        total.parse += stages.parse;
        total.md5_frame += stages.md5_frame;
        total.md5_blocks += stages.md5_blocks;
        total.inflate += stages.inflate;
        total.n_copy += stages.n_copy;
        total.extend += stages.extend;
    }
    total
}

fn median_stages(samples: &[Stages]) -> Stages {
    fn median(mut values: Vec<Duration>) -> Duration {
        values.sort();
        values[values.len() / 2]
    }
    Stages {
        parse: median(samples.iter().map(|s| s.parse).collect()),
        md5_frame: median(samples.iter().map(|s| s.md5_frame).collect()),
        md5_blocks: median(samples.iter().map(|s| s.md5_blocks).collect()),
        inflate: median(samples.iter().map(|s| s.inflate).collect()),
        n_copy: median(samples.iter().map(|s| s.n_copy).collect()),
        extend: median(samples.iter().map(|s| s.extend).collect()),
    }
}

fn print_stages(sample: &str, verify: bool, stages: Stages) {
    let sum = stages.parse
        + stages.md5_frame
        + stages.md5_blocks
        + stages.inflate
        + stages.n_copy
        + stages.extend;
    let pct = |part: Duration| {
        if sum.is_zero() {
            0.0
        } else {
            100.0 * part.as_secs_f64() / sum.as_secs_f64()
        }
    };
    println!(
        "STAGE sample={sample} verify={verify} parse_ms={:.3} md5_frame_ms={:.3} md5_blocks_ms={:.3} inflate_ms={:.3} n_copy_ms={:.3} extend_ms={:.3} sum_ms={:.3} pct_parse={:.1} pct_md5_frame={:.1} pct_md5_blocks={:.1} pct_inflate={:.1} pct_n_copy={:.1} pct_extend={:.1}",
        ms(stages.parse),
        ms(stages.md5_frame),
        ms(stages.md5_blocks),
        ms(stages.inflate),
        ms(stages.n_copy),
        ms(stages.extend),
        ms(sum),
        pct(stages.parse),
        pct(stages.md5_frame),
        pct(stages.md5_blocks),
        pct(stages.inflate),
        pct(stages.n_copy),
        pct(stages.extend),
    );
}

/// Same steps as `blte::decode`, with a clock around each step.
fn decode_staged(data: &[u8], key: &str, verify: bool) -> (Vec<u8>, Stages) {
    let mut stages = Stages::default();
    let start = Instant::now();
    assert!(data.len() >= 8 && &data[0..4] == b"BLTE");
    let header_size = u32::from_be_bytes(data[4..8].try_into().unwrap()) as usize;
    struct Block {
        encoded_size: usize,
        decoded_size: usize,
        digest: String,
    }
    let mut blocks = Vec::new();
    if header_size > 0 {
        assert!(data.len() >= header_size && data[8] == 0x0f);
        let block_count =
            ((data[9] as usize) << 16) | ((data[10] as usize) << 8) | data[11] as usize;
        let mut offset = 12usize;
        for _ in 0..block_count {
            let encoded_size =
                u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
            let decoded_size =
                u32::from_be_bytes(data[offset + 4..offset + 8].try_into().unwrap()) as usize;
            let digest = hex_encode(&data[offset + 8..offset + 24]);
            offset += 24;
            blocks.push(Block {
                encoded_size,
                decoded_size,
                digest,
            });
        }
        stages.parse += start.elapsed();
        let md5_start = Instant::now();
        if verify {
            let actual = format!("{:x}", md5::compute(&data[..header_size]));
            assert_eq!(actual, key);
        }
        stages.md5_frame += md5_start.elapsed();
    } else {
        stages.parse += start.elapsed();
        let md5_start = Instant::now();
        if verify {
            let actual = format!("{:x}", md5::compute(data));
            assert_eq!(actual, key);
        }
        stages.md5_frame += md5_start.elapsed();
        blocks.push(Block {
            encoded_size: data.len() - 8,
            decoded_size: 0,
            digest: String::new(),
        });
    }

    let mut offset = if header_size > 0 { header_size } else { 8 };
    let mut decoded = Vec::new();
    for block in blocks {
        let end = offset + block.encoded_size;
        let encoded_block = &data[offset..end];
        if !block.digest.is_empty() {
            let md5_start = Instant::now();
            if verify {
                let actual = format!("{:x}", md5::compute(encoded_block));
                assert_eq!(actual, block.digest);
            }
            stages.md5_blocks += md5_start.elapsed();
        }
        let (&kind, payload) = encoded_block.split_first().unwrap();
        let block_decoded = match kind {
            b'N' => {
                let copy_start = Instant::now();
                let copied = payload.to_vec();
                stages.n_copy += copy_start.elapsed();
                copied
            }
            b'Z' => {
                let inflate_start = Instant::now();
                let mut decoder = ZlibDecoder::new(payload);
                let mut out = Vec::new();
                decoder.read_to_end(&mut out).expect("inflate");
                stages.inflate += inflate_start.elapsed();
                out
            }
            other => panic!("bad block {other}"),
        };
        if block.decoded_size != 0 {
            assert_eq!(block_decoded.len(), block.decoded_size);
        }
        let extend_start = Instant::now();
        decoded.extend_from_slice(&block_decoded);
        stages.extend += extend_start.elapsed();
        offset = end;
    }
    (decoded, stages)
}

struct ParsedBlock {
    range: std::ops::Range<usize>,
    decoded_size: usize,
}

fn parse_blocks(data: &[u8]) -> (Vec<ParsedBlock>, bool) {
    let header_size = u32::from_be_bytes(data[4..8].try_into().unwrap()) as usize;
    if header_size == 0 {
        return (
            vec![ParsedBlock {
                range: 8..data.len(),
                decoded_size: 0,
            }],
            true,
        );
    }
    let block_count = ((data[9] as usize) << 16) | ((data[10] as usize) << 8) | data[11] as usize;
    let mut blocks = Vec::with_capacity(block_count);
    let mut cursor = header_size;
    for index in 0..block_count {
        let offset = 12 + index * 24;
        let encoded_size =
            u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
        let decoded_size =
            u32::from_be_bytes(data[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let range = cursor..cursor + encoded_size;
        cursor += encoded_size;
        blocks.push(ParsedBlock {
            range,
            decoded_size,
        });
    }
    (blocks, false)
}

fn decode_block_vec(data: &[u8]) -> Vec<u8> {
    let (&kind, payload) = data.split_first().unwrap();
    match kind {
        b'N' => payload.to_vec(),
        b'Z' => {
            let mut decoder = ZlibDecoder::new(payload);
            let mut out = Vec::new();
            decoder.read_to_end(&mut out).expect("inflate");
            out
        }
        other => panic!("bad block {other}"),
    }
}

fn decode_prealloc(data: &[u8]) -> Vec<u8> {
    let (blocks, single) = parse_blocks(data);
    let total = if single {
        0
    } else {
        blocks.iter().map(|block| block.decoded_size).sum()
    };
    let mut decoded = Vec::with_capacity(total);
    for block in &blocks {
        let block_decoded = decode_block_vec(&data[block.range.clone()]);
        decoded.extend_from_slice(&block_decoded);
    }
    decoded
}

fn decode_direct(data: &[u8], parallel: bool) -> Vec<u8> {
    let (blocks, single) = parse_blocks(data);
    if single {
        return decode_block_vec(&data[blocks[0].range.clone()]);
    }
    let total: usize = blocks.iter().map(|block| block.decoded_size).sum();
    let mut out = Vec::new();
    out.reserve_exact(total);
    let threads = if parallel {
        std::thread::available_parallelism().unwrap().get()
    } else {
        1
    };
    decode_tree(
        data,
        &blocks,
        &mut out.spare_capacity_mut()[..total],
        threads,
    );
    // Safety: decode_tree writes every byte of each chunk before returning,
    // and thread::scope joins before we get here.
    unsafe { out.set_len(total) };
    out
}

fn decode_tree(data: &[u8], blocks: &[ParsedBlock], dst: &mut [MaybeUninit<u8>], threads: usize) {
    if threads <= 1 || blocks.len() <= 1 {
        let mut rest = dst;
        for block in blocks {
            let (slot, next) = rest.split_at_mut(block.decoded_size);
            rest = next;
            write_block(&data[block.range.clone()], slot);
        }
        return;
    }
    let mid = blocks.len() / 2;
    let left_len: usize = blocks[..mid].iter().map(|block| block.decoded_size).sum();
    let (left_dst, right_dst) = dst.split_at_mut(left_len);
    let (left_blocks, right_blocks) = blocks.split_at(mid);
    let left_threads = threads / 2;
    let right_threads = threads - left_threads;
    std::thread::scope(|scope| {
        scope.spawn(|| decode_tree(data, left_blocks, left_dst, left_threads));
        decode_tree(data, right_blocks, right_dst, right_threads);
    });
}

fn write_block(encoded: &[u8], dst: &mut [MaybeUninit<u8>]) {
    let (&kind, payload) = encoded.split_first().unwrap();
    match kind {
        b'N' => {
            assert_eq!(payload.len(), dst.len());
            for (slot, byte) in dst.iter_mut().zip(payload.iter()) {
                slot.write(*byte);
            }
        }
        b'Z' => {
            let mut decoder = ZlibDecoder::new(payload);
            // Safety: read_exact initializes the whole slice or returns Err,
            // and we abort on Err before the caller publishes the length.
            let raw =
                unsafe { std::slice::from_raw_parts_mut(dst.as_mut_ptr() as *mut u8, dst.len()) };
            decoder.read_exact(raw).expect("inflate into place");
        }
        other => panic!("bad block {other}"),
    }
}

fn decode_known_size(data: &[u8], decoded_len: usize) -> Vec<u8> {
    let mut out = Vec::new();
    out.reserve_exact(decoded_len);
    {
        let uninit = out.spare_capacity_mut();
        write_single_frame(data, &mut uninit[..decoded_len]);
    }
    // Safety: write_single_frame fills decoded_len bytes or aborts.
    unsafe { out.set_len(decoded_len) };
    out
}

fn write_single_frame(data: &[u8], dst: &mut [MaybeUninit<u8>]) {
    assert_eq!(&data[0..4], b"BLTE");
    assert_eq!(&data[4..8], &0u32.to_be_bytes());
    write_block(&data[8..], dst);
}

fn probe_blocks_worker(file: &FileSample) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_time()
        .build()
        .unwrap();
    let encoded = file.encoded.clone();
    let key = file.encoding_key.clone();
    let (decode_ms, gap_ms) = rt.block_on(async move {
        let ticker = tokio::spawn(async {
            let mut last = Instant::now();
            let mut max_gap = Duration::ZERO;
            for _ in 0..30 {
                tokio::time::sleep(Duration::from_millis(2)).await;
                max_gap = max_gap.max(last.elapsed());
                last = Instant::now();
            }
            max_gap
        });
        tokio::time::sleep(Duration::from_millis(6)).await;
        let start = Instant::now();
        let out = hearthstone_linux::ngdp::blte::decode(&encoded, &key, false).unwrap();
        black_box(out);
        let decode_took = start.elapsed();
        let max_gap = ticker.await.unwrap();
        (decode_took, max_gap)
    });
    println!(
        "PROBE kind=one_worker decode_ms={:.3} max_ticker_gap_ms={:.3} blocks_worker={}",
        ms(decode_ms),
        ms(gap_ms),
        gap_ms > decode_ms / 2
    );
}

fn probe_saturates_runtime(file: &FileSample) {
    let workers = std::thread::available_parallelism().unwrap().get();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_time()
        .build()
        .unwrap();
    let encoded: Arc<[u8]> = Arc::from(file.encoded.as_slice());
    let key: Arc<str> = Arc::from(file.encoding_key.as_str());
    let (decode_ms, gap_ms) = rt.block_on(async move {
        let ticker = tokio::spawn(async {
            let mut last = Instant::now();
            let mut max_gap = Duration::ZERO;
            for _ in 0..40 {
                tokio::time::sleep(Duration::from_millis(2)).await;
                max_gap = max_gap.max(last.elapsed());
                last = Instant::now();
            }
            max_gap
        });
        tokio::time::sleep(Duration::from_millis(6)).await;
        let start = Instant::now();
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..workers {
            let encoded = Arc::clone(&encoded);
            let key = Arc::clone(&key);
            tasks.spawn(async move {
                let out = hearthstone_linux::ngdp::blte::decode(&encoded, &key, false).unwrap();
                black_box(out);
            });
        }
        while tasks.join_next().await.is_some() {}
        let decode_took = start.elapsed();
        let max_gap = ticker.await.unwrap();
        (decode_took, max_gap)
    });
    println!(
        "PROBE kind=all_workers workers={workers} decode_ms={:.3} max_ticker_gap_ms={:.3} blocks_worker={}",
        ms(decode_ms),
        ms(gap_ms),
        gap_ms > decode_ms / 2
    );
}

fn qa_main() {
    let legacy = std::env::var_os("BLTE_LEGACY_INSTALL").is_some();
    println!("QA legacy_install={legacy} runs={RUNS}");
    let total = 440 * 1024 * 1024;
    let count = 14usize;
    let base = total / count;
    let mut files = Vec::with_capacity(count);
    for index in 0..count {
        let len = if index + 1 == count {
            total - base * (count - 1)
        } else {
            base
        };
        eprintln!("qa text file {index} bytes={len}");
        files.push(make_qa_file(
            len,
            0x0A11_0000 + index as u64,
            &format!("qa/{index:02}.bin"),
            false,
        ));
    }
    let bytes = files.iter().map(|file| file.plain_len as u64).sum();
    let times = time_runs(|| {
        for file in &files {
            let out =
                hearthstone_linux::ngdp::blte::decode(&file.encoded, &file.encoding_key, false)
                    .unwrap();
            assert_eq!(out.len(), file.plain_len);
            black_box(out);
        }
    });
    print_timed(&summarize("qa_decode", "text440", times, bytes));

    eprintln!("qa zeros");
    let zeros = make_qa_file(32 * 1024 * 1024, 1, "zero.bin", true);
    println!(
        "HASH sample=qa_zero decoded_sha256={} decoded_bytes={}",
        zeros.plain_sha, zeros.plain_len
    );
    let ztimes = time_runs(|| {
        let out = hearthstone_linux::ngdp::blte::decode(&zeros.encoded, &zeros.encoding_key, false)
            .unwrap();
        assert_eq!(out.len(), zeros.plain_len);
        black_box(out);
    });
    print_timed(&summarize(
        "qa_decode",
        "zero32",
        ztimes,
        zeros.plain_len as u64,
    ));

    let record_path = PathBuf::from("/dev/shm/blte-bench-samples/A.blte");
    if record_path.is_file() {
        let encoded = fs::read(&record_path).unwrap();
        let key = encoding_key_of(&encoded);
        let decoded = hearthstone_linux::ngdp::blte::decode(&encoded, &key, false).unwrap();
        println!(
            "HASH sample=record32 decoded_sha256={} decoded_bytes={} encoded_bytes={}",
            sha256(&decoded),
            decoded.len(),
            encoded.len()
        );
        let record_bytes = decoded.len() as u64;
        let rtimes = time_runs(|| {
            let out = hearthstone_linux::ngdp::blte::decode(&encoded, &key, false).unwrap();
            black_box(out);
        });
        print_timed(&summarize("qa_decode", "record32", rtimes, record_bytes));
    } else {
        eprintln!("qa record32 skipped; {} missing", record_path.display());
    }

    let mut small_hash = Sha256::new();
    for index in 0..256u64 {
        let file = make_qa_file(
            4 * 1024,
            0x51A11 + index,
            &format!("s/{index:03}.bin"),
            false,
        );
        small_hash.update(file.plain_sha.as_bytes());
    }
    println!(
        "HASH sample=qa_small_256 rollup_sha256={}",
        hex_encode(&small_hash.finalize())
    );

    let shared: Vec<Arc<SharedFile>> = files
        .iter()
        .map(|file| {
            Arc::new(SharedFile {
                rel: file.rel.clone(),
                encoded: Arc::clone(&file.encoded),
                encoding_key: Arc::clone(&file.encoding_key),
                content_key: Arc::clone(&file.content_key),
            })
        })
        .collect();
    let root = PathBuf::from("/dev/shm/blte-qa-install");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(install_all(&shared, &root));
    let itimes = time_runs(|| {
        rt.block_on(install_all(&shared, &root));
    });
    print_timed(&summarize("qa_install8", "text440", itimes, bytes));
    let _ = fs::remove_dir_all(&root);
}

struct QaFile {
    rel: String,
    encoded: Arc<[u8]>,
    encoding_key: Arc<str>,
    content_key: Arc<str>,
    plain_sha: String,
    plain_len: usize,
}

fn make_qa_file(len: usize, seed: u64, rel: &str, zeros: bool) -> QaFile {
    let mut plain = vec![0u8; len];
    if !zeros {
        fill_qa_text(&mut plain, seed);
    }
    let content_key: Arc<str> = Arc::from(format!("{:x}", md5::compute(&plain)));
    let plain_sha = sha256(&plain);
    let cache_path = std::env::var_os("BLTE_QA_CACHE").map(|dir| {
        let mut path = PathBuf::from(dir);
        path.push(rel.replace('/', "_"));
        path
    });
    let encoded = if let Some(path) = cache_path.as_ref().filter(|path| path.is_file()) {
        fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
    } else {
        let parts: Vec<(u8, &[u8])> = plain.chunks(CHUNK).map(|chunk| (b'Z', chunk)).collect();
        let encoded = encode_blocks(&parts);
        if let Some(path) = &cache_path {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, &encoded).unwrap();
        }
        encoded
    };
    let encoding_key: Arc<str> = Arc::from(encoding_key_of(&encoded));
    let decoded = hearthstone_linux::ngdp::blte::decode(&encoded, &encoding_key, true).unwrap();
    assert_eq!(decoded, plain);
    println!(
        "HASH sample={rel} decoded_sha256={plain_sha} decoded_bytes={len} encoded_bytes={}",
        encoded.len()
    );
    QaFile {
        rel: rel.to_string(),
        encoded: Arc::from(encoded.into_boxed_slice()),
        encoding_key,
        content_key,
        plain_sha,
        plain_len: len,
    }
}

fn fill_qa_text(dst: &mut [u8], seed: u64) {
    const WORDS: [&[u8]; 32] = [
        b"the",
        b"card",
        b"hearthstone",
        b"minion",
        b"spell",
        b"damage",
        b"battlecry",
        b"taunt",
        b"player",
        b"deck",
        b"mana",
        b"health",
        b"attack",
        b"secret",
        b"weapon",
        b"hero",
        b"board",
        b"hand",
        b"graveyard",
        b"discover",
        b"draw",
        b"cast",
        b"target",
        b"random",
        b"friendly",
        b"enemy",
        b"cost",
        b"turn",
        b"start",
        b"end",
        b"silence",
        b"freeze",
    ];
    let mut state = seed | 1;
    let mut index = 0usize;
    while index < dst.len() {
        state = state
            .wrapping_mul(0x5851_F42D_4C95_7F2D)
            .wrapping_add(0x1405_7B7E_F767_814F);
        let word = WORDS[((state >> 33) as usize) & 31];
        let n = word.len().min(dst.len() - index);
        dst[index..index + n].copy_from_slice(&word[..n]);
        index += n;
        if index < dst.len() {
            dst[index] = b' ';
            index += 1;
        }
        if index + 6 <= dst.len() && (state & 0xff) < 40 {
            for offset in 0..6 {
                dst[index + offset] = b'a' + (((state >> (offset * 5)) as u8) % 26);
            }
            index += 6;
        }
    }
}
