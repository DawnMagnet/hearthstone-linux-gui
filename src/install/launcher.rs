use anyhow::{Context, Result};
use std::{path::Path, process::Command};
use tracing::info;

pub fn launch_game(game_dir: &Path) -> Result<()> {
    let exe = game_dir.join("Bin/Hearthstone.x86_64");
    anyhow::ensure!(exe.exists(), "{} does not exist", exe.display());
    anyhow::ensure!(game_dir.join("token").exists(), "login token is missing");
    anyhow::ensure!(
        game_dir.join("client.config").exists(),
        "client.config is missing"
    );

    info!(exe = %exe.display(), game_dir = %game_dir.display(), "launching Hearthstone");
    Command::new(exe)
        .current_dir(game_dir)
        .spawn()
        .context("failed to launch Hearthstone")?;
    Ok(())
}
