use super::installfile::InstallEntry;
use anyhow::Result;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug)]
pub(super) struct InstallItem {
    pub(super) entry: InstallEntry,
    pub(super) encoding_key: String,
    pub(super) target_path: String,
    pub(super) has_archive: bool,
}

pub(super) fn installed_target_path(entry_path: &str) -> Option<String> {
    const DATA_PREFIX: &str = "Hearthstone.app/Contents/Resources/Data/";
    const RESOURCES_PREFIX: &str = "Hearthstone.app/Contents/Resources/";

    if let Some(relative) = entry_path.strip_prefix(DATA_PREFIX) {
        return Some(format!("Bin/Hearthstone_Data/{relative}"));
    }

    match entry_path.strip_prefix(RESOURCES_PREFIX) {
        Some("unity default resources") => {
            Some("Bin/Hearthstone_Data/Resources/unity default resources".to_string())
        }
        Some("PlayerIcon.icns") => {
            Some("Bin/Hearthstone_Data/Resources/PlayerIcon.icns".to_string())
        }
        Some(_) => None,
        None if entry_path.starts_with("Hearthstone.app/")
            || entry_path.starts_with("Hearthstone Beta Launcher.app/") =>
        {
            None
        }
        None => Some(entry_path.to_string()),
    }
}

pub(super) fn checked_install_path(out_dir: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    anyhow::ensure!(
        !path.is_absolute(),
        "install path must be relative: {relative}"
    );
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                anyhow::bail!("unsafe install path: {relative}");
            }
        }
    }
    Ok(out_dir.join(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_macos_resource_entries_to_linux_layout() {
        assert_eq!(
            installed_target_path("Hearthstone.app/Contents/Resources/Data/level0"),
            Some("Bin/Hearthstone_Data/level0".to_string())
        );
        assert_eq!(
            installed_target_path("Hearthstone.app/Contents/Resources/unity default resources"),
            Some("Bin/Hearthstone_Data/Resources/unity default resources".to_string())
        );
        assert_eq!(
            installed_target_path("Hearthstone.app/Contents/Resources/PlayerIcon.icns"),
            Some("Bin/Hearthstone_Data/Resources/PlayerIcon.icns".to_string())
        );
        assert_eq!(
            installed_target_path("Strings/enUS.txt"),
            Some("Strings/enUS.txt".to_string())
        );
        assert_eq!(
            installed_target_path("Hearthstone.app/Contents/MacOS/Hearthstone"),
            None
        );
    }
}
