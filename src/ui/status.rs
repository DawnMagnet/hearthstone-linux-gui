use hearthstone_linux::{config::AppConfig, paths::AppPaths, tr};

#[derive(Clone, Debug)]
pub struct StatusSnapshot {
    pub headline: String,
    pub details: String,
}

pub fn reconcile(paths: &AppPaths) -> (AppConfig, StatusSnapshot) {
    let token_exists = paths.game_token().exists();
    let mut config = AppConfig::load_or_default(&paths.config_file).unwrap_or_default();
    let should_save = config.logged_in != token_exists || config.game_dir.is_none();
    config.logged_in = token_exists;
    if config.game_dir.is_none() {
        config.game_dir = Some(paths.game_dir.clone());
    }
    if should_save {
        let _ = config.save(&paths.config_file);
    }

    let installed = paths.game_dir.join("Bin/Hearthstone.x86_64").exists();
    let headline = match (installed, token_exists) {
        (true, true) => tr!("status.ready"),
        (true, false) => tr!("status.login_required"),
        (false, _) => tr!("status.not_installed"),
    };

    let snapshot = snapshot(headline, &config, token_exists);
    (config, snapshot)
}

pub fn snapshot(
    headline: impl Into<String>,
    config: &AppConfig,
    token_exists: bool,
) -> StatusSnapshot {
    let login = if config.logged_in && token_exists {
        tr!("status.logged_in")
    } else if token_exists {
        tr!("status.token_present")
    } else {
        tr!("status.logged_out")
    };
    let version = |version: &Option<String>| {
        version
            .clone()
            .filter(|version| !version.is_empty())
            .unwrap_or_else(|| tr!("status.version_missing"))
    };

    StatusSnapshot {
        headline: headline.into(),
        details: tr!(
            "status.details",
            login = login,
            game = version(&config.installed_version),
            unity = version(&config.unity_version),
            region = config.region,
            locale = config.locale
        ),
    }
}
