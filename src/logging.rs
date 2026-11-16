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
const SCRUB_MARKER: &str = ".token-scrub-v1";
/// Daily files kept by the rolling appender. tracing-appender may retain one
/// fewer than this while it rotates, which is still about two weeks.
const MAX_LOG_FILES: usize = 14;
/// Longer lines are not parsed. A line this long that still contains a login
/// token is replaced in one step so a hostile log cannot stall startup.
const MAX_SCRUB_LINE: usize = 64 * 1024;
const REDACTED: &[u8] = b"REDACTED";

#[derive(Default)]
struct StartupNotes {
    warnings: Vec<String>,
    debug: Vec<String>,
}

pub fn init() -> Option<WorkerGuard> {
    // Scrub before the rolling appender opens today's file. Rewriting a log
    // that is already open would leave the token in the inode the appender
    // still holds. A marker skips the reread after the first successful pass,
    // and short-lived helper invocations never do it.
    let paths = AppPaths::discover().ok();
    let mut notes = paths
        .as_ref()
        .map(|paths| {
            prepare_startup_logs(
                &paths.log_dir,
                Some(&legacy_hsreplay_token_tmp(&paths.hsreplay_token())),
                scrub_legacy_logs_for_this_process(),
            )
        })
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
            notes.warnings.push(warning);
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

    for warning in notes.warnings {
        tracing::warn!("{warning}");
    }
    for note in notes.debug {
        tracing::debug!("{note}");
    }
    guard
}

/// The main launcher scrubs once. `--help`, login callbacks, and the other
/// short-lived commands must not reread every historical log on the way out.
fn scrub_legacy_logs_for_this_process() -> bool {
    invocation_scrubs_legacy_logs(std::env::args().skip(1))
}

fn invocation_scrubs_legacy_logs(args: impl IntoIterator<Item = impl AsRef<str>>) -> bool {
    let args: Vec<String> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_owned())
        .collect();
    if args.iter().any(|arg| is_short_lived_flag(arg)) {
        return false;
    }
    // A bare callback URI is positional. Values of `--region` and `--locale`
    // are not.
    let mut skip_value = false;
    for arg in &args {
        if skip_value && !arg.starts_with('-') {
            skip_value = false;
            continue;
        }
        skip_value = false;
        if let Some((name, _)) = arg.split_once('=') {
            if flag_takes_value(name) {
                continue;
            }
        }
        if flag_takes_value(arg) {
            skip_value = true;
            continue;
        }
        if !arg.starts_with('-') {
            return false;
        }
    }
    true
}

fn is_short_lived_flag(arg: &str) -> bool {
    matches!(
        arg,
        "-h" | "--help" | "--print-child-env" | "--no-gui" | "--auth-callback" | "--write-token"
    ) || arg.starts_with("--auth-callback=")
        || arg.starts_with("--write-token=")
}

fn flag_takes_value(arg: &str) -> bool {
    matches!(
        arg,
        "--region" | "--locale" | "--auth-callback" | "--write-token"
    )
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

/// Tightens the log directory and, when `scrub_logs` is set, rewrites rolling
/// logs that still contain login tokens. Problems are warnings; none of them
/// stop startup.
fn prepare_startup_logs(
    log_dir: &Path,
    legacy_token_tmp: Option<&Path>,
    scrub_logs: bool,
) -> StartupNotes {
    let mut notes = StartupNotes::default();
    if is_symlink(log_dir) {
        // A symlink is an intentional redirect. Chmod would follow it, and
        // warning on every start would drown the log.
        notes.debug.push(format!(
            "log directory {} is a symlink; not changing its permissions",
            log_dir.display()
        ));
    } else if let Err(error) = ensure_private_log_dir(log_dir) {
        notes.warnings.push(format!(
            "could not restrict log directory {}: {error}",
            log_dir.display()
        ));
    }
    if scrub_logs {
        notes
            .warnings
            .extend(scrub_legacy_logs_with_token_tmp(log_dir, legacy_token_tmp));
    }
    notes
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
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
        .max_log_files(MAX_LOG_FILES)
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

/// Chmods log files created by a later daily rotation. tracing-appender 0.2
/// opens those files itself and has no creation mode, so the mode is set on
/// the write that creates the file. The directory is already `0700`.
/// Warnings are not emitted here: a log line must not recurse into this writer.
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
    for warning in scrub_legacy_logs_with_token_tmp(log_dir, None) {
        tracing::warn!("{warning}");
    }
}

fn scrub_legacy_logs_with_token_tmp(
    log_dir: &Path,
    legacy_token_tmp: Option<&Path>,
) -> Vec<String> {
    // The marker is written only after a clean pass, so a failure retries.
    if marker_is_present(log_dir) {
        return remove_legacy_auth_callback_log(log_dir);
    }

    let mut warnings = remove_legacy_auth_callback_log(log_dir);
    warnings.extend(scrub_log_files(log_dir));
    if let Some(legacy_token_tmp) = legacy_token_tmp {
        warnings.extend(remove_regular_file(
            legacy_token_tmp,
            "stale token temporary file",
        ));
    }
    if warnings.is_empty() {
        if let Err(warning) = write_scrub_marker(log_dir) {
            warnings.push(warning);
        }
    }
    warnings
}

fn marker_is_present(log_dir: &Path) -> bool {
    fs::symlink_metadata(log_dir.join(SCRUB_MARKER))
        .map(|metadata| metadata.file_type().is_file())
        .unwrap_or(false)
}

fn write_scrub_marker(log_dir: &Path) -> Result<(), String> {
    crate::auth::write_private_file(&log_dir.join(SCRUB_MARKER), b"v1\n").map_err(|error| {
        format!(
            "could not record log token scrub in {}: {error:#}",
            log_dir.display()
        )
    })
}

/// Old HSReplay saves used a fixed `token.json.tmp` name beside the token.
fn legacy_hsreplay_token_tmp(token_file: &Path) -> PathBuf {
    let mut temporary = token_file.as_os_str().to_owned();
    temporary.push(".tmp");
    PathBuf::from(temporary)
}

fn remove_regular_file(path: &Path, what: &str) -> Vec<String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => match fs::remove_file(path) {
            Ok(()) => Vec::new(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => vec![format!(
                "could not remove {what} {}: {error}",
                path.display()
            )],
        },
        Ok(_) => Vec::new(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(error) => vec![format!(
            "could not inspect {what} {}: {error}",
            path.display()
        )],
    }
}

fn scrub_log_files(log_dir: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
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

/// Replaces Battle.net `ST=` values (and `ST%3D`, either case), other callback
/// parameters that carry a login token, and any bare login token.
/// Already-redacted text is unchanged.
///
/// The scan is linear. A non-sensitive `name=` does not walk its value; the
/// cursor stays on the value so a later `ST=` is still found, without rereading
/// the suffix from every byte.
fn redact_login_tokens(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut start = 0;
    while start < input.len() {
        let mut end = start;
        while end < input.len() && input[end] != b'\n' {
            end += 1;
        }
        let line = &input[start..end];
        if line.len() > MAX_SCRUB_LINE && line_has_sensitive_marker(line) {
            output.extend_from_slice(b"REDACTED");
        } else {
            redact_line(line, &mut output);
        }
        if end < input.len() {
            output.push(b'\n');
            start = end + 1;
        } else {
            break;
        }
    }
    output
}

fn redact_line(line: &[u8], output: &mut Vec<u8>) {
    let mut index = 0;
    while index < line.len() {
        if let Some(found) = assignment_at(line, index) {
            let name = &line[index..found.name_end];
            if is_sensitive_param(name) {
                let value_end = value_end(line, found.value_start);
                if value_end > found.value_start {
                    output.extend_from_slice(&line[index..found.value_start]);
                    output.extend_from_slice(REDACTED);
                    index = value_end;
                    continue;
                }
            } else if let Some(len) = exact_token_len(line, found.value_start) {
                output.extend_from_slice(&line[index..found.value_start]);
                output.extend_from_slice(REDACTED);
                index = found.value_start + len;
                continue;
            }
            output.extend_from_slice(&line[index..found.value_start]);
            index = found.value_start;
            continue;
        }
        if is_name_start(line, index) {
            if let Some(len) = crate::auth::login_token_len_at(line, index) {
                output.extend_from_slice(REDACTED);
                index += len;
                continue;
            }
        }
        output.push(line[index]);
        index += 1;
    }
}

struct Assignment {
    name_end: usize,
    value_start: usize,
}

fn assignment_at(input: &[u8], index: usize) -> Option<Assignment> {
    if !is_name_start(input, index) {
        return None;
    }
    let mut name_end = index;
    while name_end < input.len() && is_name_byte(input[name_end]) {
        name_end += 1;
    }
    if name_end == index {
        return None;
    }
    let separator = separator_len(input, name_end)?;
    Some(Assignment {
        name_end,
        value_start: name_end + separator,
    })
}

fn value_end(input: &[u8], value_start: usize) -> usize {
    let mut value_end = value_start;
    while value_end < input.len() && !is_value_ender(input[value_end]) {
        value_end += 1;
    }
    value_end
}

/// Token length when the value is exactly one login token.
fn exact_token_len(input: &[u8], value_start: usize) -> Option<usize> {
    let len = crate::auth::login_token_len_at(input, value_start)?;
    let after = value_start + len;
    if after == input.len() || is_value_ender(input[after]) {
        Some(len)
    } else {
        None
    }
}

fn line_has_sensitive_marker(line: &[u8]) -> bool {
    let mut index = 0;
    while index < line.len() {
        if let Some(found) = assignment_at(line, index) {
            if is_sensitive_param(&line[index..found.name_end])
                || exact_token_len(line, found.value_start).is_some()
            {
                return true;
            }
            index = found.value_start;
            continue;
        }
        if is_name_start(line, index) && crate::auth::login_token_len_at(line, index).is_some() {
            return true;
        }
        index += 1;
    }
    false
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

/// `=` or a percent-encoded `=` (`%3D` / `%3d`).
fn separator_len(input: &[u8], index: usize) -> Option<usize> {
    if input.get(index) == Some(&b'=') {
        return Some(1);
    }
    let encoded = input.get(index..index + 3)?;
    if encoded[0] == b'%' && encoded[1] == b'3' && encoded[2].eq_ignore_ascii_case(&b'd') {
        Some(3)
    } else {
        None
    }
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
        let input = b"LAST=foo token_path=/home/me/token region=US LAST%3Dfoo\n";
        assert_eq!(redact_login_tokens(input), input);
    }

    #[test]
    fn redacts_percent_encoded_st_parameter() {
        let input = format!("uri=?ST%3D{TOKEN}&x=1 st%3dshort-secret foo%3D{TOKEN}");
        let once = redact_login_tokens(input.as_bytes());
        let text = String::from_utf8(once.clone()).unwrap();
        assert!(!text.contains(TOKEN), "{text}");
        assert!(text.contains("?ST%3DREDACTED&x=1"), "{text}");
        assert!(text.contains("st%3dREDACTED"), "{text}");
        assert!(text.contains("foo%3DREDACTED"), "{text}");
        assert!(!text.contains("short-secret"), "{text}");
        assert_eq!(redact_login_tokens(&once), once);
    }

    #[test]
    fn oversized_line_with_a_token_is_replaced_wholesale() {
        let mut line = vec![b'x'; MAX_SCRUB_LINE + 8];
        line.extend_from_slice(format!("ST={TOKEN}").as_bytes());
        line.push(b'\n');
        line.extend_from_slice(b"next ST=kept-out\n");
        let text = String::from_utf8(redact_login_tokens(&line)).unwrap();
        assert_eq!(text, "REDACTED\nnext ST=REDACTED\n");
        assert!(!text.contains(TOKEN));
    }

    #[test]
    fn pathological_assignments_stay_linear() {
        let mut input = Vec::with_capacity(1024 * 1024);
        while input.len() + 2 <= 1024 * 1024 {
            input.extend_from_slice(b"a=");
        }
        let started = std::time::Instant::now();
        let output = redact_login_tokens(&input);
        let elapsed = started.elapsed();
        assert_eq!(output, input);
        assert!(
            elapsed < std::time::Duration::from_secs(1),
            "redaction took {elapsed:?}"
        );
    }

    #[test]
    fn main_launcher_scrubs_and_short_invocations_do_not() {
        assert!(invocation_scrubs_legacy_logs(std::iter::empty::<&str>()));
        assert!(invocation_scrubs_legacy_logs(["--launch"]));
        assert!(invocation_scrubs_legacy_logs([
            "--install",
            "--region",
            "eu"
        ]));
        assert!(!invocation_scrubs_legacy_logs(["--help"]));
        assert!(!invocation_scrubs_legacy_logs(["-h"]));
        assert!(!invocation_scrubs_legacy_logs([
            "--auth-callback",
            "wtcg://x"
        ]));
        assert!(!invocation_scrubs_legacy_logs(["--auth-callback=wtcg://x"]));
        assert!(!invocation_scrubs_legacy_logs(["--no-gui"]));
        assert!(!invocation_scrubs_legacy_logs(["--print-child-env"]));
        assert!(!invocation_scrubs_legacy_logs(["--write-token", TOKEN]));
        assert!(!invocation_scrubs_legacy_logs([
            "blizzard-hearthstone://localhost:0/?ST=x"
        ]));
    }

    #[test]
    fn scrub_marker_skips_later_passes_and_short_invocations_do_not_write_it() {
        let root = tempfile::tempdir().unwrap();
        let log = root.path().join("hearthstone-linux-gui.log");
        fs::write(&log, format!("ST={TOKEN}\n")).unwrap();

        let skipped = prepare_startup_logs(root.path(), None, false);
        assert!(skipped.warnings.is_empty(), "{:?}", skipped.warnings);
        assert!(fs::read(&log)
            .unwrap()
            .windows(TOKEN.len())
            .any(|window| window == TOKEN.as_bytes()));
        assert!(!root.path().join(SCRUB_MARKER).exists());

        let warnings = capture_warnings(|| scrub_legacy_logs(root.path()));
        assert!(!warnings.contains("WARN"), "{warnings}");
        assert!(!fs::read(&log)
            .unwrap()
            .windows(TOKEN.len())
            .any(|window| window == TOKEN.as_bytes()));
        let marker = root.path().join(SCRUB_MARKER);
        assert!(marker.is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&marker).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }

        fs::write(&log, format!("ST={TOKEN}\n")).unwrap();
        let again = capture_warnings(|| scrub_legacy_logs(root.path()));
        assert!(!again.contains("WARN"), "{again}");
        assert!(fs::read(&log)
            .unwrap()
            .windows(TOKEN.len())
            .any(|window| window == TOKEN.as_bytes()));
    }

    #[cfg(unix)]
    #[test]
    fn removes_a_regular_legacy_token_temp_file_and_leaves_other_types() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let token = root.path().join("hsreplay-token.json");
        let temporary = legacy_hsreplay_token_tmp(&token);
        fs::write(&temporary, b"partial").unwrap();
        let warnings = scrub_legacy_logs_with_token_tmp(root.path(), Some(&temporary));
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(!temporary.exists());

        fs::create_dir(&temporary).unwrap();
        assert!(remove_regular_file(&temporary, "stale token temporary file").is_empty());
        assert!(temporary.is_dir());
        fs::remove_dir(&temporary).unwrap();

        let target = root.path().join("elsewhere");
        fs::write(&target, b"keep").unwrap();
        symlink(&target, &temporary).unwrap();
        assert!(remove_regular_file(&temporary, "stale token temporary file").is_empty());
        assert!(fs::symlink_metadata(&temporary)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read(&target).unwrap(), b"keep");
    }

    #[cfg(unix)]
    #[test]
    fn symlink_log_directory_is_debug_not_a_repeated_warning() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("real-logs");
        fs::create_dir(&real).unwrap();
        fs::write(
            real.join("hearthstone-linux-gui.log"),
            format!("ST={TOKEN}\n"),
        )
        .unwrap();
        let link = root.path().join("logs");
        symlink(&real, &link).unwrap();

        let first = prepare_startup_logs(&link, None, true);
        assert!(first.warnings.is_empty(), "{:?}", first.warnings);
        assert!(first.debug.iter().any(|note| note.contains("symlink")));
        assert!(!fs::read(real.join("hearthstone-linux-gui.log"))
            .unwrap()
            .windows(TOKEN.len())
            .any(|window| window == TOKEN.as_bytes()));

        fs::write(
            real.join("hearthstone-linux-gui.log"),
            format!("ST={TOKEN}\n"),
        )
        .unwrap();
        let second = prepare_startup_logs(&link, None, true);
        assert!(second.warnings.is_empty(), "{:?}", second.warnings);
        assert!(fs::read(real.join("hearthstone-linux-gui.log"))
            .unwrap()
            .windows(TOKEN.len())
            .any(|window| window == TOKEN.as_bytes()));
    }

    #[cfg(unix)]
    #[test]
    fn rolling_appender_drops_logs_beyond_the_retention_cap() {
        let root = tempfile::tempdir().unwrap();
        let log_dir = root.path().join("logs");
        fs::create_dir(&log_dir).unwrap();
        for day in 1..=20 {
            fs::write(
                log_dir.join(format!("hearthstone-linux-gui.log.2020-01-{day:02}")),
                b"old\n",
            )
            .unwrap();
        }
        let (writer, guard) = open_log_writer(&log_dir).expect("log writer");
        drop(writer);
        drop(guard);
        let count = fs::read_dir(&log_dir)
            .unwrap()
            .filter(|entry| {
                entry.as_ref().ok().is_some_and(|entry| {
                    entry.file_type().is_ok_and(|kind| kind.is_file())
                        && is_launcher_log_name(&entry.file_name())
                })
            })
            .count();
        assert!(count <= MAX_LOG_FILES, "{count}");
        assert!(count > 1, "{count}");
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

        let notes = prepare_startup_logs(&log_dir, None, true);
        assert!(notes.warnings.is_empty(), "{:?}", notes.warnings);
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
