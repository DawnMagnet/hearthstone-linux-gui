use anyhow::{Context, Result};
use directories::ProjectDirs;
use std::path::{Path, PathBuf};
use tracing::warn;

const APP_DIR: &str = "hearthstone-linux-gui";

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub state_dir: PathBuf,
    pub config_file: PathBuf,
    pub game_dir: PathBuf,
    pub ngdp_dir: PathBuf,
    pub unity_cache_dir: PathBuf,
    pub log_dir: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        let dirs = ProjectDirs::from(
            "io.github",
            "hearthstone-linux-gui",
            "hearthstone-linux-gui",
        )
        .context("could not resolve XDG project directories")?;
        let config_dir = writable_dir("config", dirs.config_dir(), fallback_config_dir())?;
        let data_dir = writable_dir("data", dirs.data_dir(), fallback_data_dir())?;
        let cache_dir = writable_dir("cache", dirs.cache_dir(), fallback_cache_dir())?;
        let state_dir = writable_dir(
            "state",
            dirs.state_dir().unwrap_or_else(|| dirs.data_dir()),
            fallback_state_dir().or_else(fallback_data_dir),
        )?;

        Ok(Self {
            config_file: config_dir.join("config.toml"),
            game_dir: data_dir.join("game"),
            ngdp_dir: cache_dir.join("ngdp"),
            unity_cache_dir: cache_dir.join("unity"),
            log_dir: state_dir.join("logs"),
            config_dir,
            data_dir,
            cache_dir,
            state_dir,
        })
    }

    pub fn ensure(&self) -> Result<()> {
        for dir in [
            &self.config_dir,
            &self.data_dir,
            &self.cache_dir,
            &self.state_dir,
            &self.game_dir,
            &self.ngdp_dir,
            &self.unity_cache_dir,
            &self.log_dir,
        ] {
            ensure_writable_dir(dir)?;
        }
        Ok(())
    }

    pub fn game_token(&self) -> PathBuf {
        self.game_dir.join("token")
    }
}

fn writable_dir(kind: &str, preferred: &Path, fallback: Option<PathBuf>) -> Result<PathBuf> {
    match ensure_writable_dir(preferred) {
        Ok(()) => Ok(preferred.to_path_buf()),
        Err(preferred_error) => {
            let Some(fallback) = fallback else {
                return Err(preferred_error).with_context(|| {
                    format!(
                        "{} directory is not writable: {}",
                        kind,
                        preferred.display()
                    )
                });
            };
            if fallback == preferred {
                return Err(preferred_error).with_context(|| {
                    format!(
                        "{} directory is not writable: {}",
                        kind,
                        preferred.display()
                    )
                });
            }

            ensure_writable_dir(&fallback).with_context(|| {
                format!(
                    "{} directory is not writable: {}; fallback is also not writable: {}",
                    kind,
                    preferred.display(),
                    fallback.display()
                )
            })?;
            warn!(
                kind,
                preferred = %preferred.display(),
                fallback = %fallback.display(),
                error = %preferred_error,
                "XDG directory is not writable; using fallback"
            );
            Ok(fallback)
        }
    }
}

fn ensure_writable_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)
        .with_context(|| format!("failed to create directory {}", path.display()))?;

    let probe = path.join(format!(".write-test-{}", std::process::id()));
    std::fs::write(&probe, b"")
        .with_context(|| format!("failed to write into directory {}", path.display()))?;
    std::fs::remove_file(&probe)
        .with_context(|| format!("failed to remove write probe {}", probe.display()))?;
    Ok(())
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn fallback_config_dir() -> Option<PathBuf> {
    home_dir().map(|home| home.join(".config").join(APP_DIR))
}

fn fallback_data_dir() -> Option<PathBuf> {
    home_dir().map(|home| home.join(".local/share").join(APP_DIR))
}

fn fallback_cache_dir() -> Option<PathBuf> {
    home_dir().map(|home| home.join(".cache").join(APP_DIR))
}

fn fallback_state_dir() -> Option<PathBuf> {
    home_dir().map(|home| home.join(".local/state").join(APP_DIR))
}
