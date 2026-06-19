use super::{compatibility, transform, unity};
use crate::{
    config::AppConfig,
    ngdp::{InstallOptions, NgdpClient},
    paths::AppPaths,
    util,
};
use anyhow::{Context, Result};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

#[derive(Clone, Debug)]
pub enum TaskEvent {
    Started(String),
    Progress {
        message: String,
        fraction: Option<f64>,
    },
    Finished(String),
    Failed(String),
    Cancelled(String),
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
        event(TaskEvent::Started("Preparing installation".into()));
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
            message: "Transforming macOS layout".into(),
            fraction: Some(0.96),
        });
        debug!("transforming macOS layout");
        transform::transform_macos_installation(&self.paths.game_dir)?;
        util::check_cancelled(cancel.as_ref(), "installation cancelled")?;

        event(TaskEvent::Progress {
            message: "Installing Unity Linux player".into(),
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
            message: "Installing compatibility files".into(),
            fraction: Some(0.99),
        });
        compatibility::install_compatibility_files(
            &self.paths.game_dir,
            config.region,
            config.locale,
        )?;

        config.game_dir = Some(self.paths.game_dir.clone());
        preserve_login_metadata(config, &self.paths);
        config.save(&self.paths.config_file)?;
        info!("install/update completed");
        event(TaskEvent::Finished("Ready to play".into()));
        Ok(())
    }
}

fn preserve_login_metadata(config: &mut AppConfig, paths: &AppPaths) {
    let saved = AppConfig::load_or_default(&paths.config_file).ok();
    let token_exists = paths.game_token().exists();
    if saved.as_ref().is_some_and(|saved| saved.logged_in) || token_exists {
        config.logged_in = true;
        config.last_login_at = saved.and_then(|saved| saved.last_login_at);
    }
}

fn format_unity_download_progress(progress: &unity::UnityDownloadProgress) -> String {
    let action = match progress.phase {
        unity::UnityProgressPhase::Downloading if progress.resumed => "Resuming Unity download",
        unity::UnityProgressPhase::Downloading => "Downloading Unity",
        unity::UnityProgressPhase::Extracting => "Extracting Unity",
    };
    let downloaded = util::format_bytes(progress.downloaded as f64);
    let speed = if progress.speed_bytes_per_second > 0.0 {
        format!(
            " at {}/s",
            util::format_bytes(progress.speed_bytes_per_second)
        )
    } else {
        String::new()
    };

    match progress.total {
        Some(total) if total > 0 => format!(
            "{action}: {downloaded}/{}{speed}",
            util::format_bytes(total as f64)
        ),
        _ => format!("{action}: {downloaded}{speed}"),
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
