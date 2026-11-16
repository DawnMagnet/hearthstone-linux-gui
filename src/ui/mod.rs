mod app;
mod auth;
mod browser;
mod components;
mod game_settings;
mod login_window;
mod logo;
mod status;
mod system_info;
mod text;

use relm4::gtk::prelude::*;
use relm4::{
    adw,
    gtk::{
        self,
        gio::{self, prelude::ApplicationExtManual},
    },
};

use hearthstone_linux::{auth as core_auth, paths::AppPaths};

pub fn run() {
    tracing::debug!("creating Relm4 application");
    let gtk_app = adw::Application::builder()
        .application_id("io.github.hearthstone_linux_gui")
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    gtk_app.connect_startup(|_| configure_color_scheme());
    gtk_app.connect_open(handle_open);

    app::run(gtk_app);
}

fn handle_open(_app: &adw::Application, files: &[gio::File], _hint: &str) {
    let paths = match AppPaths::discover() {
        Ok(paths) => paths,
        Err(error) => {
            tracing::error!(error = %format!("{error:#}"), "failed to resolve paths for auth callback");
            return;
        }
    };

    handle_callback_files(&paths, files);
}

fn handle_callback_files(paths: &AppPaths, files: &[gio::File]) {
    for file in files {
        let uri = file.uri();
        tracing::info!("handling auth callback from open event");
        match core_auth::handle_callback_uri(paths, uri.as_str()) {
            Ok(()) => tracing::info!("auth callback handled"),
            Err(error) => {
                tracing::error!(error = %format!("{error:#}"), "failed to handle auth callback")
            }
        }
    }
}

fn configure_color_scheme() {
    let prefer_dark = gtk::Settings::default().is_some_and(|settings| {
        let prefer_dark = settings.is_gtk_application_prefer_dark_theme();
        if prefer_dark {
            settings.set_gtk_application_prefer_dark_theme(false);
        }
        prefer_dark
    });

    adw::StyleManager::default().set_color_scheme(if prefer_dark {
        adw::ColorScheme::PreferDark
    } else {
        adw::ColorScheme::Default
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_event_writes_the_token_without_logging_the_callback_uri() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        let paths = AppPaths {
            config_dir: root.join("config"),
            data_dir: root.join("data"),
            cache_dir: root.join("cache"),
            state_dir: root.join("state"),
            config_file: root.join("config/config.toml"),
            game_dir: root.join("game"),
            ngdp_dir: root.join("cache/ngdp"),
            unity_cache_dir: root.join("cache/unity"),
            log_dir: root.join("state/logs"),
        };
        let token = "US-0123456789ABCDEFGHIJKLMNOPQRSTUV-1234567";
        let uri = format!("blizzard-hearthstone://localhost:0/?ST={token}");
        let log = root.join("captured.log");
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_writer(std::sync::Mutex::new(std::fs::File::create(&log).unwrap()))
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            handle_callback_files(&paths, &[gio::File::for_uri(&uri)]);
        });
        assert_eq!(std::fs::read(paths.game_token()).unwrap().len(), 48);
        let logs = std::fs::read_to_string(log).unwrap();
        assert!(logs.contains("auth callback handled"), "{logs}");
        assert!(!logs.contains(token), "callback token leaked into logs");
        assert!(!logs.contains(&uri), "callback URI leaked into logs");
    }
}
