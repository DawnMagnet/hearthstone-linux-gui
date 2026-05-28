mod actions;
mod auth_callback;
mod browser;
mod state;
mod status;
mod theme;
mod window;

use crate::ui::{
    actions::{
        connect_install, connect_launch, connect_locale, connect_login, connect_refresh,
        connect_region, handle_install_event, poll_game, poll_login,
    },
    state::{LoginSession, UiMessage},
    status::{update_login_button, update_status},
    theme::{apply_theme, detect_color_scheme, ThemeState},
    window::build_window,
};
use fltk::{app, enums::Font, prelude::*};
use hearthstone_linux::paths::AppPaths;
use std::{
    cell::RefCell,
    process::Child,
    rc::Rc,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

pub fn run() {
    tracing::debug!("creating FLTK application");
    let app = app::App::default().with_scheme(app::Scheme::Gtk);
    app::set_font(Font::Helvetica);

    let paths = Rc::new(AppPaths::discover().expect("XDG paths are required"));
    let config = Rc::new(RefCell::new(
        hearthstone_linux::config::AppConfig::load_or_default(&paths.config_file)
            .unwrap_or_default(),
    ));
    let color_scheme = Rc::new(RefCell::new(detect_color_scheme()));
    let install_cancel = Rc::new(RefCell::new(None::<Arc<AtomicBool>>));
    let login_session = Rc::new(RefCell::new(None::<LoginSession>));
    let running_game = Rc::new(RefCell::new(None::<Child>));
    let (sender, receiver) = app::channel::<UiMessage>();

    let mut ui = build_window(&paths, &config);
    apply_theme(
        &mut ui,
        *color_scheme.borrow(),
        theme_state(&paths, &install_cancel, &login_session, &running_game),
    );
    update_status(&mut ui.status, &mut ui.version, &paths);
    update_login_button(ui.login_button.clone(), &paths, *color_scheme.borrow());

    connect_region(&mut ui.region, config.clone());
    connect_locale(&mut ui.locale, config.clone());
    connect_install(
        &mut ui.install_button,
        paths.clone(),
        config.clone(),
        ui.status.clone(),
        ui.version.clone(),
        ui.progress.clone(),
        install_cancel.clone(),
        sender,
        color_scheme.clone(),
    );
    connect_login(
        &mut ui.login_button,
        paths.clone(),
        config.clone(),
        ui.status.clone(),
        ui.version.clone(),
        login_session.clone(),
        color_scheme.clone(),
    );
    connect_launch(
        &mut ui.launch_button,
        paths.clone(),
        config.clone(),
        ui.status.clone(),
        running_game.clone(),
        color_scheme.clone(),
    );
    connect_refresh(
        &mut ui.refresh_button,
        paths.clone(),
        config.clone(),
        ui.status.clone(),
        ui.version.clone(),
        ui.login_button.clone(),
        color_scheme.clone(),
    );

    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        sender.send(UiMessage::Tick);
    });

    ui.window.show();
    while app.wait() {
        while let Some(message) = receiver.recv() {
            match message {
                UiMessage::InstallEvent(event) => handle_install_event(
                    event,
                    paths.clone(),
                    config.clone(),
                    ui.status.clone(),
                    ui.version.clone(),
                    ui.progress.clone(),
                    ui.install_button.clone(),
                    install_cancel.clone(),
                    color_scheme.clone(),
                ),
                UiMessage::Tick => {
                    let detected = detect_color_scheme();
                    if detected != *color_scheme.borrow() {
                        *color_scheme.borrow_mut() = detected;
                    }
                    let scheme = *color_scheme.borrow();
                    apply_theme(
                        &mut ui,
                        scheme,
                        theme_state(&paths, &install_cancel, &login_session, &running_game),
                    );
                    poll_login(
                        paths.clone(),
                        config.clone(),
                        ui.status.clone(),
                        ui.version.clone(),
                        ui.login_button.clone(),
                        login_session.clone(),
                        color_scheme.clone(),
                    );
                    poll_game(
                        ui.status.clone(),
                        ui.launch_button.clone(),
                        running_game.clone(),
                        color_scheme.clone(),
                    );
                }
            }
        }
    }
}

fn theme_state(
    paths: &AppPaths,
    install_cancel: &Rc<RefCell<Option<Arc<AtomicBool>>>>,
    login_session: &Rc<RefCell<Option<LoginSession>>>,
    running_game: &Rc<RefCell<Option<Child>>>,
) -> ThemeState {
    ThemeState {
        logged_in: paths.game_token().exists(),
        install_active: install_cancel.borrow().is_some(),
        login_active: login_session.borrow().is_some(),
        game_active: running_game.borrow().is_some(),
    }
}
