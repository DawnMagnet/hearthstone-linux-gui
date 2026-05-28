use crate::ui::{
    auth_callback::ensure_auth_scheme_handlers,
    browser::open_login_url,
    state::{LoginSession, UiMessage},
    status::{
        mark_logged_out, preserve_install_metadata, set_install_idle, set_install_running,
        set_install_stopping, set_launch_idle, set_launch_running, set_launch_stopping,
        set_login_idle, set_login_waiting, sync_config_from_disk, update_login_button,
        update_status,
    },
    theme::ColorScheme,
};
use fltk::{app, button::Button, dialog, frame::Frame, menu::Choice, misc::Progress, prelude::*};
use hearthstone_linux::{
    auth::{start_local_callback_server, LocalCallbackServer},
    config::{AppConfig, Locale, Region},
    install::{
        launcher,
        manager::{InstallManager, TaskEvent},
    },
    paths::AppPaths,
};
use std::{
    cell::RefCell,
    process::Child,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

pub(crate) fn connect_region(region: &mut Choice, config: Rc<RefCell<AppConfig>>) {
    region.set_callback(move |choice| {
        let index = choice.value();
        let Some(region) = usize::try_from(index)
            .ok()
            .and_then(|index| Region::ALL.get(index))
            .copied()
        else {
            return;
        };
        config.borrow_mut().region = region;
    });
}

pub(crate) fn connect_locale(locale: &mut Choice, config: Rc<RefCell<AppConfig>>) {
    locale.set_callback(move |choice| {
        let index = choice.value();
        let Some(locale) = usize::try_from(index)
            .ok()
            .and_then(|index| Locale::ALL.get(index))
            .copied()
        else {
            return;
        };
        config.borrow_mut().locale = locale;
    });
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn connect_install(
    install: &mut Button,
    paths: Rc<AppPaths>,
    config: Rc<RefCell<AppConfig>>,
    mut status: Frame,
    _version: Frame,
    mut progress: Progress,
    install_cancel: Rc<RefCell<Option<Arc<AtomicBool>>>>,
    sender: app::Sender<UiMessage>,
    color_scheme: Rc<RefCell<ColorScheme>>,
) {
    install.set_callback(move |button| {
        if let Some(cancel) = install_cancel.borrow().as_ref() {
            tracing::info!("install stop requested from UI");
            cancel.store(true, Ordering::Relaxed);
            set_install_stopping(button.clone(), *color_scheme.borrow());
            status.set_label("Stopping installation");
            return;
        }

        let install_action = if paths.game_dir.join("Bin/Hearthstone.x86_64").exists() {
            "Update"
        } else {
            "Install"
        };
        let cancel = Arc::new(AtomicBool::new(false));
        *install_cancel.borrow_mut() = Some(cancel.clone());
        set_install_running(button.clone(), install_action, *color_scheme.borrow());
        tracing::info!(action = install_action, "install/update requested from UI");
        progress.show();
        progress.set_value(0.0);
        progress.set_label("0%");
        status.set_label("Preparing");

        let paths_for_thread = (*paths).clone();
        let mut config_for_thread = config.borrow().clone();
        let cancel_for_thread = cancel.clone();
        let sender_for_thread = sender;
        std::thread::spawn(move || {
            let manager = InstallManager::new(paths_for_thread);
            let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
            let result = runtime.block_on(manager.install_or_update_cancellable(
                &mut config_for_thread,
                |event| {
                    sender_for_thread.send(UiMessage::InstallEvent(event));
                },
                cancel_for_thread.clone(),
            ));
            if let Err(error) = result {
                tracing::error!(error = %format!("{error:#}"), "install/update failed");
                let event = if cancel_for_thread.load(Ordering::Relaxed) {
                    TaskEvent::Cancelled("Installation cancelled".into())
                } else {
                    TaskEvent::Failed(format!("{error:#}"))
                };
                sender_for_thread.send(UiMessage::InstallEvent(event));
            }
        });
    });
}

pub(crate) fn connect_login(
    login: &mut Button,
    paths: Rc<AppPaths>,
    config: Rc<RefCell<AppConfig>>,
    status: Frame,
    version: Frame,
    login_session: Rc<RefCell<Option<LoginSession>>>,
    color_scheme: Rc<RefCell<ColorScheme>>,
) {
    login.set_callback(move |button| {
        if let Some(session) = login_session.borrow_mut().take() {
            tracing::info!("login wait cancelled from UI");
            session.cancel();
            set_login_idle(button.clone(), *color_scheme.borrow());
            let mut status = status.clone();
            status.set_label("Login cancelled");
            return;
        }

        if paths.game_token().exists() {
            tracing::debug!("login token already exists");
            show_account_dialog(
                paths.clone(),
                config.clone(),
                status.clone(),
                version.clone(),
                button.clone(),
                login_session.clone(),
                color_scheme.clone(),
            );
            return;
        }

        begin_login(
            paths.clone(),
            config.clone(),
            status.clone(),
            version.clone(),
            button.clone(),
            login_session.clone(),
            color_scheme.clone(),
        );
    });
}

pub(crate) fn connect_launch(
    launch: &mut Button,
    paths: Rc<AppPaths>,
    config: Rc<RefCell<AppConfig>>,
    mut status: Frame,
    running_game: Rc<RefCell<Option<Child>>>,
    color_scheme: Rc<RefCell<ColorScheme>>,
) {
    launch.set_callback(move |button| {
        if let Some(child) = running_game.borrow_mut().as_mut() {
            tracing::info!("game stop requested from UI");
            match child.kill() {
                Ok(()) => {
                    set_launch_stopping(button.clone(), *color_scheme.borrow());
                    status.set_label("Stopping game");
                }
                Err(error) => {
                    tracing::error!(error = %error, "failed to stop game");
                    status.set_label(&format!("Failed to stop game: {error}"));
                }
            }
            return;
        }

        let game_dir = config
            .borrow()
            .game_dir
            .clone()
            .unwrap_or(paths.game_dir.clone());
        tracing::info!(game_dir = %game_dir.display(), "launch requested from UI");
        match launcher::launch_game(&game_dir) {
            Ok(child) => {
                status.set_label("Game running");
                set_launch_running(button.clone(), *color_scheme.borrow());
                *running_game.borrow_mut() = Some(child);
            }
            Err(error) => {
                tracing::error!(error = %format!("{error:#}"), "launch failed");
                status.set_label(&format!("Launch failed: {error:#}"));
            }
        }
    });
}

pub(crate) fn connect_refresh(
    refresh: &mut Button,
    paths: Rc<AppPaths>,
    config: Rc<RefCell<AppConfig>>,
    mut status: Frame,
    mut version: Frame,
    login_button: Button,
    color_scheme: Rc<RefCell<ColorScheme>>,
) {
    refresh.set_callback(move |_| {
        tracing::debug!("refresh requested from UI");
        sync_config_from_disk(&paths, &config);
        update_status(&mut status, &mut version, &paths);
        update_login_button(login_button.clone(), &paths, *color_scheme.borrow());
    });
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_install_event(
    event: TaskEvent,
    paths: Rc<AppPaths>,
    config: Rc<RefCell<AppConfig>>,
    mut status: Frame,
    mut version: Frame,
    mut progress: Progress,
    install_button: Button,
    install_cancel: Rc<RefCell<Option<Arc<AtomicBool>>>>,
    color_scheme: Rc<RefCell<ColorScheme>>,
) {
    match event {
        TaskEvent::Started(message) => {
            status.set_label(&message);
            progress.show();
            progress.set_label("");
        }
        TaskEvent::Progress { message, fraction } => {
            status.set_label(&message);
            progress.show();
            if let Some(fraction) = fraction {
                progress.set_value(fraction);
                progress.set_label(&format!("{:.0}%", fraction * 100.0));
            } else {
                progress.set_label("");
            }
        }
        TaskEvent::Finished(message) => {
            tracing::info!("install/update finished");
            status.set_label(&message);
            progress.set_value(1.0);
            progress.set_label("100%");
            progress.hide();
            *install_cancel.borrow_mut() = None;
            set_install_idle(install_button, *color_scheme.borrow());
            sync_config_from_disk(&paths, &config);
            update_status(&mut status, &mut version, &paths);
        }
        TaskEvent::Failed(message) => {
            tracing::error!(error = %message, "install/update failed in UI");
            status.set_label(&format!("Failed: {message}"));
            progress.hide();
            *install_cancel.borrow_mut() = None;
            set_install_idle(install_button, *color_scheme.borrow());
        }
        TaskEvent::Cancelled(message) => {
            tracing::info!("install/update cancelled");
            status.set_label(&message);
            progress.hide();
            *install_cancel.borrow_mut() = None;
            set_install_idle(install_button, *color_scheme.borrow());
            sync_config_from_disk(&paths, &config);
            update_status(&mut status, &mut version, &paths);
        }
    }
}

pub(crate) fn poll_login(
    paths: Rc<AppPaths>,
    config: Rc<RefCell<AppConfig>>,
    mut status: Frame,
    mut version: Frame,
    login_button: Button,
    login_session: Rc<RefCell<Option<LoginSession>>>,
    color_scheme: Rc<RefCell<ColorScheme>>,
) {
    let Some(cancelled) = login_session
        .borrow()
        .as_ref()
        .map(LoginSession::is_cancelled)
    else {
        return;
    };
    if cancelled {
        return;
    }

    let token_exists = paths.game_token().exists();
    let config_logged_in = AppConfig::load_or_default(&paths.config_file)
        .map(|config| config.logged_in)
        .unwrap_or(false);
    if token_exists || config_logged_in {
        tracing::info!("browser login completed");
        *login_session.borrow_mut() = None;
        sync_config_from_disk(&paths, &config);
        status.set_label("Login complete");
        update_status(&mut status, &mut version, &paths);
        update_login_button(login_button, &paths, *color_scheme.borrow());
    }
}

pub(crate) fn poll_game(
    mut status: Frame,
    launch_button: Button,
    running_game: Rc<RefCell<Option<Child>>>,
    color_scheme: Rc<RefCell<ColorScheme>>,
) {
    let mut game = running_game.borrow_mut();
    let Some(child) = game.as_mut() else {
        return;
    };

    match child.try_wait() {
        Ok(Some(exit)) => {
            tracing::info!(status = %exit, "game exited");
            status.set_label("Game stopped");
            *game = None;
            set_launch_idle(launch_button, *color_scheme.borrow());
        }
        Ok(None) => {}
        Err(error) => {
            tracing::error!(error = %error, "failed to poll game process");
            status.set_label(&format!("Game status error: {error}"));
            *game = None;
            set_launch_idle(launch_button, *color_scheme.borrow());
        }
    }
}

fn show_account_dialog(
    paths: Rc<AppPaths>,
    config: Rc<RefCell<AppConfig>>,
    mut status: Frame,
    mut version: Frame,
    login_button: Button,
    login_session: Rc<RefCell<Option<LoginSession>>>,
    color_scheme: Rc<RefCell<ColorScheme>>,
) {
    match dialog::choice2_default(
        "Account is logged in",
        "Switch Account",
        "Log Out",
        "Cancel",
    ) {
        Some(0) => match mark_logged_out(&paths, &config) {
            Ok(()) => {
                update_login_button(login_button.clone(), &paths, *color_scheme.borrow());
                begin_login(
                    paths,
                    config,
                    status,
                    version,
                    login_button,
                    login_session,
                    color_scheme,
                );
            }
            Err(error) => {
                tracing::error!(error = %format!("{error:#}"), "failed to clear previous login");
                status.set_label(&format!("Switch account failed: {error:#}"));
            }
        },
        Some(1) => match mark_logged_out(&paths, &config) {
            Ok(()) => {
                sync_config_from_disk(&paths, &config);
                status.set_label("Logged out");
                update_status(&mut status, &mut version, &paths);
                update_login_button(login_button, &paths, *color_scheme.borrow());
            }
            Err(error) => {
                tracing::error!(error = %format!("{error:#}"), "logout failed");
                status.set_label(&format!("Logout failed: {error:#}"));
            }
        },
        _ => {}
    }
}

fn begin_login(
    paths: Rc<AppPaths>,
    config: Rc<RefCell<AppConfig>>,
    mut status: Frame,
    _version: Frame,
    login_button: Button,
    login_session: Rc<RefCell<Option<LoginSession>>>,
    color_scheme: Rc<RefCell<ColorScheme>>,
) {
    if let Some(session) = login_session.borrow_mut().take() {
        session.cancel();
    }

    let mut current = config.borrow().clone();
    preserve_install_metadata(&paths, &mut current);
    current.game_dir = Some(paths.game_dir.clone());
    if let Err(error) = current.save(&paths.config_file) {
        tracing::error!(error = %format!("{error:#}"), "failed to save login settings");
        status.set_label(&format!("Login setup failed: {error:#}"));
        return;
    }

    let callback: Rc<LocalCallbackServer> = match start_local_callback_server(
        (*paths).clone(),
        current.region,
    ) {
        Ok(callback) => Rc::new(callback),
        Err(error) => {
            tracing::error!(error = %format!("{error:#}"), "failed to start login callback server");
            status.set_label(&format!("Login setup failed: {error:#}"));
            return;
        }
    };

    let login_url = callback.login_url.clone();
    *login_session.borrow_mut() = Some(LoginSession::new(callback));

    if let Err(error) = ensure_auth_scheme_handlers() {
        tracing::warn!(error = %format!("{error:#}"), "failed to register auth URI handlers");
        status.set_label("Login handler registration failed; continuing with browser login");
    }

    set_login_waiting(login_button, *color_scheme.borrow());
    status.set_label("Complete login in browser; waiting for desktop callback");
    tracing::info!(region = %current.region, "opening browser login with desktop callback handler");

    if let Err(error) = open_login_url(&login_url) {
        tracing::error!(
            url = login_url,
            error = %format!("{error:#}"),
            "failed to open browser login"
        );
        status.set_label(&format!("Could not open browser. URL: {login_url}"));
    }
}
