use anyhow::Result;
use std::path::Path;

pub fn transform_macos_installation(game_dir: &Path) -> Result<()> {
    let app = game_dir.join("Hearthstone.app");
    if !app.exists() {
        return Ok(());
    }

    let data_dir = game_dir.join("Bin/Hearthstone_Data");
    std::fs::create_dir_all(game_dir.join("Bin"))?;
    rename_or_copy(&app.join("Contents/Resources/Data"), &data_dir)?;
    rename_or_copy(
        &app.join("Contents/Resources/unity default resources"),
        &data_dir.join("Resources"),
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
        Err(_) => {
            std::fs::copy(from, to)?;
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
