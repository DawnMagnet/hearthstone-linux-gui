use anyhow::{Context, Result};
use std::{fs, io::Write, path::Path};

/// Replace a credential file atomically, with owner-only permissions on Unix.
/// A destination symlink is replaced rather than followed. Existing directory
/// permissions are preserved, including user-selected game directories.
pub fn write_private_file(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut directories = fs::DirBuilder::new();
    directories.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        directories.mode(0o700);
    }
    directories
        .create(parent)
        .with_context(|| format!("failed to create {}", parent.display()))?;

    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    temporary.write_all(contents)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to replace {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn new_files_and_directories_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("new/game/token");
        write_private_file(&path, b"secret").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"secret");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }

    #[cfg(unix)]
    #[test]
    fn replacing_a_file_tightens_its_permissions_without_changing_its_directory() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("token");
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(&path, b"old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        write_private_file(&path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(root.path()).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }

    #[cfg(unix)]
    #[test]
    fn replacing_a_symlink_does_not_write_to_its_target() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let unrelated = root.path().join("unrelated");
        let token = root.path().join("token");
        fs::write(&unrelated, b"keep").unwrap();
        symlink(&unrelated, &token).unwrap();
        write_private_file(&token, b"secret").unwrap();
        assert_eq!(fs::read(unrelated).unwrap(), b"keep");
        assert_eq!(fs::read(&token).unwrap(), b"secret");
        assert!(!fs::symlink_metadata(token)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[test]
    fn failed_replacement_does_not_leave_a_temporary_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("token");
        fs::create_dir(&path).unwrap();
        assert!(write_private_file(&path, b"secret").is_err());
        assert!(path.is_dir());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }
}
