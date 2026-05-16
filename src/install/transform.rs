use anyhow::{Context, Result};
use std::{io::ErrorKind, path::Path};
use tracing::{debug, info};

pub fn transform_macos_installation(game_dir: &Path) -> Result<()> {
    let app = game_dir.join("Hearthstone.app");
    if !app.exists() {
        debug!(game_dir = %game_dir.display(), "macOS layout already transformed");
        return Ok(());
    }

    info!(game_dir = %game_dir.display(), "transforming macOS layout");
    let data_dir = game_dir.join("Bin/Hearthstone_Data");
    std::fs::create_dir_all(game_dir.join("Bin"))?;
    rename_or_copy(&app.join("Contents/Resources/Data"), &data_dir)?;
    rename_or_copy(
        &app.join("Contents/Resources/unity default resources"),
        &data_dir.join("Resources/unity default resources"),
    )?;
    rename_or_copy(
        &app.join("Contents/Resources/PlayerIcon.icns"),
        &data_dir.join("Resources/PlayerIcon.icns"),
    )?;

    let _ = std::fs::remove_dir_all(app);
    let _ = std::fs::remove_dir_all(game_dir.join("Hearthstone Beta Launcher.app"));
    Ok(())
}

fn rename_or_copy(from: &Path, to: &Path) -> Result<()> {
    debug!(from = %from.display(), to = %to.display(), "moving install path");
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) if from.is_dir() => {
            copy_dir(from, to)?;
            std::fs::remove_dir_all(from)?;
            Ok(())
        }
        Err(error) if error.kind() == ErrorKind::NotFound && to.exists() => {
            debug!(from = %from.display(), to = %to.display(), "install path already moved");
            Ok(())
        }
        Err(_) => {
            std::fs::copy(from, to).with_context(|| {
                format!("failed to copy {} to {}", from.display(), to.display())
            })?;
            let _ = std::fs::remove_file(from);
            Ok(())
        }
    }
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(from) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(from)?;
        let target = to.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::transform_macos_installation;

    #[test]
    fn moves_unity_default_resources_as_file_inside_resources_dir() {
        let temp = tempfile::tempdir().unwrap();
        let game_dir = temp.path();
        let resources = game_dir.join("Hearthstone.app/Contents/Resources");
        std::fs::create_dir_all(resources.join("Data/Resources")).unwrap();
        std::fs::write(resources.join("Data/level0"), b"level").unwrap();
        std::fs::write(resources.join("unity default resources"), b"unity").unwrap();
        std::fs::write(resources.join("PlayerIcon.icns"), b"icon").unwrap();

        transform_macos_installation(game_dir).unwrap();

        assert_eq!(
            std::fs::read(game_dir.join("Bin/Hearthstone_Data/Resources/unity default resources"))
                .unwrap(),
            b"unity"
        );
        assert_eq!(
            std::fs::read(game_dir.join("Bin/Hearthstone_Data/Resources/PlayerIcon.icns")).unwrap(),
            b"icon"
        );
        assert!(game_dir.join("Bin/Hearthstone_Data/level0").exists());
        assert!(!game_dir.join("Hearthstone.app").exists());
    }

    #[test]
    fn accepts_path_already_moved_after_partial_transform() {
        let temp = tempfile::tempdir().unwrap();
        let game_dir = temp.path();
        let resources = game_dir.join("Hearthstone.app/Contents/Resources");
        std::fs::create_dir_all(&resources).unwrap();
        std::fs::create_dir_all(game_dir.join("Bin/Hearthstone_Data")).unwrap();
        std::fs::write(game_dir.join("Bin/Hearthstone_Data/level0"), b"level").unwrap();
        std::fs::write(resources.join("unity default resources"), b"unity").unwrap();
        std::fs::write(resources.join("PlayerIcon.icns"), b"icon").unwrap();

        transform_macos_installation(game_dir).unwrap();

        assert!(game_dir.join("Bin/Hearthstone_Data/level0").exists());
        assert!(game_dir
            .join("Bin/Hearthstone_Data/Resources/unity default resources")
            .exists());
        assert!(!game_dir.join("Hearthstone.app").exists());
    }
}
