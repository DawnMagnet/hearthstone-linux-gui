use gtk4 as gtk;
use libadwaita as adw;

use adw::prelude::*;
use gtk::{gio, glib};
use hearthstone_linux::{
    config::{AppConfig, Locale, Region},
    install::{
        launcher,
        manager::{InstallManager, TaskEvent},
    },
    paths::AppPaths,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::mpsc,
};

pub fn run() {
    let app = adw::Application::builder()
        .application_id("io.github.hearthstone_linux")
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();
    app.connect_startup(|_| configure_color_scheme());
    app.connect_activate(build_window);
    app.run();
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

fn build_window(app: &adw::Application) {
    let paths = Rc::new(AppPaths::discover().expect("XDG paths are required"));
    let config = Rc::new(RefCell::new(
        AppConfig::load_or_default(&paths.config_file).unwrap_or_default(),
    ));

    let title = adw::WindowTitle::new("Hearthstone Linux", "");
    let header = adw::HeaderBar::builder().title_widget(&title).build();

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);

    let status = gtk::Label::new(None);
    status.set_xalign(0.0);
    status.add_css_class("title-3");

    let version = gtk::Label::new(None);
    version.set_xalign(0.0);
    version.add_css_class("dim-label");

    let progress = gtk::ProgressBar::new();
    progress.set_show_text(true);
    progress.set_visible(false);

    let region = gtk::ComboBoxText::new();
    for item in Region::ALL {
        region.append(Some(item.as_str()), item.as_str());
    }
    region.set_active_id(Some(config.borrow().region.as_str()));

    let locale = gtk::ComboBoxText::new();
    for item in Locale::ALL {
        locale.append(Some(item.as_str()), item.as_str());
    }
    locale.set_active_id(Some(config.borrow().locale.as_str()));

    let install = gtk::Button::with_label("Install / Update");
    install.add_css_class("suggested-action");
    let login = gtk::Button::with_label("Login");
    let launch = gtk::Button::with_label("Play");
    launch.add_css_class("suggested-action");
    let refresh = gtk::Button::with_label("Refresh");

    let settings = gtk::Grid::new();
    settings.set_column_spacing(12);
    settings.set_row_spacing(8);
    settings.attach(&gtk::Label::new(Some("Region")), 0, 0, 1, 1);
    settings.attach(&region, 1, 0, 1, 1);
    settings.attach(&gtk::Label::new(Some("Locale")), 0, 1, 1, 1);
    settings.attach(&locale, 1, 1, 1, 1);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.append(&install);
    actions.append(&login);
    actions.append(&launch);
    actions.append(&refresh);

    content.append(&status);
    content.append(&version);
    content.append(&progress);
    content.append(&settings);
    content.append(&actions);
    root.append(&header);
    root.append(&content);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Hearthstone Linux")
        .default_width(620)
        .default_height(360)
        .content(&root)
        .build();

    update_status(&status, &version, &paths);
    update_login_button(&login, &paths);
    let login_waiting = Rc::new(Cell::new(false));

    {
        let config = config.clone();
        region.connect_changed(move |combo| {
            if let Some(value) = combo.active_id() {
                if let Ok(parsed) = value.parse() {
                    config.borrow_mut().region = parsed;
                }
            }
        });
    }

    {
        let config = config.clone();
        locale.connect_changed(move |combo| {
            if let Some(value) = combo.active_id() {
                if let Ok(parsed) = value.parse() {
                    config.borrow_mut().locale = parsed;
                }
            }
        });
    }

    {
        let paths = paths.clone();
        let config = config.clone();
        let status = status.clone();
        let version = version.clone();
        let progress = progress.clone();
        let install_button = install.clone();
        install.connect_clicked(move |_| {
            install_button.set_sensitive(false);
            progress.set_visible(true);
            progress.set_fraction(0.0);
            progress.set_text(Some("0%"));
            status.set_text("Preparing");

            let (sender, receiver) = mpsc::channel::<TaskEvent>();
            let paths_for_thread = (*paths).clone();
            let mut config_for_thread = config.borrow().clone();
            std::thread::spawn(move || {
                let manager = InstallManager::new(paths_for_thread);
                let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
                let result =
                    runtime.block_on(manager.install_or_update(&mut config_for_thread, |event| {
                        let _ = sender.send(event);
                    }));
                if let Err(error) = result {
                    let _ = sender.send(TaskEvent::Failed(error.to_string()));
                }
            });

            let paths = paths.clone();
            let install_button = install_button.clone();
            let status = status.clone();
            let version = version.clone();
            let progress = progress.clone();
            glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                while let Ok(event) = receiver.try_recv() {
                    match event {
                        TaskEvent::Started(message) => {
                            status.set_text(&message);
                            progress.pulse();
                        }
                        TaskEvent::Progress { message, fraction } => {
                            status.set_text(&message);
                            if let Some(fraction) = fraction {
                                progress.set_fraction(fraction);
                                progress.set_text(Some(&format!("{:.0}%", fraction * 100.0)));
                            } else {
                                progress.pulse();
                                progress.set_text(None);
                            }
                        }
                        TaskEvent::Finished(message) => {
                            status.set_text(&message);
                            progress.set_fraction(1.0);
                            progress.set_text(Some("100%"));
                            progress.set_visible(false);
                            install_button.set_sensitive(true);
                            update_status(&status, &version, &paths);
                            return glib::ControlFlow::Break;
                        }
                        TaskEvent::Failed(message) => {
                            status.set_text(&format!("Failed: {message}"));
                            progress.set_visible(false);
                            install_button.set_sensitive(true);
                            return glib::ControlFlow::Break;
                        }
                    }
                }
                glib::ControlFlow::Continue
            });
        });
    }

    {
        let paths = paths.clone();
        let config = config.clone();
        let status = status.clone();
        let version = version.clone();
        let login_button = login.clone();
        let login_waiting = login_waiting.clone();
        login.connect_clicked(move |_| {
            if login_waiting.get() {
                login_waiting.set(false);
                set_login_idle(&login_button, &paths);
                status.set_text("Login cancelled");
                return;
            }

            if paths.game_token().exists() {
                mark_logged_in(&paths, &config);
                status.set_text("Already logged in");
                update_status(&status, &version, &paths);
                update_login_button(&login_button, &paths);
                return;
            }

            let mut current = config.borrow().clone();
            current.game_dir = Some(paths.game_dir.clone());
            let login_url = current.region.login_url();
            let _ = current.save(&paths.config_file);

            login_waiting.set(true);
            set_login_waiting(&login_button);
            status.set_text("Waiting for browser login");

            let _ = gio::AppInfo::launch_default_for_uri(login_url, None::<&gio::AppLaunchContext>);

            let paths = paths.clone();
            let config = config.clone();
            let status = status.clone();
            let version = version.clone();
            let login_button = login_button.clone();
            let login_waiting = login_waiting.clone();
            glib::timeout_add_local(std::time::Duration::from_secs(1), move || {
                if !login_waiting.get() {
                    return glib::ControlFlow::Break;
                }

                if paths.game_token().exists() {
                    login_waiting.set(false);
                    mark_logged_in(&paths, &config);
                    status.set_text("Login complete");
                    update_status(&status, &version, &paths);
                    update_login_button(&login_button, &paths);
                    return glib::ControlFlow::Break;
                }

                glib::ControlFlow::Continue
            });
        });
    }

    {
        let paths = paths.clone();
        let config = config.clone();
        launch.connect_clicked(move |_| {
            let game_dir = config
                .borrow()
                .game_dir
                .clone()
                .unwrap_or(paths.game_dir.clone());
            let _ = launcher::launch_game(&game_dir);
        });
    }

    {
        let paths = paths.clone();
        let status = status.clone();
        let version = version.clone();
        let login = login.clone();
        refresh.connect_clicked(move |_| {
            update_status(&status, &version, &paths);
            update_login_button(&login, &paths);
        });
    }

    window.present();
}

fn update_status(status: &gtk::Label, version: &gtk::Label, paths: &AppPaths) {
    let installed = paths.game_dir.join("Bin/Hearthstone.x86_64").exists();
    let token = paths.game_token().exists();
    match (installed, token) {
        (true, true) => status.set_text("Ready"),
        (true, false) => status.set_text("Login Required"),
        (false, _) => status.set_text("Not Installed"),
    }

    let config = AppConfig::load_or_default(&paths.config_file).unwrap_or_default();
    version.set_text(&format!(
        "Region {} · Locale {} · Version {}",
        config.region,
        config.locale,
        config.installed_version.as_deref().unwrap_or("none")
    ));
}

fn update_login_button(button: &gtk::Button, paths: &AppPaths) {
    if paths.game_token().exists() {
        button.set_label("Logged In");
        button.remove_css_class("destructive-action");
        button.add_css_class("suggested-action");
    } else {
        set_login_idle(button, paths);
    }
}

fn set_login_idle(button: &gtk::Button, _paths: &AppPaths) {
    button.set_label("Login");
    button.remove_css_class("destructive-action");
    button.remove_css_class("suggested-action");
}

fn set_login_waiting(button: &gtk::Button) {
    button.set_label("Stop");
    button.remove_css_class("suggested-action");
    button.add_css_class("destructive-action");
}

fn mark_logged_in(paths: &AppPaths, config: &Rc<RefCell<AppConfig>>) {
    let mut current = config.borrow_mut();
    current.game_dir = Some(paths.game_dir.clone());
    current.logged_in = true;
    current.last_login_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs().to_string());
    let _ = current.save(&paths.config_file);
}
