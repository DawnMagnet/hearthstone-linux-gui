use anyhow::{Context, Result};
use std::{
    io::Write,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

pub fn check_cancelled(cancel: Option<&CancellationToken>, message: &'static str) -> Result<()> {
    if cancel.is_some_and(CancellationToken::is_cancelled) {
        anyhow::bail!(message);
    }
    Ok(())
}

/// Localized transfer speed to append to a progress message, e.g.
/// a line `3.2 MiB/s`; empty while the speed is unknown.
///
/// Returned as a [`Phrase`](crate::i18n::Phrase) so the unit follows a
/// language change together with the message it is embedded in.
pub fn format_speed_suffix(bytes_per_second: f64) -> crate::i18n::Phrase {
    if bytes_per_second > 0.0 {
        crate::phrase!("progress.speed", speed = format_bytes(bytes_per_second))
    } else {
        crate::i18n::Phrase::literal("")
    }
}

/// Joins a multi-line message into one line for terminals and logs: the
/// launcher window shows progress texts on several lines.
pub fn single_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn format_bytes(bytes: f64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes;
    let mut unit = UNITS[0];
    for candidate in UNITS.iter().skip(1) {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = candidate;
    }

    if unit == "B" {
        format!("{value:.0} {unit}")
    } else {
        format!("{value:.1} {unit}")
    }
}

/// Writes `data` to a temporary file in the same directory as `path`, syncs
/// it, then renames it into place. A crash leaves the previous `path` intact.
pub fn write_file_atomic(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = parent_dir(path) {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create {} for {}",
                parent.display(),
                path.display()
            )
        })?;
    }
    let temp = temporary_sibling(path);
    let mut created = false;
    let result = (|| -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .with_context(|| format!("failed to write {}", path.display()))?;
        created = true;
        file.write_all(data)
            .with_context(|| format!("failed to write {}", path.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync {}", path.display()))?;
        drop(file);
        std::fs::rename(&temp, path)
            .with_context(|| format!("failed to replace {}", path.display()))?;
        if let Some(parent) = parent_dir(path) {
            sync_dir(parent)?;
        }
        Ok(())
    })();
    if created && result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

pub async fn write_file_atomic_async(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = parent_dir(path) {
        tokio::fs::create_dir_all(parent).await.with_context(|| {
            format!(
                "failed to create {} for {}",
                parent.display(),
                path.display()
            )
        })?;
    }
    let temp = temporary_sibling(path);
    let mut file = match tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .await
    {
        Ok(file) => file,
        Err(error) => {
            return Err(error).with_context(|| format!("failed to write {}", path.display()));
        }
    };
    let result = async {
        file.write_all(data)
            .await
            .with_context(|| format!("failed to write {}", path.display()))?;
        file.sync_all()
            .await
            .with_context(|| format!("failed to sync {}", path.display()))?;
        drop(file);
        tokio::fs::rename(&temp, path)
            .await
            .with_context(|| format!("failed to replace {}", path.display()))?;
        if let Some(parent) = parent_dir(path) {
            sync_dir(parent)?;
        }
        Ok(())
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&temp).await;
    }
    result
}

/// Copies `from` onto `to` by writing a sibling temporary file and renaming it.
/// The previous `to` stays in place until the replacement is complete.
pub fn copy_file_atomic(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = parent_dir(to) {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("failed to create {} for {}", parent.display(), to.display())
        })?;
    }
    let temp = temporary_sibling(to);
    let mut created = false;
    let result = (|| -> Result<()> {
        let mut input = std::fs::File::open(from)
            .with_context(|| format!("failed to read {}", from.display()))?;
        let permissions = input
            .metadata()
            .with_context(|| format!("failed to inspect {}", from.display()))?
            .permissions();
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .with_context(|| format!("failed to write {}", to.display()))?;
        created = true;
        std::io::copy(&mut input, &mut output)
            .with_context(|| format!("failed to write {}", to.display()))?;
        std::fs::set_permissions(&temp, permissions)
            .with_context(|| format!("failed to set permissions on {}", to.display()))?;
        output
            .sync_all()
            .with_context(|| format!("failed to sync {}", to.display()))?;
        drop(output);
        std::fs::rename(&temp, to)
            .with_context(|| format!("failed to replace {}", to.display()))?;
        if let Some(parent) = parent_dir(to) {
            sync_dir(parent)?;
        }
        Ok(())
    })();
    if created && result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

pub fn copy_file_replace(from: &Path, to: &Path) -> Result<()> {
    copy_file_atomic(from, to)
}

pub fn copy_dir_all(from: &Path, to: &Path, replace: bool) -> Result<()> {
    if replace && to.exists() {
        std::fs::remove_dir_all(to)
            .with_context(|| format!("failed to remove {}", to.display()))?;
    }
    for entry in walkdir::WalkDir::new(from) {
        let entry = entry.with_context(|| format!("failed to read {}", from.display()))?;
        let relative = entry.path().strip_prefix(from)?;
        let target = to.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)
                .with_context(|| format!("failed to create {}", target.display()))?;
        } else {
            copy_file_atomic(entry.path(), &target)?;
        }
    }
    Ok(())
}

pub fn sync_dir(dir: &Path) -> Result<()> {
    let file =
        std::fs::File::open(dir).with_context(|| format!("failed to open {}", dir.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to sync {}", dir.display()))?;
    Ok(())
}

fn parent_dir(path: &Path) -> Option<&Path> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
}

pub(crate) fn temporary_sibling(target: &Path) -> std::path::PathBuf {
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let name = format!(".tmp-{}-{seq}", std::process::id());
    match parent_dir(target) {
        Some(parent) => parent.join(name),
        None => std::path::PathBuf::from(name),
    }
}

#[cfg(unix)]
pub fn make_user_writable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?
        .permissions();
    let mode = permissions.mode();
    if mode & 0o200 == 0 {
        permissions.set_mode(mode | 0o200);
        std::fs::set_permissions(path, permissions)
            .with_context(|| format!("failed to set permissions on {}", path.display()))?;
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn make_user_writable(path: &Path) -> Result<()> {
    let mut permissions = std::fs::metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?
        .permissions();
    if permissions.readonly() {
        permissions.set_readonly(false);
        std::fs::set_permissions(path, permissions)
            .with_context(|| format!("failed to set permissions on {}", path.display()))?;
    }
    Ok(())
}

#[cfg(unix)]
pub fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?
        .permissions();
    permissions.set_mode(permissions.mode() | 0o755);
    std::fs::set_permissions(path, permissions)
        .with_context(|| format!("failed to set permissions on {}", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
pub fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{single_line, write_file_atomic};

    #[test]
    fn single_line_joins_progress_lines() {
        assert_eq!(
            single_line("Downloading Hearthstone:\n3/10 files, 1 MiB/2 MiB\n512 KiB/s"),
            "Downloading Hearthstone: 3/10 files, 1 MiB/2 MiB 512 KiB/s"
        );
        assert_eq!(single_line("Checking\n\n"), "Checking");
        assert_eq!(single_line("one line"), "one line");
    }

    #[test]
    fn atomic_write_replaces_file_without_leaving_a_temp() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("file.bin");
        write_file_atomic(&target, b"one").unwrap();
        write_file_atomic(&target, b"two").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"two");
        let names: Vec<_> = std::fs::read_dir(temp.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("file.bin")]);
    }

    #[test]
    fn atomic_write_error_names_the_path_and_keeps_the_original() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("dir");
        std::fs::create_dir(&target).unwrap();
        let error = write_file_atomic(&target, b"x").unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains(&target.display().to_string()), "{message}");
        assert!(target.is_dir());
        assert!(std::fs::read_dir(&target).unwrap().next().is_none());
        let names: Vec<_> = std::fs::read_dir(temp.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("dir")]);
    }
}
