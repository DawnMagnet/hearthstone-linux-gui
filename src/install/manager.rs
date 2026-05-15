use super::{compatibility, transform, unity};
use crate::{
    config::AppConfig,
    ngdp::{InstallOptions, NgdpClient},
    paths::AppPaths,
};
use anyhow::Result;

#[derive(Clone, Debug)]
pub enum TaskEvent {
    Started(String),
    Progress {
        message: String,
        fraction: Option<f64>,
    },
    Finished(String),
    Failed(String),
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
        self.paths.ensure()?;
        event(TaskEvent::Started("Preparing installation".into()));

        cleanup_installation(&self.paths.game_dir).await?;

        let client = NgdpClient::new().with_cache_dir(self.paths.ngdp_dir.clone());
        let options = InstallOptions {
            region: config.region,
            locale: config.locale,
            verify: true,
        };
        let version = client
            .install_latest(&options, &self.paths.game_dir, |progress| {
                event(TaskEvent::Progress {
                    message: progress.message,
                    fraction: progress.fraction,
                })
            })
            .await?;
        config.installed_version = Some(version.version_name.clone());

        event(TaskEvent::Progress {
            message: "Transforming macOS layout".into(),
            fraction: Some(0.96),
        });
        transform::transform_macos_installation(&self.paths.game_dir)?;

        event(TaskEvent::Progress {
            message: "Installing Unity Linux player".into(),
            fraction: Some(0.98),
        });
        let unity_version =
            unity::ensure_unity_player(&self.paths.game_dir, &self.paths.unity_cache_dir).await?;
        config.unity_version = Some(unity_version);

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
        config.save(&self.paths.config_file)?;
        event(TaskEvent::Finished("Ready to play".into()));
        Ok(())
    }
}

async fn cleanup_installation(game_dir: &std::path::Path) -> Result<()> {
    tokio::fs::create_dir_all(game_dir).await?;
    for name in [
        "Bin/Hearthstone_Data",
        "Data",
        "Strings",
        "Logs",
        "BlizzardBrowser",
        "Hearthstone.app",
        "Hearthstone Beta Launcher.app",
    ] {
        let path = game_dir.join(name);
        if path.exists() {
            let _ = tokio::fs::remove_dir_all(&path).await;
            let _ = tokio::fs::remove_file(&path).await;
        }
    }
    Ok(())
}
