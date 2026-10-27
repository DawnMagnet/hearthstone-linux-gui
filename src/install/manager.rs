use super::{compatibility, transform, unity};
use crate::{
    config::AppConfig,
    i18n::Phrase,
    ngdp::{InstallOptions, NgdpClient},
    paths::AppPaths,
    phrase, util,
};
use anyhow::{Context, Result};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

#[derive(Clone, Debug)]
pub enum TaskEvent {
    Started(Phrase),
    Progress {
        message: Phrase,
        fraction: Option<f64>,
    },
    Finished(Phrase),
    /// The raw error. The window wraps it in `install.failed` when showing it.
    Failed(String),
    Cancelled(Phrase),
}

pub struct InstallManager {
    paths: AppPaths,
}

impl InstallManager {
    pub fn new(paths: AppPaths) -> Self {
        Self { paths }
    }

    pub async fn install_or_update(
        &self,
        config: &mut AppConfig,
        mut event: impl FnMut(TaskEvent) + Send,
    ) -> Result<()> {
        self.install_or_update_with_cancel(config, &mut event, None)
            .await
    }

    pub async fn install_or_update_cancellable(
        &self,
        config: &mut AppConfig,
        mut event: impl FnMut(TaskEvent) + Send,
        cancel: CancellationToken,
    ) -> Result<()> {
        self.install_or_update_with_cancel(config, &mut event, Some(cancel))
            .await
    }

    pub fn uninstall(&self, config: &mut AppConfig) -> Result<()> {
        info!(
            game_dir = %self.paths.game_dir.display(),
            "uninstalling managed game files"
        );
        if self.paths.game_dir.exists() {
            std::fs::remove_dir_all(&self.paths.game_dir)
                .with_context(|| format!("failed to remove {}", self.paths.game_dir.display()))?;
        }

        config.game_dir = Some(self.paths.game_dir.clone());
        config.installed_version = None;
        config.unity_version = None;
        config.logged_in = false;
        config.last_login_at = None;
        config.save(&self.paths.config_file)
    }

    async fn install_or_update_with_cancel(
        &self,
        config: &mut AppConfig,
        event: &mut (impl FnMut(TaskEvent) + Send),
        cancel: Option<CancellationToken>,
    ) -> Result<()> {
        self.paths.ensure()?;
        info!(
            region = %config.region,
            locale = %config.locale,
            game_dir = %self.paths.game_dir.display(),
            cache_dir = %self.paths.cache_dir.display(),
            "starting install/update"
        );
        event(TaskEvent::Started(phrase!(
            "install.preparing_installation"
        )));
        util::check_cancelled(cancel.as_ref(), "installation cancelled")?;

        cleanup_installation(&self.paths.game_dir).await?;
        debug!("old install payload cleaned");
        util::check_cancelled(cancel.as_ref(), "installation cancelled")?;

        let client = NgdpClient::new().with_cache_dir(self.paths.ngdp_dir.clone());
        let options = InstallOptions {
            region: config.region,
            locale: config.locale,
            verify: true,
        };
        let version = client
            .install_latest_cancellable(
                &options,
                &self.paths.game_dir,
                |progress| {
                    event(TaskEvent::Progress {
                        message: progress.message,
                        fraction: progress.fraction,
                    })
                },
                cancel.clone(),
            )
            .await?;
        config.installed_version = Some(version.version_name.clone());
        info!(
            version = %version.version_name,
            build_id = %version.build_id,
            "NGDP install finished"
        );
        util::check_cancelled(cancel.as_ref(), "installation cancelled")?;

        event(TaskEvent::Progress {
            message: phrase!("install.transforming"),
            fraction: Some(0.96),
        });
        debug!("transforming macOS layout");
        transform::transform_macos_installation(&self.paths.game_dir)?;
        util::check_cancelled(cancel.as_ref(), "installation cancelled")?;

        event(TaskEvent::Progress {
            message: phrase!("install.unity_player"),
            fraction: Some(0.98),
        });
        debug!("installing Unity player");
        let unity_version = unity::ensure_unity_player_with_progress(
            &self.paths.game_dir,
            &self.paths.unity_cache_dir,
            cancel.clone(),
            |download| {
                event(TaskEvent::Progress {
                    message: format_unity_download_progress(&download),
                    fraction: download.fraction().map(|value| 0.97 + value * 0.02),
                })
            },
        )
        .await?;
        config.unity_version = Some(unity_version);
        util::check_cancelled(cancel.as_ref(), "installation cancelled")?;

        event(TaskEvent::Progress {
            message: phrase!("install.compatibility"),
            fraction: Some(0.99),
        });
        let overrides = match std::fs::read_to_string(self.paths.client_config_overrides()) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to read {}",
                        self.paths.client_config_overrides().display()
                    )
                })
            }
        };
        compatibility::install_compatibility_files(
            &self.paths.game_dir,
            config.region,
            config.locale,
            &overrides,
        )?;

        config.game_dir = Some(self.paths.game_dir.clone());
        *config = merge_into_saved_settings(config, &self.paths);
        config.save(&self.paths.config_file)?;
        info!("install/update completed");
        event(TaskEvent::Finished(phrase!("install.ready")));
        Ok(())
    }
}

/// Applies what the install changed onto the settings currently on disk.
///
/// The install works on a copy taken when it started, but the other settings
/// stay editable in the window meanwhile, so the file's values win for
/// everything the install does not own. A setting added later is kept without
/// having to be listed here.
fn merge_into_saved_settings(install: &AppConfig, paths: &AppPaths) -> AppConfig {
    let mut merged =
        AppConfig::load_or_default(&paths.config_file).unwrap_or_else(|_| install.clone());
    merged.region = install.region;
    merged.locale = install.locale;
    merged.game_dir = install.game_dir.clone();
    merged.installed_version = install.installed_version.clone();
    merged.unity_version = install.unity_version.clone();
    if merged.logged_in || paths.game_token().exists() {
        merged.logged_in = true;
    } else {
        merged.logged_in = install.logged_in;
        merged.last_login_at = install.last_login_at.clone();
    }
    merged
}

fn format_unity_download_progress(progress: &unity::UnityDownloadProgress) -> Phrase {
    let action = match progress.phase {
        unity::UnityProgressPhase::Downloading if progress.resumed => {
            phrase!("progress.unity_resuming")
        }
        unity::UnityProgressPhase::Downloading => phrase!("progress.unity_downloading"),
        unity::UnityProgressPhase::Extracting => phrase!("progress.unity_extracting"),
    };
    let done = util::format_bytes(progress.downloaded as f64);
    let speed = util::format_speed_suffix(progress.speed_bytes_per_second);

    match progress.total {
        Some(total) if total > 0 => phrase!(
            "progress.with_total",
            action = action,
            done = done,
            total = util::format_bytes(total as f64),
            speed = speed
        ),
        _ => phrase!(
            "progress.without_total",
            action = action,
            done = done,
            speed = speed
        ),
    }
}

async fn cleanup_installation(game_dir: &std::path::Path) -> Result<()> {
    tokio::fs::create_dir_all(game_dir).await?;
    for name in ["Hearthstone.app", "Hearthstone Beta Launcher.app"] {
        let path = game_dir.join(name);
        if path.exists() {
            debug!(path = %path.display(), "removing legacy install path");
            let _ = tokio::fs::remove_dir_all(&path).await;
            let _ = tokio::fs::remove_file(&path).await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths_in(dir: &std::path::Path) -> AppPaths {
        AppPaths {
            config_dir: dir.join("config"),
            data_dir: dir.join("data"),
            cache_dir: dir.join("cache"),
            state_dir: dir.join("state"),
            config_file: dir.join("config.toml"),
            game_dir: dir.join("game"),
            ngdp_dir: dir.join("ngdp"),
            unity_cache_dir: dir.join("unity"),
            log_dir: dir.join("logs"),
        }
    }

    #[test]
    fn install_result_is_merged_into_settings_changed_meanwhile() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(dir.path());

        // The install started from this copy ...
        let install = AppConfig {
            installed_version: Some("1.2.3".into()),
            game_dir: Some(paths.game_dir.clone()),
            ..AppConfig::default()
        };

        // ... while the user changed settings in the window.
        AppConfig {
            use_discrete_gpu: true,
            use_gamemode: true,
            ui_language: Some("de".into()),
            logged_in: true,
            last_login_at: Some("yesterday".into()),
            ..AppConfig::default()
        }
        .save(&paths.config_file)
        .unwrap();

        let merged = merge_into_saved_settings(&install, &paths);
        assert!(merged.use_discrete_gpu && merged.use_gamemode);
        assert_eq!(merged.ui_language.as_deref(), Some("de"));
        assert_eq!(merged.installed_version.as_deref(), Some("1.2.3"));
        assert_eq!(merged.game_dir, install.game_dir);
        assert!(merged.logged_in);
        assert_eq!(merged.last_login_at.as_deref(), Some("yesterday"));
    }

    #[test]
    fn install_result_stands_without_a_saved_config() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(dir.path());
        let install = AppConfig {
            installed_version: Some("1.2.3".into()),
            ..AppConfig::default()
        };

        let merged = merge_into_saved_settings(&install, &paths);
        assert_eq!(merged.installed_version.as_deref(), Some("1.2.3"));
        assert!(!merged.logged_in);
    }
}
