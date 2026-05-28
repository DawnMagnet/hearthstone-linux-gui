use crate::ui::theme::{style_button, ButtonRole, ColorScheme};
use fltk::{button::Button, frame::Frame, prelude::*};
use hearthstone_linux::{config::AppConfig, paths::AppPaths};
use std::{cell::RefCell, rc::Rc};

pub(crate) fn update_status(status: &mut Frame, version: &mut Frame, paths: &AppPaths) {
    let installed = paths.game_dir.join("Bin/Hearthstone.x86_64").exists();
    let token = paths.game_token().exists();
    match (installed, token) {
        (true, true) => status.set_label("Ready"),
        (true, false) => status.set_label("Login Required"),
        (false, _) => status.set_label("Not Installed"),
    }

    let config = reconcile_status_config(paths, token);
    let login = if config.logged_in && token {
        "Logged in"
    } else if token {
        "Token present"
    } else {
        "Logged out"
    };
    let game = config
        .installed_version
        .as_deref()
        .filter(|version| !version.is_empty())
        .unwrap_or("Not installed");
    let unity = config
        .unity_version
        .as_deref()
        .filter(|version| !version.is_empty())
        .unwrap_or("Not installed");
    version.set_label(&format!(
        "Login: {login} · Game: {game} · Unity: {unity} · {} / {}",
        config.region, config.locale
    ));
}

pub(crate) fn update_login_button(mut button: Button, paths: &AppPaths, scheme: ColorScheme) {
    if paths.game_token().exists() {
        button.activate();
        button.set_label("Logged In");
        style_button(&mut button, ButtonRole::Suggested, scheme);
    } else {
        set_login_idle(button, scheme);
    }
}

pub(crate) fn set_install_idle(mut button: Button, scheme: ColorScheme) {
    button.activate();
    button.set_label("Install / Update");
    style_button(&mut button, ButtonRole::Suggested, scheme);
}

pub(crate) fn set_install_running(mut button: Button, action: &str, scheme: ColorScheme) {
    button.activate();
    button.set_label(&format!("Stop {action}"));
    style_button(&mut button, ButtonRole::Destructive, scheme);
}

pub(crate) fn set_install_stopping(mut button: Button, scheme: ColorScheme) {
    button.set_label("Stopping...");
    style_button(&mut button, ButtonRole::Destructive, scheme);
    button.deactivate();
}

pub(crate) fn set_login_idle(mut button: Button, scheme: ColorScheme) {
    button.activate();
    button.set_label("Login");
    style_button(&mut button, ButtonRole::Normal, scheme);
}

pub(crate) fn set_login_waiting(mut button: Button, scheme: ColorScheme) {
    button.activate();
    button.set_label("Cancel Login");
    style_button(&mut button, ButtonRole::Destructive, scheme);
}

pub(crate) fn set_launch_idle(mut button: Button, scheme: ColorScheme) {
    button.activate();
    button.set_label("Play");
    style_button(&mut button, ButtonRole::Suggested, scheme);
}

pub(crate) fn set_launch_running(mut button: Button, scheme: ColorScheme) {
    button.activate();
    button.set_label("Stop");
    style_button(&mut button, ButtonRole::Destructive, scheme);
}

pub(crate) fn set_launch_stopping(mut button: Button, scheme: ColorScheme) {
    button.set_label("Stopping...");
    style_button(&mut button, ButtonRole::Destructive, scheme);
    button.deactivate();
}

pub(crate) fn mark_logged_out(
    paths: &AppPaths,
    config: &Rc<RefCell<AppConfig>>,
) -> anyhow::Result<()> {
    let token = paths.game_token();
    if token.exists() {
        std::fs::remove_file(&token)?;
    }

    let mut current = config.borrow_mut();
    preserve_install_metadata(paths, &mut current);
    current.game_dir = Some(paths.game_dir.clone());
    current.logged_in = false;
    current.last_login_at = None;
    current.save(&paths.config_file)
}

pub(crate) fn sync_config_from_disk(paths: &AppPaths, config: &Rc<RefCell<AppConfig>>) {
    if let Ok(saved) = AppConfig::load_or_default(&paths.config_file) {
        *config.borrow_mut() = saved;
    }
}

pub(crate) fn preserve_install_metadata(paths: &AppPaths, config: &mut AppConfig) {
    let Ok(saved) = AppConfig::load_or_default(&paths.config_file) else {
        return;
    };
    if saved.installed_version.is_some() {
        config.installed_version = saved.installed_version;
    }
    if saved.unity_version.is_some() {
        config.unity_version = saved.unity_version;
    }
}

fn reconcile_status_config(paths: &AppPaths, token_exists: bool) -> AppConfig {
    let mut config = AppConfig::load_or_default(&paths.config_file).unwrap_or_default();
    let should_save = config.logged_in != token_exists || config.game_dir.is_none();
    config.logged_in = token_exists;
    if config.game_dir.is_none() {
        config.game_dir = Some(paths.game_dir.clone());
    }
    if should_save {
        let _ = config.save(&paths.config_file);
    }
    config
}
