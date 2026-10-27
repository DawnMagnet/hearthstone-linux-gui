use hearthstone_linux::{config::AppConfig, i18n::Phrase, paths::AppPaths, phrase, tr};

#[derive(Clone, Debug, PartialEq)]
pub struct StatusSnapshot {
    /// Rendered when drawn, so a language change keeps this value and only
    /// the visible text moves.
    pub headline: Phrase,
    pub details: String,
}

pub fn reconcile(paths: &AppPaths) -> (AppConfig, StatusSnapshot) {
    let token_exists = paths.game_token().exists();
    let mut config = if paths.config_file.exists() {
        AppConfig::load_or_default(&paths.config_file).unwrap_or_default()
    } else {
        AppConfig::for_system_locale()
    };
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
        (true, true) => phrase!("status.ready"),
        (true, false) => phrase!("status.login_required"),
        (false, _) => phrase!("status.not_installed"),
    };

    let snapshot = snapshot(headline, &config, token_exists);
    (config, snapshot)
}

pub fn snapshot(headline: Phrase, config: &AppConfig, token_exists: bool) -> StatusSnapshot {
    StatusSnapshot {
        headline,
        details: details(config, token_exists),
    }
}

/// The status line under the logo, in the language selected right now.
pub fn details(config: &AppConfig, token_exists: bool) -> String {
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

    tr!(
        "status.details",
        login = login,
        game = version(&config.installed_version),
        unity = version(&config.unity_version),
        region = config.region,
        locale = config.locale
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use hearthstone_linux::i18n::LanguageHold;

    #[test]
    fn language_switch_keeps_the_snapshot_and_rerenders() {
        let hold = LanguageHold::acquire();
        hold.select(Some("en"));
        let config = AppConfig {
            logged_in: true,
            installed_version: Some("1.2.3".into()),
            unity_version: Some("2022.3".into()),
            ..AppConfig::default()
        };
        let snapshot = snapshot(phrase!("game.running"), &config, true);
        let english_headline = snapshot.headline.render();
        let english_details = snapshot.details.clone();
        let kept = snapshot.clone();

        assert!(hold.select(Some("fr")));
        // The model is untouched: same phrase, same cached details string.
        assert_eq!(snapshot, kept);
        assert_ne!(snapshot.headline.render(), english_headline);
        // Details are derived from config, so the window recomputes them.
        let french_details = details(&config, true);
        assert_ne!(french_details, english_details);
        assert_eq!(snapshot.headline, kept.headline);
    }
}
