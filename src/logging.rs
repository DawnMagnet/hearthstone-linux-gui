use crate::paths::AppPaths;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

const LOG_FILE_PREFIX: &str = "hearthstone-linux-gui.log";
const LEGACY_AUTH_CALLBACK_LOG: &str = "auth-callback.log";
const REDACTED: &[u8] = b"REDACTED";

pub fn init() -> Option<WorkerGuard> {
    // One pass per process, before the rolling appender opens today's file.
    // Rewriting a log that is already open would leave the token in the inode
    // the appender still holds.
    let paths = AppPaths::discover().ok();
    let mut warnings = paths
        .as_ref()
        .map(|paths| prepare_log_dir(&paths.log_dir))
        .unwrap_or_default();

    let stderr_layer = tracing_subscriber::fmt::layer()
        .compact()
        .with_ansi(std::io::stderr().is_terminal())
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .with_writer(std::io::stderr);

    let guard = match paths.as_ref().map(|paths| open_log_writer(&paths.log_dir)) {
        Some(Ok((writer, guard))) => {
            let file_layer = tracing_subscriber::fmt::layer()
                .compact()
                .with_ansi(false)
                .with_target(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true)
                .with_writer(writer);
            let result = tracing_subscriber::registry()
                .with(default_filter())
                .with(stderr_layer)
                .with(file_layer)
                .try_init();
            if result.is_ok() {
                Some(guard)
            } else {
                None
            }
        }
        Some(Err(warning)) => {
            warnings.push(warning);
            let _ = tracing_subscriber::registry()
                .with(default_filter())
                .with(stderr_layer)
                .try_init();
            None
        }
        None => {
            let _ = tracing_subscriber::registry()
                .with(default_filter())
                .with(stderr_layer)
                .try_init();
            None
        }
    };

    for warning in warnings {
        tracing::warn!("{warning}");
    }
    guard
}

fn default_filter() -> EnvFilter {
    let app_filter = if cfg!(debug_assertions) {
        "hearthstone_linux=trace,CoreFoundation=trace,OSXWindowManagement=trace,NativeApiMac=trace,blz_commerce_sdk_plugin=trace,commerce_http_client=trace"
    } else {
        "hearthstone_linux=info,CoreFoundation=info,OSXWindowManagement=info,NativeApiMac=info,blz_commerce_sdk_plugin=info,commerce_http_client=info"
    };

    match std::env::var("RUST_LOG") {
        Ok(filter) if mentions_app_filter(&filter) => EnvFilter::new(filter),
        Ok(filter) if !filter.trim().is_empty() => EnvFilter::new(format!("{filter},{app_filter}")),
        _ if cfg!(debug_assertions) => EnvFilter::new(format!("info,{app_filter}")),
        _ => EnvFilter::new(format!("info,{app_filter}")),
    }
}

fn mentions_app_filter(filter: &str) -> bool {
    [
        "hearthstone_linux",
        "CoreFoundation",
        "OSXWindowManagement",
        "NativeApiMac",
        "blz_commerce_sdk_plugin",
        "commerce_http_client",
    ]
    .iter()
    .any(|target| filter.contains(target))
}

/// Tightens the log directory, deletes the legacy callback log, and rewrites
/// rolling logs that still contain login tokens. Problems are warnings; none
/// of them stop startup.
fn prepare_log_dir(log_dir: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
    if let Err(error) = ensure_private_log_dir(log_dir) {
        warnings.push(format!(
            "could not restrict log directory {}: {error}",
            log_dir.display()
        ));
    }
    warnings.extend(scrub_legacy_logs_returning_warnings(log_dir));
    warnings
}

fn ensure_private_log_dir(log_dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(log_dir)?;
    #[cfg(unix)]
    tighten_dir_mode(log_dir)?;
    Ok(())
}

#[cfg(unix)]
fn tighten_dir_mode(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() {
        return Err(io::Error::other("log directory is not a real directory"));
    }
    if metadata.permissions().mode() & 0o777 != 0o700 {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn open_log_writer(log_dir: &Path) -> Result<(NonBlocking, WorkerGuard), String> {
    if !log_dir.is_dir() {
        return Err(format!(
            "could not open log files in {}: directory is not available",
            log_dir.display()
        ));
    }
    let file = match tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX)
        .build(log_dir)
    {
        Ok(file) => file,
        Err(error) => {
            return Err(format!(
                "could not open log files in {}: {error}",
                log_dir.display()
            ));
        }
    };
    // The appender creates today's file with the process umask. Tighten it,
    // and any older rolling file, before the first log line is written.
    tighten_existing_log_files(log_dir);
    let writer = RestrictingWriter {
        inner: file,
        log_dir: log_dir.to_path_buf(),
        restricted_day: unix_day_number(),
    };
    Ok(tracing_appender::non_blocking(writer))
}

/// Chmods log files created by a later daily rotation. The directory is
/// already `0700`; this keeps the file itself `0600` if that directory is
/// later relaxed. Warnings are not emitted here: a log line must not recurse
/// back into this writer.
struct RestrictingWriter<W> {
    inner: W,
    log_dir: PathBuf,
    restricted_day: i64,
}

impl<W: Write> Write for RestrictingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buf)?;
        self.restrict_if_day_changed();
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()?;
        self.restrict_if_day_changed();
        Ok(())
    }
}

impl<W> RestrictingWriter<W> {
    fn restrict_if_day_changed(&mut self) {
        let day = unix_day_number();
        if day != self.restricted_day {
            tighten_existing_log_files(&self.log_dir);
            self.restricted_day = day;
        }
    }
}

fn unix_day_number() -> i64 {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    i64::try_from(seconds / 86_400).unwrap_or(i64::MAX)
}

fn tighten_existing_log_files(log_dir: &Path) {
    let Ok(entries) = fs::read_dir(log_dir) else {
        return;
    };
    for entry in entries.flatten() {
        if !is_launcher_log_name(&entry.file_name()) {
            continue;
        }
        if entry.file_type().is_ok_and(|kind| kind.is_file()) {
            let _ = tighten_file_mode(&entry.path());
        }
    }
}

fn tighten_file_mode(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.file_type().is_file() {
            return Ok(());
        }
        if metadata.permissions().mode() & 0o777 != 0o600 {
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

/// Startup entry used by tests. [`init`] reports the same warnings after the
/// subscriber is installed, because this runs before any subscriber exists.
#[cfg(test)]
fn scrub_legacy_logs(log_dir: &Path) {
    for warning in scrub_legacy_logs_returning_warnings(log_dir) {
        tracing::warn!("{warning}");
    }
}

fn scrub_legacy_logs_returning_warnings(log_dir: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
    warnings.extend(remove_legacy_auth_callback_log(log_dir));

    let entries = match fs::read_dir(log_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return warnings,
        Err(error) => {
            warnings.push(format!(
                "could not read log directory {} to remove login tokens: {error}",
                log_dir.display()
            ));
            return warnings;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                warnings.push(format!(
                    "could not read an entry in {}: {error}",
                    log_dir.display()
                ));
                continue;
            }
        };
        if !is_launcher_log_name(&entry.file_name()) {
            continue;
        }
        // `file_type` uses lstat, so a symlink is not followed or rewritten.
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                warnings.push(format!(
                    "could not inspect {}: {error}",
                    entry.path().display()
                ));
                continue;
            }
        };
        if !file_type.is_file() {
            continue;
        }
        if let Err(warning) = redact_log_file(&entry.path()) {
            warnings.push(warning);
        }
    }
    warnings
}

fn remove_legacy_auth_callback_log(log_dir: &Path) -> Vec<String> {
    let path = log_dir.join(LEGACY_AUTH_CALLBACK_LOG);
    match fs::remove_file(&path) {
        Ok(()) => Vec::new(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(error) => vec![format!(
            "could not remove legacy auth callback log {}: {error}",
            path.display()
        )],
    }
}

fn redact_log_file(path: &Path) -> Result<(), String> {
    let original = fs::read(path).map_err(|error| {
        format!(
            "could not read {} to remove login tokens: {error}",
            path.display()
        )
    })?;
    let redacted = redact_login_tokens(&original);
    if redacted != original {
        crate::auth::write_private_file(path, &redacted).map_err(|error| {
            format!(
                "could not rewrite {} to remove login tokens: {error:#}",
                path.display()
            )
        })?;
    }
    tighten_file_mode(path).map_err(|error| {
        format!(
            "could not restrict permissions on {}: {error}",
            path.display()
        )
    })?;
    Ok(())
}

fn is_launcher_log_name(name: &OsStr) -> bool {
    name.as_encoded_bytes()
        .starts_with(LOG_FILE_PREFIX.as_bytes())
}

/// Replaces Battle.net `ST=` values, other callback parameters that carry a
/// login token, and any bare login token. Already-redacted text is unchanged.
fn redact_login_tokens(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        if let Some(value) = sensitive_param_value(input, index) {
            output.extend_from_slice(&input[index..value.start]);
            output.extend_from_slice(REDACTED);
            index = value.end;
            continue;
        }
        if is_name_start(input, index) {
            if let Some(len) = crate::auth::login_token_len_at(input, index) {
                output.extend_from_slice(REDACTED);
                index += len;
                continue;
            }
        }
        output.push(input[index]);
        index += 1;
    }
    output
}

fn sensitive_param_value(input: &[u8], index: usize) -> Option<std::ops::Range<usize>> {
    if !is_name_start(input, index) {
        return None;
    }
    let mut name_end = index;
    while name_end < input.len() && is_name_byte(input[name_end]) {
        name_end += 1;
    }
    if name_end == index || input.get(name_end) != Some(&b'=') {
        return None;
    }
    let value_start = name_end + 1;
    let mut value_end = value_start;
    while value_end < input.len() && !is_value_ender(input[value_end]) {
        value_end += 1;
    }
    if value_start == value_end {
        return None;
    }
    let name = &input[index..name_end];
    let value = &input[value_start..value_end];
    let token_value = crate::auth::login_token_len_at(value, 0) == Some(value.len());
    if is_sensitive_param(name) || token_value {
        Some(value_start..value_end)
    } else {
        None
    }
}

fn is_sensitive_param(name: &[u8]) -> bool {
    const NAMES: &[&[u8]] = &[
        b"st",
        b"token",
        b"access_token",
        b"refresh_token",
        b"id_token",
        b"session_token",
        b"auth_token",
        b"login_token",
    ];
    NAMES
        .iter()
        .any(|expected| eq_ignore_ascii_case(name, expected))
}

fn eq_ignore_ascii_case(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
}

fn is_name_start(bytes: &[u8], index: usize) -> bool {
    index == 0 || !is_name_byte(bytes[index - 1])
}

fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_value_ender(byte: u8) -> bool {
    byte.is_ascii_control()
        || byte.is_ascii_whitespace()
        || matches!(
            byte,
            b'&' | b'#' | b'"' | b'\'' | b'<' | b'>' | b'`' | b')' | b']' | b',' | b';'
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    const TOKEN: &str = "US-0123456789ABCDEFGHIJKLMNOPQRSTUV-1234567";

    #[derive(Clone)]
    struct MemWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for MemWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().expect("log buffer").write(buf)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn capture_warnings(f: impl FnOnce()) -> String {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let writer = Arc::clone(&buffer);
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_max_level(tracing::Level::WARN)
            .with_writer(move || MemWriter(Arc::clone(&writer)))
            .finish();
        tracing::subscriber::with_default(subscriber, f);
        let bytes = buffer.lock().expect("log buffer").clone();
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn redacts_callback_token_parameters_and_is_idempotent() {
        let input = format!(
            "uri=blizzard-hearthstone://localhost:0/?ST={TOKEN}&region=US&foo={TOKEN} st=short-secret access_token=abc {TOKEN}\n"
        );
        let once = redact_login_tokens(input.as_bytes());
        let text = String::from_utf8(once.clone()).unwrap();
        assert!(!text.contains(TOKEN), "{text}");
        assert!(text.contains("?ST=REDACTED&region=US&foo=REDACTED"));
        assert!(text.contains("st=REDACTED"));
        assert!(text.contains("access_token=REDACTED"));
        assert!(!text.contains("short-secret"), "{text}");
        assert!(!text.contains("access_token=abc"), "{text}");
        assert_eq!(redact_login_tokens(&once), once);

        let mut binary = b"\xff ST=".to_vec();
        binary.extend_from_slice(TOKEN.as_bytes());
        binary.extend_from_slice(b" \xff");
        let redacted = redact_login_tokens(&binary);
        assert_eq!(redacted.first(), Some(&0xff));
        assert_eq!(redacted.last(), Some(&0xff));
        assert!(!redacted
            .windows(TOKEN.len())
            .any(|window| window == TOKEN.as_bytes()));
        assert_eq!(redact_login_tokens(&redacted), redacted);
    }

    #[test]
    fn leaves_unrelated_assignments_alone() {
        let input = b"LAST=foo token_path=/home/me/token region=US\n";
        assert_eq!(redact_login_tokens(input), input);
    }

    #[cfg(unix)]
    #[test]
    fn rewrites_rolling_logs_and_a_second_pass_does_not_touch_them() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let root = tempfile::tempdir().unwrap();
        let dated = root.path().join("hearthstone-linux-gui.log.2024-05-06");
        let current = root.path().join("hearthstone-linux-gui.log");
        let untouched = root.path().join("notes.log");
        let line = format!(
            "handling auth callback uri=blizzard-hearthstone://localhost:0/?ST={TOKEN}&region=US\n"
        );
        fs::write(&dated, &line).unwrap();
        fs::write(&current, format!("bare {TOKEN}\n")).unwrap();
        fs::write(&untouched, &line).unwrap();
        fs::set_permissions(&dated, fs::Permissions::from_mode(0o644)).unwrap();

        let warnings = capture_warnings(|| scrub_legacy_logs(root.path()));
        assert!(!warnings.contains("WARN"), "{warnings}");
        let redacted = fs::read(&dated).unwrap();
        assert!(!redacted
            .windows(TOKEN.len())
            .any(|window| window == TOKEN.as_bytes()));
        assert!(redacted
            .windows(b"region=US".len())
            .any(|window| window == b"region=US"));
        assert_eq!(
            fs::metadata(&dated).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(!fs::read(&current)
            .unwrap()
            .windows(TOKEN.len())
            .any(|window| window == TOKEN.as_bytes()));
        assert_eq!(fs::read(&untouched).unwrap(), line.as_bytes());

        let inode = fs::metadata(&dated).unwrap().ino();
        let again = capture_warnings(|| scrub_legacy_logs(root.path()));
        assert!(!again.contains("WARN"), "{again}");
        assert_eq!(fs::read(&dated).unwrap(), redacted);
        assert_eq!(fs::metadata(&dated).unwrap().ino(), inode);
    }

    #[cfg(unix)]
    #[test]
    fn skips_symlinks_and_directories() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let line = format!("ST={TOKEN}\n");
        let outside = root.path().join("outside.log");
        fs::write(&outside, &line).unwrap();
        symlink(&outside, root.path().join("hearthstone-linux-gui.log.link")).unwrap();
        let nested = root.path().join("hearthstone-linux-gui.log.d");
        fs::create_dir(&nested).unwrap();
        fs::write(nested.join("inner.log"), &line).unwrap();
        let regular = root.path().join("hearthstone-linux-gui.log.2020-01-01");
        fs::write(&regular, &line).unwrap();

        let warnings = capture_warnings(|| scrub_legacy_logs(root.path()));
        assert!(!warnings.contains("WARN"), "{warnings}");
        assert_eq!(fs::read(&outside).unwrap(), line.as_bytes());
        assert!(
            fs::symlink_metadata(root.path().join("hearthstone-linux-gui.log.link"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(nested.join("inner.log")).unwrap(), line.as_bytes());
        assert!(nested.is_dir());
        assert!(!fs::read(&regular)
            .unwrap()
            .windows(TOKEN.len())
            .any(|window| window == TOKEN.as_bytes()));
    }

    #[cfg(unix)]
    #[test]
    fn unwritable_log_directory_warns_and_keeps_secrets() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let log_dir = root.path().join("logs");
        fs::create_dir(&log_dir).unwrap();
        let line = format!("uri=?ST={TOKEN}&keep=yes\n");
        let first = log_dir.join("hearthstone-linux-gui.log");
        let second = log_dir.join("hearthstone-linux-gui.log.2024-01-01");
        fs::write(&first, &line).unwrap();
        fs::write(&second, &line).unwrap();
        fs::write(log_dir.join(LEGACY_AUTH_CALLBACK_LOG), b"old token").unwrap();
        fs::set_permissions(&log_dir, fs::Permissions::from_mode(0o555)).unwrap();
        // Root ignores mode bits, so the package build can still create files
        // here. Only require a warning when the directory actually rejects writes.
        let blocked = !can_create_file(&log_dir);

        let warnings = capture_warnings(|| scrub_legacy_logs(&log_dir));
        fs::set_permissions(&log_dir, fs::Permissions::from_mode(0o755)).unwrap();

        if blocked {
            assert!(
                warnings.contains("could not rewrite"),
                "expected a rewrite warning, got {warnings}"
            );
            assert!(
                warnings.contains("could not remove legacy auth callback log"),
                "expected a delete warning, got {warnings}"
            );
            assert_eq!(fs::read(&first).unwrap(), line.as_bytes());
            assert_eq!(fs::read(&second).unwrap(), line.as_bytes());
            assert_eq!(
                fs::read(log_dir.join(LEGACY_AUTH_CALLBACK_LOG)).unwrap(),
                b"old token"
            );
        } else {
            assert!(!warnings.contains("WARN"), "{warnings}");
            assert!(!fs::read(&first)
                .unwrap()
                .windows(TOKEN.len())
                .any(|window| window == TOKEN.as_bytes()));
            assert!(!fs::read(&second)
                .unwrap()
                .windows(TOKEN.len())
                .any(|window| window == TOKEN.as_bytes()));
            assert!(!log_dir.join(LEGACY_AUTH_CALLBACK_LOG).exists());
        }
    }

    #[cfg(unix)]
    fn can_create_file(dir: &Path) -> bool {
        let probe = dir.join(".write-probe");
        match fs::write(&probe, b"x") {
            Ok(()) => {
                let _ = fs::remove_file(&probe);
                true
            }
            Err(_) => false,
        }
    }

    #[test]
    fn removes_legacy_auth_callback_log_and_ignores_a_missing_one() {
        let root = tempfile::tempdir().unwrap();
        let log = root.path().join(LEGACY_AUTH_CALLBACK_LOG);
        fs::write(&log, b"callback uri").unwrap();
        let removed = capture_warnings(|| scrub_legacy_logs(root.path()));
        assert!(!removed.contains("WARN"), "{removed}");
        assert!(!log.exists());

        let missing = capture_warnings(|| scrub_legacy_logs(root.path()));
        assert!(!missing.contains("WARN"), "{missing}");
    }

    #[cfg(unix)]
    #[test]
    fn legacy_auth_callback_log_directory_warns_and_cleanup_continues() {
        let root = tempfile::tempdir().unwrap();
        let blocked = root.path().join(LEGACY_AUTH_CALLBACK_LOG);
        fs::create_dir(&blocked).unwrap();
        let log = root.path().join("hearthstone-linux-gui.log");
        fs::write(&log, format!("ST={TOKEN}\n")).unwrap();

        let warnings = capture_warnings(|| scrub_legacy_logs(root.path()));
        assert!(
            warnings.contains("could not remove legacy auth callback log"),
            "{warnings}"
        );
        assert!(blocked.is_dir());
        assert!(!fs::read(&log)
            .unwrap()
            .windows(TOKEN.len())
            .any(|window| window == TOKEN.as_bytes()));
    }

    #[cfg(unix)]
    #[test]
    fn log_directory_and_log_files_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let log_dir = root.path().join("logs");
        fs::create_dir(&log_dir).unwrap();
        fs::set_permissions(&log_dir, fs::Permissions::from_mode(0o755)).unwrap();
        let existing = log_dir.join("hearthstone-linux-gui.log.2020-01-01");
        fs::write(&existing, b"plain log line\n").unwrap();
        fs::set_permissions(&existing, fs::Permissions::from_mode(0o644)).unwrap();

        let warnings = prepare_log_dir(&log_dir);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(mode(&log_dir), 0o700);
        assert_eq!(mode(&existing), 0o600);
        assert_eq!(fs::read(&existing).unwrap(), b"plain log line\n");

        let (writer, guard) = open_log_writer(&log_dir).expect("log writer");
        drop(writer);
        drop(guard);

        let mut saw_log = false;
        for entry in fs::read_dir(&log_dir).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() && is_launcher_log_name(&entry.file_name()) {
                assert_eq!(mode(&entry.path()), 0o600, "{}", entry.path().display());
                saw_log = true;
            }
        }
        assert!(saw_log);
        assert_eq!(mode(&log_dir), 0o700);
    }

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }
}
