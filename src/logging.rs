use crate::paths::AppPaths;
use std::ffi::OsStr;
use std::fs;
#[cfg(unix)]
use std::io::Read;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

const LOG_FILE_PREFIX: &str = "hearthstone-linux-gui.log";
const LEGACY_AUTH_CALLBACK_LOG: &str = "auth-callback.log";
const SCRUB_MARKER: &str = ".token-scrub-v1";
/// Days of rotated logs to keep. The date in the filename decides this, not
/// mtime or ctime: rewriting a log gives it a new creation time, and
/// tracing-appender's `max_log_files` would then keep that rewritten file and
/// delete a genuinely newer one. A filename date more than this many days
/// before today (UTC) is outside the window.
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
    // Drop expired rotated logs, then scrub, before the rolling appender opens
    // today's file. Rewriting a log that is already open would leave the token
    // in the inode the appender still holds, and rewriting first would make an
    // old file look newly created. A marker skips the reread after the first
    // successful pass, and short-lived helper invocations never do it.
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
    } else {
        // Short-lived commands skip the token rewrite but still enforce the
        // filename retention window. They must not leave that to creation time.
        notes.warnings.extend(delete_expired_rotated_logs(log_dir));
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
    // Filename retention again, including launches that never scrub. Expired
    // files are already gone before any rewrite can refresh their timestamps.
    let _ = delete_expired_rotated_logs(log_dir);
    // Retention is by filename date (see `delete_expired_rotated_logs`), not
    // tracing-appender's creation-time prune. That prune also matches every
    // name that merely starts with the log prefix.
    //
    // Create today's file as `0600` before the appender opens it. The appender
    // has no creation mode; a file that already exists is opened, not recreated.
    let today = unix_day_number();
    precreate_private_log(log_dir, today);
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
        restricted_day: today,
    };
    Ok(tracing_appender::non_blocking(writer))
}

/// Pre-creates the next day's log as `0600` before tracing-appender opens it.
/// The appender has no creation mode. A log line must not recurse into this
/// writer, so retention and chmod failures are ignored here. The directory is
/// already `0700`.
struct RestrictingWriter<W> {
    inner: W,
    log_dir: PathBuf,
    restricted_day: i64,
}

impl<W: Write> Write for RestrictingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.prepare_today();
        let written = self.inner.write(buf)?;
        self.restrict_if_day_changed();
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.prepare_today();
        self.inner.flush()?;
        self.restrict_if_day_changed();
        Ok(())
    }
}

impl<W> RestrictingWriter<W> {
    fn prepare_today(&mut self) {
        let day = unix_day_number();
        if day == self.restricted_day {
            return;
        }
        // Before the appender creates the new day's file with the process umask.
        let _ = delete_expired_rotated_logs(&self.log_dir);
        precreate_private_log(&self.log_dir, day);
    }

    fn restrict_if_day_changed(&mut self) {
        let day = unix_day_number();
        if day != self.restricted_day {
            tighten_existing_log_files(&self.log_dir);
            self.restricted_day = day;
        }
    }
}

fn unix_day_number() -> i64 {
    utc_unix_day(SystemTime::now())
}

fn utc_unix_day(now: SystemTime) -> i64 {
    let seconds = now
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    i64::try_from(seconds / 86_400).unwrap_or(i64::MAX)
}

/// `YYYY-MM-DD` for a UTC day count, matching tracing-appender's daily suffix.
#[cfg(unix)]
fn dated_log_name(day: i64) -> String {
    let (year, month, day_of_month) = date_from_unix_day(day);
    format!("{LOG_FILE_PREFIX}.{year:04}-{month:02}-{day_of_month:02}")
}

/// Creates `hearthstone-linux-gui.log.YYYY-MM-DD` as mode `0600` when it does
/// not already exist. `OpenOptions::mode` is applied by `open(2)` before the
/// file is visible; the process umask can only clear bits, and `0600` has no
/// group or other bits for a typical umask to remove.
fn precreate_private_log(log_dir: &Path, day: i64) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let path = log_dir.join(dated_log_name(day));
        let _ = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path);
    }
    #[cfg(not(unix))]
    {
        let _ = (log_dir, day);
    }
}

fn filename_date_outside_retention(file_day: i64, today: i64) -> bool {
    let window = i64::try_from(MAX_LOG_FILES).unwrap_or(i64::MAX);
    today.saturating_sub(file_day) > window
}

/// Removes rotated logs whose filename date is outside the retention window.
/// Names that only share the prefix, symlinks, and directories are left alone.
/// The file's mtime and ctime are not consulted.
///
/// The newest [`MAX_LOG_FILES`] dated logs are always kept. A file is removed
/// only when it is older than those and its filename date is outside the
/// window. A clock set far ahead still cannot drop the whole history: today's
/// pre-created file does not make every older log eligible.
fn delete_expired_rotated_logs(log_dir: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
    let today = unix_day_number();
    let entries = match fs::read_dir(log_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return warnings,
        Err(error) => {
            warnings.push(format!(
                "could not read log directory {} to apply log retention: {error}",
                log_dir.display()
            ));
            return warnings;
        }
    };

    let mut dated = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                warnings.push(format!(
                    "could not read an entry in {} while applying log retention: {error}",
                    log_dir.display()
                ));
                continue;
            }
        };
        let Some(file_day) = rotated_log_day(&entry.file_name()) else {
            continue;
        };
        match fs::symlink_metadata(entry.path()) {
            Ok(metadata) if metadata.file_type().is_file() => {
                dated.push((file_day, entry.path()));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => warnings.push(format!(
                "could not inspect {} while applying log retention: {error}",
                entry.path().display()
            )),
        }
    }
    dated.sort_by_key(|(day, _)| std::cmp::Reverse(*day));

    for (file_day, path) in dated.into_iter().skip(MAX_LOG_FILES) {
        if !filename_date_outside_retention(file_day, today) {
            continue;
        }
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_file() => {
                if let Err(error) = fs::remove_file(&path) {
                    if error.kind() != io::ErrorKind::NotFound {
                        warnings.push(format!(
                            "could not remove expired log {}: {error}",
                            path.display()
                        ));
                    }
                }
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => warnings.push(format!(
                "could not inspect expired log {}: {error}",
                path.display()
            )),
        }
    }
    warnings
}

/// Day count for `prefix.YYYY-MM-DD` only. Any other suffix is not a rotated log.
fn rotated_log_day(name: &OsStr) -> Option<i64> {
    let bytes = name.as_encoded_bytes();
    let rest = bytes.strip_prefix(LOG_FILE_PREFIX.as_bytes())?;
    let rest = rest.strip_prefix(b".")?;
    if rest.len() != 10 || rest[4] != b'-' || rest[7] != b'-' {
        return None;
    }
    if !rest
        .iter()
        .enumerate()
        .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
    {
        return None;
    }
    let year = i32::try_from(parse_u32(&rest[0..4])?).ok()?;
    let month = parse_u32(&rest[5..7])?;
    let day = parse_u32(&rest[8..10])?;
    unix_day_from_date(year, month, day)
}

fn parse_u32(bytes: &[u8]) -> Option<u32> {
    let mut value = 0u32;
    for byte in bytes {
        let digit = u32::from(byte.wrapping_sub(b'0'));
        if digit > 9 {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(digit)?;
    }
    Some(value)
}

fn unix_day_from_date(year: i32, month: u32, day: u32) -> Option<i64> {
    if !is_valid_civil_date(year, month, day) {
        return None;
    }
    // Howard Hinnant, `days_from_civil`.
    let year = i64::from(year) - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = (year - era * 400) as u64;
    let month_prime = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * u64::from(month_prime) + 2) / 5 + u64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146097 + i64::try_from(day_of_era).ok()? - 719468)
}

#[cfg(unix)]
fn date_from_unix_day(mut days: i64) -> (i32, u32, u32) {
    // Howard Hinnant, `civil_from_days`.
    days += 719468;
    let era = days.div_euclid(146097);
    let day_of_era = (days - era * 146097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    (i32::try_from(year).unwrap_or(0), month as u32, day as u32)
}

fn is_valid_civil_date(year: i32, month: u32, day: u32) -> bool {
    if !(1..=12).contains(&month) || day == 0 || year < 1 {
        return false;
    }
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => return false,
    };
    day <= max_day
}

fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
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
        // Open with `O_NOFOLLOW`, then `fstat` and `fchmod` that descriptor.
        // A path `chmod` would follow a symlink swapped in after the listing.
        let file = match open_nofollow(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound || is_eloop(&error) => {
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        restrict_log_fd(&file)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

#[cfg(unix)]
fn open_nofollow(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

/// `O_NOFOLLOW` fails with `ELOOP` when the path is a symlink.
/// `ErrorKind::FilesystemLoop` is still unstable (`io_error_more`) on the 1.92
/// MSRV, so this compares `libc::ELOOP`, which is the target's own errno.
#[cfg(unix)]
fn is_eloop(error: &io::Error) -> bool {
    error.raw_os_error() == Some(libc::ELOOP)
}

#[cfg(unix)]
fn restrict_log_fd(file: &fs::File) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    // `File::metadata` is `fstat` on this descriptor, not a path lookup.
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Ok(());
    }
    if metadata.permissions().mode() & 0o777 != 0o600 {
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
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
    // Delete by filename date before any rewrite. A rewrite replaces the file,
    // so the old log would otherwise look newer than logs that were left alone.
    let mut warnings = delete_expired_rotated_logs(log_dir);
    // The marker is written only after a clean pass, so a failure retries.
    if marker_is_present(log_dir) {
        warnings.extend(remove_legacy_auth_callback_log(log_dir));
        return warnings;
    }

    warnings.extend(remove_legacy_auth_callback_log(log_dir));
    let (file_warnings, hard_links) = scrub_log_files(log_dir);
    warnings.extend(file_warnings);
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
    // A hard link is skipped on purpose. Reporting it must not withhold the
    // marker, or every later startup would scan and warn again.
    warnings.extend(hard_links);
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

fn scrub_log_files(log_dir: &Path) -> (Vec<String>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut hard_links = Vec::new();
    let entries = match fs::read_dir(log_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return (warnings, hard_links),
        Err(error) => {
            warnings.push(format!(
                "could not read log directory {} to remove login tokens: {error}",
                log_dir.display()
            ));
            return (warnings, hard_links);
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
        match redact_log_file(&entry.path()) {
            Ok(RedactOutcome::Done) => {}
            Ok(RedactOutcome::HardLink) => hard_links.push(format!(
                "skipping {} because it has multiple hard links; permissions were tightened and the shared file was not rewritten",
                entry.path().display()
            )),
            Err(warning) => warnings.push(warning),
        }
    }
    (warnings, hard_links)
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

enum RedactOutcome {
    Done,
    /// Present on every platform so the scrub loop can name it. Only Unix
    /// opens a log with `fstat` and produces this outcome.
    #[cfg_attr(not(unix), allow(dead_code))]
    HardLink,
}

fn redact_log_file(path: &Path) -> Result<RedactOutcome, String> {
    #[cfg(unix)]
    {
        redact_log_file_unix(path)
    }
    #[cfg(not(unix))]
    {
        let original = fs::read(path).map_err(|error| {
            format!(
                "could not read {} to remove login tokens: {error}",
                path.display()
            )
        })?;
        rewrite_redacted_log(path, &original)?;
        Ok(RedactOutcome::Done)
    }
}

/// Opens with `O_NOFOLLOW` and `fstat`s that descriptor before reading. A
/// symlink swapped in after the directory listing fails the open instead of
/// being followed. A hard link is mode `0600` and then left unread: rewriting
/// the inode would change every name, and replacing this name would leave the
/// token on the other link.
#[cfg(unix)]
fn redact_log_file_unix(path: &Path) -> Result<RedactOutcome, String> {
    use std::os::unix::fs::MetadataExt;
    let mut file = match open_nofollow(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound || is_eloop(&error) => {
            return Ok(RedactOutcome::Done);
        }
        Err(error) => {
            return Err(format!(
                "could not read {} to remove login tokens: {error}",
                path.display()
            ));
        }
    };
    let metadata = file.metadata().map_err(|error| {
        format!(
            "could not inspect {} to remove login tokens: {error}",
            path.display()
        )
    })?;
    if !metadata.is_file() {
        return Ok(RedactOutcome::Done);
    }
    if metadata.nlink() > 1 {
        restrict_log_fd(&file).map_err(|error| {
            format!(
                "could not restrict permissions on {}: {error}",
                path.display()
            )
        })?;
        return Ok(RedactOutcome::HardLink);
    }
    let mut original = Vec::new();
    file.read_to_end(&mut original).map_err(|error| {
        format!(
            "could not read {} to remove login tokens: {error}",
            path.display()
        )
    })?;
    let redacted = redact_login_tokens(&original);
    if redacted != original {
        // Drop the read fd before replacing the directory entry.
        drop(file);
        crate::auth::write_private_file(path, &redacted).map_err(|error| {
            format!(
                "could not rewrite {} to remove login tokens: {error:#}",
                path.display()
            )
        })?;
        return Ok(RedactOutcome::Done);
    }
    restrict_log_fd(&file).map_err(|error| {
        format!(
            "could not restrict permissions on {}: {error}",
            path.display()
        )
    })?;
    Ok(RedactOutcome::Done)
}

#[cfg(not(unix))]
fn rewrite_redacted_log(path: &Path, original: &[u8]) -> Result<(), String> {
    let redacted = redact_login_tokens(original);
    if redacted != original {
        crate::auth::write_private_file(path, &redacted).map_err(|error| {
            format!(
                "could not rewrite {} to remove login tokens: {error:#}",
                path.display()
            )
        })?;
    }
    Ok(())
}

/// The active log (`hearthstone-linux-gui.log`) or one daily rotation
/// (`hearthstone-linux-gui.log.YYYY-MM-DD`). A name that only shares the
/// prefix is the user's file and is not scrubbed, chmod'd, or deleted.
fn is_launcher_log_name(name: &OsStr) -> bool {
    name.as_encoded_bytes() == LOG_FILE_PREFIX.as_bytes() || rotated_log_day(name).is_some()
}

/// Replaces Battle.net `ST=` values (and percent-encoded forms of `=`, `?`, and
/// `&`, including a once-doubled `=`), other callback parameters that carry a
/// login token, and any bare login token. Already-redacted text is unchanged.
///
/// The scan is linear. Each byte is classified with a fixed-size lookahead, and
/// a non-sensitive `name=` does not walk its value; the cursor stays on the
/// value so a later `ST=` is still found, without rereading the suffix.
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
        // `%3F` / `%26` (and `%253F` / `%2526`) are `?` / `&`. Skip them before
        // reading a name, or the hex digits become part of the name (`3Fst`).
        if let Some(len) = encoded_query_delim_len(line, index) {
            output.extend_from_slice(&line[index..index + len]);
            index += len;
            continue;
        }
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
        if let Some(len) = bare_token_len(line, index) {
            output.extend_from_slice(REDACTED);
            index += len;
            continue;
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
    while value_end < input.len() && !is_value_ender_at(input, value_end) {
        value_end += 1;
    }
    value_end
}

/// Token length when the value is exactly one login token.
fn exact_token_len(input: &[u8], value_start: usize) -> Option<usize> {
    let len = crate::auth::login_token_len_at(input, value_start)?;
    let after = value_start + len;
    if after == input.len() || is_value_ender_at(input, after) {
        Some(len)
    } else {
        None
    }
}

fn line_has_sensitive_marker(line: &[u8]) -> bool {
    let mut index = 0;
    while index < line.len() {
        if let Some(len) = encoded_query_delim_len(line, index) {
            index += len;
            continue;
        }
        if let Some(found) = assignment_at(line, index) {
            if is_sensitive_param(&line[index..found.name_end])
                || exact_token_len(line, found.value_start).is_some()
            {
                return true;
            }
            index = found.value_start;
            continue;
        }
        if bare_token_len(line, index).is_some() {
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

/// `=`, percent-encoded `=` (`%3D` / `%3d`), or that encoding doubled (`%253D` / `%253d`).
fn separator_len(input: &[u8], index: usize) -> Option<usize> {
    if input.get(index) == Some(&b'=') {
        return Some(1);
    }
    if percent_hex_at(input, index, b"253d") {
        return Some(5);
    }
    if percent_hex_at(input, index, b"3d") {
        return Some(3);
    }
    None
}

/// `true` when `input[index..]` is `%` plus the hex digits in `hex`, ignoring case.
fn percent_hex_at(input: &[u8], index: usize, hex: &[u8]) -> bool {
    let Some(encoded) = input.get(index..index + 1 + hex.len()) else {
        return false;
    };
    encoded[0] == b'%'
        && encoded[1..]
            .iter()
            .zip(hex)
            .all(|(byte, expected)| byte.eq_ignore_ascii_case(expected))
}

/// Length of `%3F` / `%3f` / `%26` / `%253F` / `%2526` at `index`, longest first.
fn encoded_query_delim_len(input: &[u8], index: usize) -> Option<usize> {
    if percent_hex_at(input, index, b"253f") || percent_hex_at(input, index, b"2526") {
        return Some(5);
    }
    if percent_hex_at(input, index, b"3f") || percent_hex_at(input, index, b"26") {
        return Some(3);
    }
    None
}

/// Login token glued to `%2F` / `%2f` (`/`) or `%3A` / `%3a` (`:`). Those hex
/// digits are name bytes, so the token would not otherwise be a bare token.
fn bare_token_len(line: &[u8], index: usize) -> Option<usize> {
    if !is_name_start(line, index) && !encoded_slash_or_colon_ends_at(line, index) {
        return None;
    }
    crate::auth::login_token_len_at(line, index)
}

fn encoded_slash_or_colon_ends_at(bytes: &[u8], index: usize) -> bool {
    if index < 3 {
        return false;
    }
    percent_hex_at(bytes, index - 3, b"2f") || percent_hex_at(bytes, index - 3, b"3a")
}

fn is_name_start(bytes: &[u8], index: usize) -> bool {
    if index == 0 || !is_name_byte(bytes[index - 1]) {
        return true;
    }
    // `%3F` / `%3f` is `?` and `%26` is `&`. The hex digits are name bytes, so
    // the parameter that follows them would otherwise stay glued to the encoding.
    // `%253F` and `%2526` are those delimiters encoded a second time.
    encoded_delimiter_ends_at(bytes, index)
}

fn encoded_delimiter_ends_at(bytes: &[u8], index: usize) -> bool {
    (index >= 5
        && (percent_hex_at(bytes, index - 5, b"253f") || percent_hex_at(bytes, index - 5, b"2526")))
        || (index >= 3
            && (percent_hex_at(bytes, index - 3, b"3f") || percent_hex_at(bytes, index - 3, b"26")))
}

fn is_value_ender_at(input: &[u8], index: usize) -> bool {
    if is_value_ender(input[index]) {
        return true;
    }
    // Encoded `&`, then the same sequence with the percent sign encoded again.
    percent_hex_at(input, index, b"2526") || percent_hex_at(input, index, b"26")
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
    fn redacts_nested_and_double_encoded_parameters() {
        // Values are not login-token shaped, so a bare-token match cannot pass these.
        let cases = [
            ("%3Fst%3dshortsecret", "%3Fst%3dREDACTED"),
            ("%3fst%3Dshortsecret", "%3fst%3DREDACTED"),
            ("x%26ST%3Dshortsecret", "x%26ST%3DREDACTED"),
            ("x%26st%3dshortsecret&tail", "x%26st%3dREDACTED&tail"),
            ("%2526ST%253Dshortsecret", "%2526ST%253DREDACTED"),
            (
                "%2526access_token%253dshortsecret%26ok",
                "%2526access_token%253dREDACTED%26ok",
            ),
            (
                "note=keep%3FST%3dshortsecret%26tail",
                "note=keep%3FST%3dREDACTED%26tail",
            ),
            ("foo%253Dshortsecret%26", "foo%253Dshortsecret%26"),
        ];
        for (input, expected) in cases {
            let once = redact_login_tokens(input.as_bytes());
            let text = String::from_utf8(once.clone()).unwrap();
            assert_eq!(text, expected, "{input}");
            assert_eq!(redact_login_tokens(&once), once, "{text}");
        }
    }

    #[test]
    fn redacts_bare_token_after_encoded_slash_or_colon() {
        let cases = [
            (format!("pre%2F{TOKEN}"), "pre%2FREDACTED"),
            (format!("pre%2f{TOKEN} tail"), "pre%2fREDACTED tail"),
            (format!("pre%3A{TOKEN}"), "pre%3AREDACTED"),
            (format!("pre%3a{TOKEN},next"), "pre%3aREDACTED,next"),
        ];
        for (input, expected) in cases {
            let once = redact_login_tokens(input.as_bytes());
            let text = String::from_utf8(once.clone()).unwrap();
            assert_eq!(text, expected, "{input}");
            assert!(!text.contains(TOKEN));
            assert_eq!(redact_login_tokens(&once), once, "{text}");
        }
        assert_eq!(
            redact_login_tokens(b"pre%2Fnot-a-token"),
            b"pre%2Fnot-a-token"
        );
        assert_eq!(
            redact_login_tokens(b"pre%3Anot-a-token"),
            b"pre%3Anot-a-token"
        );
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

        let mut encoded = vec![b'y'; MAX_SCRUB_LINE + 8];
        encoded.extend_from_slice(b"%3Fst%3dshortsecret%26");
        encoded.push(b'\n');
        let text = String::from_utf8(redact_login_tokens(&encoded)).unwrap();
        assert_eq!(text, "REDACTED\n");
        assert!(!text.contains("shortsecret"));
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
    fn encoded_delimiters_stay_linear() {
        // No sensitive parameter name: a line over the cap would otherwise be
        // replaced wholesale, which does not measure the scanner.
        let piece = b"%3f%26%253f%2526%3d%253dxx";
        let mut input = Vec::with_capacity(1024 * 1024);
        while input.len() + piece.len() <= 1024 * 1024 {
            input.extend_from_slice(piece);
        }
        let started = std::time::Instant::now();
        let output = redact_login_tokens(&input);
        let elapsed = started.elapsed();
        assert_eq!(output, input);
        assert!(
            elapsed < std::time::Duration::from_secs(1),
            "encoded redaction took {elapsed:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn civil_dates_round_trip_unix_days() {
        assert_eq!(unix_day_from_date(1970, 1, 1), Some(0));
        assert_eq!(date_from_unix_day(0), (1970, 1, 1));
        for (year, month, day) in [(2024, 2, 29), (2026, 11, 16), (2000, 2, 29), (1999, 12, 31)] {
            let unix_day = unix_day_from_date(year, month, day).unwrap();
            assert_eq!(date_from_unix_day(unix_day), (year, month, day));
        }
        assert!(unix_day_from_date(1900, 2, 29).is_none());
        assert!(unix_day_from_date(2021, 2, 29).is_none());
        assert!(unix_day_from_date(2024, 13, 1).is_none());
        let today = unix_day_number();
        assert!(!filename_date_outside_retention(today, today));
        assert!(!filename_date_outside_retention(
            today - i64::try_from(MAX_LOG_FILES).unwrap(),
            today
        ));
        assert!(filename_date_outside_retention(
            today - i64::try_from(MAX_LOG_FILES).unwrap() - 1,
            today
        ));
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
        let recent = log_dir.join(dated_log_name(unix_day_number() - 1));
        fs::write(&recent, b"recent\n").unwrap();
        fs::File::options()
            .write(true)
            .open(&recent)
            .unwrap()
            .set_modified(UNIX_EPOCH)
            .unwrap();
        let (writer, guard) = open_log_writer(&log_dir).expect("log writer");
        drop(writer);
        drop(guard);
        assert_eq!(fs::read(&recent).unwrap(), b"recent\n");
        // Newest 14 at prune time are yesterday plus 2020-01-08..20. Today is
        // created after that prune, so the January 1–7 files are the ones removed.
        for day in 1..=7 {
            assert!(
                !log_dir
                    .join(format!("hearthstone-linux-gui.log.2020-01-{day:02}"))
                    .exists(),
                "2020-01-{day:02} should be outside the newest {MAX_LOG_FILES}"
            );
        }
        for day in 8..=20 {
            assert!(
                log_dir
                    .join(format!("hearthstone-linux-gui.log.2020-01-{day:02}"))
                    .is_file(),
                "2020-01-{day:02} is among the newest {MAX_LOG_FILES}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn rewrites_rolling_logs_and_a_second_pass_does_not_touch_them() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let root = tempfile::tempdir().unwrap();
        let dated = root.path().join(dated_log_name(unix_day_number() - 1));
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
        let regular = root.path().join(dated_log_name(unix_day_number() - 1));
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
        let second = log_dir.join(dated_log_name(unix_day_number() - 1));
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
        let existing = log_dir.join(dated_log_name(unix_day_number() - 1));
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

    #[cfg(unix)]
    #[test]
    fn retention_uses_filename_date_and_keeps_recent_logs() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let root = tempfile::tempdir().unwrap();
        let today_day = unix_day_number();
        let recent = root.path().join(dated_log_name(
            today_day - i64::try_from(MAX_LOG_FILES).unwrap(),
        ));
        let expired = root.path().join(dated_log_name(
            today_day - i64::try_from(MAX_LOG_FILES).unwrap() - 1,
        ));
        let user = root.path().join("hearthstone-linux-gui.log.backup");
        let invalid = root.path().join("hearthstone-linux-gui.log.2021-02-29");
        let current = root.path().join(LOG_FILE_PREFIX);
        let line = format!("ST={TOKEN}\n");
        // Fourteen newer dated logs put `expired` past the keep set. `recent`
        // is outside that set too, but its filename date is still inside the window.
        for age in 0..i64::try_from(MAX_LOG_FILES).unwrap() {
            fs::write(
                root.path().join(dated_log_name(today_day - age)),
                b"plain\n",
            )
            .unwrap();
        }
        fs::write(&recent, &line).unwrap();
        fs::write(&expired, &line).unwrap();
        fs::write(&user, &line).unwrap();
        fs::write(&invalid, &line).unwrap();
        fs::write(&current, &line).unwrap();
        fs::set_permissions(&user, fs::Permissions::from_mode(0o644)).unwrap();
        fs::File::options()
            .write(true)
            .open(&recent)
            .unwrap()
            .set_modified(UNIX_EPOCH)
            .unwrap();
        let outside = root.path().join("outside.log");
        fs::write(&outside, &line).unwrap();
        let link = root.path().join(dated_log_name(
            today_day - i64::try_from(MAX_LOG_FILES).unwrap() - 2,
        ));
        symlink(&outside, &link).unwrap();

        let warnings = capture_warnings(|| scrub_legacy_logs(root.path()));
        assert!(!warnings.contains("WARN"), "{warnings}");
        assert!(recent.is_file(), "recent log was deleted");
        assert!(!fs::read(&recent)
            .unwrap()
            .windows(TOKEN.len())
            .any(|window| window == TOKEN.as_bytes()));
        assert!(!expired.exists(), "expired log survived the scrub");
        assert_eq!(fs::read(&user).unwrap(), line.as_bytes());
        assert_eq!(mode(&user), 0o644);
        assert_eq!(fs::read(&invalid).unwrap(), line.as_bytes());
        assert!(!fs::read(&current)
            .unwrap()
            .windows(TOKEN.len())
            .any(|window| window == TOKEN.as_bytes()));
        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read(&outside).unwrap(), line.as_bytes());

        let (writer, guard) = open_log_writer(root.path()).expect("log writer");
        drop(writer);
        drop(guard);
        assert!(
            recent.is_file(),
            "opening the log writer deleted a recent log"
        );
        assert!(!expired.exists());
        assert_eq!(fs::read(&user).unwrap(), line.as_bytes());
    }

    #[cfg(unix)]
    #[test]
    fn clock_jump_keeps_history_across_two_startups() {
        let root = tempfile::tempdir().unwrap();
        let log_dir = root.path().join("logs");
        fs::create_dir(&log_dir).unwrap();
        let today = unix_day_number();
        let historical: Vec<_> = [30_i64, 45, 60, 90]
            .into_iter()
            .map(|age| {
                let path = log_dir.join(dated_log_name(today - age));
                fs::write(&path, format!("history {age}\n")).unwrap();
                path
            })
            .collect();

        for pass in 0..2 {
            let notes = prepare_startup_logs(&log_dir, None, true);
            assert!(
                notes.warnings.is_empty(),
                "pass {pass}: {:?}",
                notes.warnings
            );
            let (writer, guard) = open_log_writer(&log_dir).expect("log writer");
            drop(writer);
            drop(guard);
        }

        for path in &historical {
            assert!(path.is_file(), "lost {}", path.display());
            let body = fs::read(path).unwrap();
            assert!(body.starts_with(b"history "), "{}", path.display());
        }
        assert!(log_dir.join(dated_log_name(today)).is_file());
    }

    #[cfg(unix)]
    #[test]
    fn first_upgrade_redacts_the_last_fifteen_days_and_drops_older_logs() {
        let root = tempfile::tempdir().unwrap();
        let log_dir = root.path().join("logs");
        fs::create_dir(&log_dir).unwrap();
        let today = unix_day_number();
        let line = format!("uri=?ST={TOKEN}&region=US\n");
        for age in 0..=14 {
            fs::write(log_dir.join(dated_log_name(today - age)), &line).unwrap();
        }
        for age in [15, 16, 40] {
            fs::write(log_dir.join(dated_log_name(today - age)), &line).unwrap();
        }
        fs::write(log_dir.join("hearthstone-linux-gui.log.2020-01-03"), &line).unwrap();

        let notes = prepare_startup_logs(&log_dir, None, true);
        assert!(notes.warnings.is_empty(), "{:?}", notes.warnings);
        let (writer, guard) = open_log_writer(&log_dir).expect("log writer");
        drop(writer);
        drop(guard);

        for age in 0..=14 {
            let path = log_dir.join(dated_log_name(today - age));
            let body = fs::read(&path).unwrap_or_else(|_| panic!("missing {}", path.display()));
            assert!(
                !body
                    .windows(TOKEN.len())
                    .any(|window| window == TOKEN.as_bytes()),
                "{}",
                path.display()
            );
            assert!(
                body.windows(b"ST=REDACTED".len())
                    .any(|window| window == b"ST=REDACTED"),
                "{}",
                String::from_utf8_lossy(&body)
            );
        }
        for age in [15, 16, 40] {
            assert!(!log_dir.join(dated_log_name(today - age)).exists());
        }
        assert!(!log_dir
            .join("hearthstone-linux-gui.log.2020-01-03")
            .exists());
    }

    #[cfg(unix)]
    #[test]
    fn hardlinked_log_is_left_unchanged_and_symlink_is_not_followed() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let line = format!("ST={TOKEN}\n");
        use std::os::unix::fs::PermissionsExt;
        let log = root.path().join(dated_log_name(unix_day_number()));
        fs::write(&log, &line).unwrap();
        fs::set_permissions(&log, fs::Permissions::from_mode(0o644)).unwrap();
        let other = root.path().join("shared.log");
        fs::hard_link(&log, &other).unwrap();

        let warnings = capture_warnings(|| scrub_legacy_logs(root.path()));
        assert!(
            warnings.contains("hard link"),
            "expected a hard-link warning, got {warnings}"
        );
        assert_eq!(fs::read(&log).unwrap(), line.as_bytes());
        assert_eq!(fs::read(&other).unwrap(), line.as_bytes());
        assert_eq!(
            fs::metadata(&log).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(root.path().join(SCRUB_MARKER).is_file());

        let again = capture_warnings(|| scrub_legacy_logs(root.path()));
        assert!(!again.contains("WARN"), "{again}");
        assert_eq!(fs::read(&log).unwrap(), line.as_bytes());

        let outside = root.path().join("secret.log");
        fs::write(&outside, &line).unwrap();
        let link = root.path().join(dated_log_name(unix_day_number() - 1));
        symlink(&outside, &link).unwrap();
        assert!(redact_log_file(&link).is_ok());
        assert_eq!(fs::read(&outside).unwrap(), line.as_bytes());
        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn precreate_private_log_is_owner_only_and_does_not_truncate() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let day = unix_day_number();
        let existing = root.path().join(dated_log_name(day));
        fs::write(&existing, b"keep").unwrap();
        fs::set_permissions(&existing, fs::Permissions::from_mode(0o644)).unwrap();
        precreate_private_log(root.path(), day);
        assert_eq!(fs::read(&existing).unwrap(), b"keep");
        assert_eq!(
            fs::metadata(&existing).unwrap().permissions().mode() & 0o777,
            0o644
        );

        let created = root.path().join(dated_log_name(day - 1));
        precreate_private_log(root.path(), day - 1);
        assert_eq!(fs::read(&created).unwrap(), b"");
        assert_eq!(
            fs::metadata(&created).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
