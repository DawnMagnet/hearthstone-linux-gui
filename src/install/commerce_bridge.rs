//! Experimental, opt-in shop and gold support.
//!
//! Blizzard ships the commerce SDK for Windows and macOS only, so the game
//! normally gets stubs and shows a closed shop. With the bridge on, the stubs
//! are replaced by `crates/commerce-bridge`, which forwards the game's calls
//! to `commerce_helper.exe` under Wine. The helper hosts Blizzard's own
//! Windows commerce DLLs, downloaded from Blizzard's CDN for the current
//! build like the rest of the game. Purchases are refused for now.
//!
//! Nothing here may block the launch: on any problem the stubs stay.

use super::{compatibility, launcher};
use crate::{
    host_env,
    ngdp::{InstallOptions, NgdpClient},
    paths::AppPaths,
    util, AppConfig,
};
use anyhow::{Context, Result};
use std::{ffi::OsString, path::Path, process::Command};
use tracing::{info, warn};

/// `(path in the Windows build, local name)`. Wine's own Visual C++
/// runtime is used, so these are the only Blizzard files needed.
const WINDOWS_FILES: &[(&str, &str)] = &[
    (
        "Hearthstone_Data/Plugins/x86_64/blz_commerce_sdk_plugin.dll",
        "blz_commerce_sdk_plugin.dll",
    ),
    (
        "Hearthstone_Data/Plugins/x86_64/commerce_http_client.dll",
        "commerce_http_client.dll",
    ),
];

/// Installs the bridge or the stubs and returns the environment the game
/// needs for the bridge (empty when it is off or could not be prepared).
pub fn select(game_dir: &Path, enabled: bool) -> Vec<(&'static str, OsString)> {
    if enabled {
        match prepare(game_dir) {
            Ok(env) => return env,
            Err(error) => warn!(
                error = %format!("{error:#}"),
                "commerce bridge unavailable; the shop stays closed"
            ),
        }
    }
    if let Err(error) = compatibility::select_commerce_libraries(game_dir, None) {
        warn!(error = %format!("{error:#}"), "could not restore the commerce stubs");
    }
    Vec::new()
}

fn prepare(game_dir: &Path) -> Result<Vec<(&'static str, OsString)>> {
    let wine = launcher::find_in_path("wine").context("Wine is not installed")?;
    let library = compatibility::stub_resource("libcommerce_bridge.so")
        .context("libcommerce_bridge.so is missing")?;
    let helper = compatibility::stub_resource("commerce_helper.exe")
        .context("commerce_helper.exe is missing")?;
    let paths = AppPaths::discover()?;
    let config = AppConfig::load_or_default(&paths.config_file)?;
    let dir = paths.data_dir.join("commerce-bridge");
    util::copy_file_replace(&helper, &dir.join("commerce_helper.exe"))?;

    if let Err(error) = fetch_windows_files(&paths, &config, &dir) {
        let present = WINDOWS_FILES
            .iter()
            .all(|(_, name)| dir.join(name).exists());
        anyhow::ensure!(
            present,
            error.context("could not download the commerce DLLs")
        );
        warn!(error = %format!("{error:#}"), "commerce DLLs not updated; using the existing ones");
    }

    let prefix = dir.join("wine-prefix");
    if !prefix.join("system.reg").exists() {
        info!(prefix = %prefix.display(), "creating the Wine prefix for the commerce bridge");
        let mut wineboot = Command::new(&wine);
        host_env::apply(&mut wineboot);
        let status = wineboot
            .args(["wineboot", "-i"])
            .env("WINEPREFIX", &prefix)
            .env("WINEDEBUG", "-all")
            .env_remove("LD_LIBRARY_PATH")
            .env_remove("LD_PRELOAD")
            .status()
            .context("failed to run wineboot")?;
        anyhow::ensure!(status.success(), "wineboot failed: {status}");
    }

    compatibility::select_commerce_libraries(game_dir, Some(&library))?;
    Ok(vec![
        ("HEARTHSTONE_COMMERCE_BRIDGE_DIR", dir.into()),
        ("HEARTHSTONE_COMMERCE_WINEPREFIX", prefix.into()),
        ("HEARTHSTONE_COMMERCE_WINE", wine.into()),
    ])
}

/// Brings the DLLs in line with the current build. Fast when they already are.
fn fetch_windows_files(paths: &AppPaths, config: &AppConfig, dir: &Path) -> Result<()> {
    let options = InstallOptions {
        region: config.region,
        locale: config.locale,
        verify: true,
    };
    let client = NgdpClient::new().with_cache_dir(paths.ngdp_dir.clone());
    let dir = dir.to_path_buf();
    // Own thread and runtime: the caller may already be inside one.
    std::thread::spawn(move || {
        tokio::runtime::Runtime::new()?
            .block_on(client.install_windows_files(&options, WINDOWS_FILES, &dir))
            .map(|version| info!(version = %version.version_name, "commerce DLLs ready"))
    })
    .join()
    .map_err(|_| anyhow::anyhow!("commerce DLL download panicked"))?
}
