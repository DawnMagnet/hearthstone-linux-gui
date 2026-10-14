use super::{
    auth, browser,
    components::{
        embedded_image, preferences_menu, ActionBar, ActionBarInput, ActionBarOutput,
        ActionBarState, InstallAction, InstallState, LaunchState, LoginState, SettingsOutput,
        SettingsPanel, SettingsPanelInput, StatusPanel, StatusPanelInput, StatusPanelState,
    },
    game_settings, status,
};
use hearthstone_linux::{
    config::AppConfig,
    install::{
        launcher,
        manager::{InstallManager, TaskEvent},
    },
    paths::AppPaths,
    tr,
    update::{self, PackageKind, Release},
};
use relm4::adw::prelude::*;
use relm4::{
    abstractions::Toaster, adw, gtk, gtk::glib, prelude::*, ComponentController, Controller,
    RelmApp,
};
use std::{
    path::PathBuf,
    process::Child,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

const PROJECT_URL: &str = "https://github.com/DawnMagnet/hearthstone-linux-gui";
const COPYRIGHT_TEXT: &str = "Copyright (c) 2026 DawnMagnet";
const APP_VERSION_TEXT: &str = concat!("v", env!("CARGO_PKG_VERSION"));

pub(super) fn run(gtk_app: adw::Application) {
    let relm_app = RelmApp::from_app(gtk_app);
    relm_app.run::<MainWindow>(AppInit::load());
}

struct AppInit {
    paths: AppPaths,
    config: AppConfig,
    snapshot: status::StatusSnapshot,
}

impl AppInit {
    pub fn load() -> Self {
        let paths = AppPaths::discover().expect("XDG paths are required");
        let (config, snapshot) = status::reconcile(&paths);
        Self {
            paths,
            config,
            snapshot,
        }
    }
}

struct MainWindow {
    paths: AppPaths,
    config: AppConfig,
    status: status::StatusSnapshot,
    progress: ProgressState,
    install_state: InstallState,
    install_cancel: Option<CancellationToken>,
    login_session: Option<LoginSession>,
    next_login_id: u64,
    game_session: Option<GameSession>,
    status_panel: Controller<StatusPanel>,
    settings_panel: Controller<SettingsPanel>,
    actions: Controller<ActionBar>,
    toaster: Toaster,
    window: adw::ApplicationWindow,
    /// Status line above the logo.
    details: gtk::Label,
    game_settings_button: gtk::Button,
    update_button: gtk::Button,
    /// Newer launcher release found by the update check.
    update: Option<Release>,
    update_in_progress: bool,
}

#[derive(Clone, Debug)]
pub enum AppMsg {
    Settings(SettingsOutput),
    UiLanguageChanged(Option<&'static str>),
    OpenGameSettings,
    GameSettingsSaved,
    Refresh,
    CheckForUpdatesChanged(bool),
    UpdateFound(Release),
    InstallUpdate,
    UpdateFetched(Result<UpdateOutcome, String>),
    Action(ActionBarOutput),
    InstallEvent(TaskEvent),
    UninstallFinished(Result<(), String>),
    LoginPoll,
    BrowserOpened {
        session: u64,
        result: Result<String, String>,
    },
    GamePoll,
}

struct LoginSession {
    id: u64,
    cancel: CancellationToken,
    url: String,
}

struct GameSession {
    child: Child,
    poll_cancel: CancellationToken,
    started: Instant,
    stop_requested: bool,
    output_log: PathBuf,
}

/// What clicking the upgrade button led to.
#[derive(Clone, Copy, Debug)]
pub enum UpdateOutcome {
    /// The package was downloaded and opened with the desktop's installer.
    PackageOpened,
    /// No package fits this installation; the release page was opened.
    PageOpened,
}

fn fetch_update(
    release: &Release,
    download_dir: &std::path::Path,
) -> anyhow::Result<UpdateOutcome> {
    let Some(asset) = release.asset_for(PackageKind::detect()) else {
        browser::open_url(&release.page_url)?;
        return Ok(UpdateOutcome::PageOpened);
    };
    let runtime = tokio::runtime::Runtime::new()?;
    let package = runtime.block_on(update::download(asset, download_dir))?;
    let url = url::Url::from_file_path(&package)
        .map_err(|()| anyhow::anyhow!("{} is not an absolute path", package.display()))?;
    browser::open_url(url.as_str())?;
    Ok(UpdateOutcome::PackageOpened)
}

/// Makes the upgrade button pulse so it is noticed without moving anything.
const UPDATE_BUTTON_CSS: &str = "
.update-available {
    animation: update-glow 1.4s ease-in-out infinite alternate;
}
@keyframes update-glow {
    from { box-shadow: 0 0 2px 0 alpha(@accent_bg_color, 0.35); }
    to { box-shadow: 0 0 12px 4px alpha(@accent_bg_color, 0.95); }
}
";

#[derive(Clone, Debug, Default)]
pub struct ProgressState {
    pub visible: bool,
    pub fraction: Option<f64>,
    pub text: Option<String>,
}

#[relm4::component]
impl SimpleComponent for MainWindow {
    type Init = AppInit;
    type Input = AppMsg;
    type Output = ();

    view! {
        main_window = adw::ApplicationWindow {
            set_title: Some(&tr!("app.name")),
            // Smaller than any layout: the window opens at its minimum size
            // instead of the wider natural size of the header bar.
            set_default_size: (1, 1),

            #[local_ref]
            toast_overlay -> adw::ToastOverlay {
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,

                    adw::HeaderBar {
                        #[wrap(Some)]
                        set_title_widget = &adw::WindowTitle {
                            set_title: &tr!("app.name"),
                            // Keeps the launcher from passing as an official product.
                            set_subtitle: &tr!("app.unofficial"),
                        }
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 18,
                        set_margin_top: 12,
                        set_margin_bottom: 18,
                        set_margin_start: 18,
                        set_margin_end: 18,

                        // One centred block. Status line, settings and
                        // actions start flush at its left edge; the logo and
                        // the headline are centred in it.
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_halign: gtk::Align::Center,

                            // Status line with refresh to its right.
                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 12,
                                set_margin_bottom: 16,

                                #[local_ref]
                                details -> gtk::Label {
                                    set_hexpand: true,
                                    set_xalign: 0.0,
                                    add_css_class: relm4::css::DIM_LABEL,
                                },

                                #[local_ref]
                                refresh -> gtk::Button {
                                    set_valign: gtk::Align::Center,
                                },
                            },

                            // The logo spans the block. On its right, below
                            // refresh: the settings menu and, once a newer
                            // release was found, the upgrade button.
                            #[local_ref]
                            logo -> gtk::Overlay {},

                            #[local_ref]
                            status_panel -> gtk::Box {
                                set_margin_bottom: 32,
                            },

                            #[local_ref]
                            settings_panel -> gtk::Grid {},

                            #[local_ref]
                            actions -> gtk::Box {
                                set_margin_top: 16,
                            },
                        },

                        gtk::CenterBox {
                            set_margin_top: 4,

                            #[wrap(Some)]
                            set_center_widget = &gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 2,
                                set_halign: gtk::Align::Center,

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 8,
                                    set_halign: gtk::Align::Center,
                                    set_valign: gtk::Align::Center,

                                    gtk::Label {
                                        add_css_class: relm4::css::DIM_LABEL,
                                        set_label: COPYRIGHT_TEXT,
                                    },

                                    gtk::Label {
                                        add_css_class: relm4::css::DIM_LABEL,
                                        set_label: APP_VERSION_TEXT,
                                    },
                                },

                                gtk::LinkButton {
                                    set_label: "github.com/DawnMagnet/hearthstone-linux-gui",
                                    set_uri: PROJECT_URL,
                                    set_tooltip_text: Some(PROJECT_URL),

                                    // Open through our host-environment-aware
                                    // openers instead of GTK's default handler.
                                    connect_activate_link => move |button| {
                                        let uri = button.uri().to_string();
                                        std::thread::spawn(move || {
                                            if let Err(error) = browser::open_url(&uri) {
                                                tracing::warn!(error = %format!("{error:#}"), "failed to open project URL");
                                            }
                                        });
                                        glib::Propagation::Stop
                                    },
                                },
                            },
                        },
                    }
                }
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        tracing::debug!("building Relm4 main window");

        let status_panel = StatusPanel::builder()
            .launch(StatusPanelState {
                snapshot: init.snapshot.clone(),
                progress: ProgressState::default(),
                login_url: None,
            })
            .detach();
        let settings_panel = SettingsPanel::builder()
            .launch(init.config.clone())
            .forward(sender.input_sender(), AppMsg::Settings);
        let actions = ActionBar::builder()
            .launch(ActionBarState::default())
            .forward(sender.input_sender(), AppMsg::Action);
        let toaster = Toaster::default();
        let preferences_menu = {
            let open_input = sender.input_sender().clone();
            let language_input = sender.input_sender().clone();
            let updates_input = sender.input_sender().clone();
            preferences_menu(
                init.config.ui_language.as_deref(),
                init.config.check_for_updates,
                move || open_input.emit(AppMsg::OpenGameSettings),
                move |language| language_input.emit(AppMsg::UiLanguageChanged(language)),
                move |enabled| updates_input.emit(AppMsg::CheckForUpdatesChanged(enabled)),
            )
        };

        let refresh = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh.set_tooltip_text(Some(&tr!("action.refresh")));
        refresh.add_css_class(relm4::css::FLAT);
        {
            let input = sender.input_sender().clone();
            refresh.connect_clicked(move |_| input.emit(AppMsg::Refresh));
        }

        relm4::set_global_css(UPDATE_BUTTON_CSS);
        let update_button = gtk::Button::from_icon_name("software-update-available-symbolic");
        update_button.set_tooltip_text(Some(&tr!("update.install_tooltip")));
        update_button.add_css_class(relm4::css::CIRCULAR);
        update_button.add_css_class(relm4::css::SUGGESTED_ACTION);
        update_button.add_css_class("update-available");
        update_button.set_halign(gtk::Align::Center);
        update_button.set_visible(false);
        {
            let input = sender.input_sender().clone();
            update_button.connect_clicked(move |_| input.emit(AppMsg::InstallUpdate));
        }
        if init.config.check_for_updates {
            Self::check_for_updates(&sender);
        }

        let model = MainWindow {
            paths: init.paths,
            config: init.config,
            status: init.snapshot,
            progress: ProgressState::default(),
            install_state: InstallState::Idle,
            install_cancel: None,
            login_session: None,
            next_login_id: 0,
            game_session: None,
            status_panel,
            settings_panel,
            actions,
            toaster,
            window: root.clone(),
            details: gtk::Label::new(None),
            game_settings_button: preferences_menu.game_settings,
            update_button: update_button.clone(),
            update: None,
            update_in_progress: false,
        };

        let logo_image = embedded_image(include_bytes!("../../data/icons/logo.png"));
        logo_image.set_pixel_size(184);
        logo_image.set_halign(gtk::Align::Center);
        // Room on both sides keeps the corner buttons outside the picture.
        logo_image.set_margin_start(48);
        logo_image.set_margin_end(48);
        let logo = gtk::Overlay::new();
        logo.set_child(Some(&logo_image));
        let side = gtk::Box::new(gtk::Orientation::Vertical, 6);
        side.set_halign(gtk::Align::End);
        side.set_valign(gtk::Align::Start);
        side.append(&preferences_menu.button);
        side.append(&update_button);
        logo.add_overlay(&side);
        let details = &model.details;
        let toast_overlay = model.toaster.overlay_widget();
        let status_panel = model.status_panel.widget();
        let settings_panel = model.settings_panel.widget();
        let actions = model.actions.widget();

        let widgets = view_output!();
        model.render_children();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            AppMsg::Settings(output) => self.handle_settings(output),
            AppMsg::UiLanguageChanged(language) => self.handle_ui_language(language),
            AppMsg::OpenGameSettings => self.open_game_settings(sender),
            AppMsg::GameSettingsSaved => self.toast(&tr!("game_settings.saved")),
            AppMsg::Refresh => self.refresh_from_disk(),
            AppMsg::CheckForUpdatesChanged(enabled) => {
                self.config.check_for_updates = enabled;
                self.save_settings();
                if enabled {
                    Self::check_for_updates(&sender);
                } else {
                    self.update = None;
                }
            }
            AppMsg::UpdateFound(release) => self.update = Some(release),
            AppMsg::InstallUpdate => self.install_update(sender),
            AppMsg::UpdateFetched(result) => self.handle_update_fetched(result),
            AppMsg::Action(output) => self.handle_action(output, sender),
            AppMsg::InstallEvent(event) => self.handle_install_event(event),
            AppMsg::UninstallFinished(result) => self.handle_uninstall_finished(result),
            AppMsg::LoginPoll => self.handle_login_poll(),
            AppMsg::BrowserOpened { session, result } => {
                self.handle_browser_opened(session, result)
            }
            AppMsg::GamePoll => self.handle_game_poll(),
        }

        self.render_children();
    }
}

impl MainWindow {
    fn handle_settings(&mut self, output: SettingsOutput) {
        match output {
            SettingsOutput::RegionChanged(region) => self.config.region = region,
            SettingsOutput::LocaleChanged(locale) => self.config.locale = locale,
            SettingsOutput::DiscreteGpuChanged(use_discrete_gpu) => {
                self.config.use_discrete_gpu = use_discrete_gpu;
            }
            SettingsOutput::GameModeChanged(use_gamemode) => {
                self.config.use_gamemode = use_gamemode;
            }
            SettingsOutput::MangoHudChanged(use_mangohud) => {
                self.config.use_mangohud = use_mangohud;
            }
        }
        self.save_settings();
        self.refresh_details();
    }

    fn handle_ui_language(&mut self, language: Option<&'static str>) {
        if self.config.ui_language.as_deref() == language {
            return;
        }
        self.config.ui_language = language.map(str::to_owned);
        self.save_settings();
        self.refresh_details();

        // Texts are fixed once the window is built, so the language is
        // applied by restarting. Never interrupt an install, a pending login
        // or a running game for that.
        let busy = self.install_state != InstallState::Idle
            || self.login_session.is_some()
            || self.game_session.is_some();
        if busy {
            self.toast(&tr!("settings.language_after_restart"));
        } else if let Err(error) = self.restart() {
            tracing::error!(%error, "failed to restart the launcher");
            self.toast(&tr!("settings.language_after_restart"));
        }
    }

    /// Starts a new launcher and closes this one.
    fn restart(&self) -> std::io::Result<()> {
        let launcher = auth::launcher_executable()?;
        tracing::info!(launcher = %launcher.display(), "restarting the launcher");
        // The new instance has to start after this one has gone, otherwise
        // it only activates this window and exits.
        hearthstone_linux::host_env::host_command("sh")
            .args(["-c", "sleep 0.7; exec \"$0\""])
            .arg(launcher)
            .stdin(std::process::Stdio::null())
            .spawn()?;
        self.window.close();
        Ok(())
    }

    fn open_game_settings(&mut self, sender: ComponentSender<Self>) {
        // The game rewrites its options on exit, so edits made meanwhile
        // would be lost.
        if self.game_session.is_some() {
            return;
        }
        let input = sender.input_sender().clone();
        game_settings::open(&self.window, &self.paths, &self.config, move || {
            input.emit(AppMsg::GameSettingsSaved);
        });
    }

    /// Asks for the latest release in the background. Failures only go to
    /// the log: being offline must not disturb the launcher.
    fn check_for_updates(sender: &ComponentSender<Self>) {
        let input = sender.input_sender().clone();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
            match runtime.block_on(update::latest_release()) {
                Ok(release) if release.is_upgrade() => {
                    tracing::info!(version = %release.version, "launcher upgrade available");
                    input.emit(AppMsg::UpdateFound(release));
                }
                Ok(release) => {
                    tracing::debug!(version = %release.version, "launcher is up to date")
                }
                Err(error) => {
                    tracing::warn!(error = %format!("{error:#}"), "launcher update check failed")
                }
            }
        });
    }

    /// Downloads the package for this installation and hands it to the
    /// desktop's installer, or opens the release page when there is none.
    fn install_update(&mut self, sender: ComponentSender<Self>) {
        let Some(release) = self.update.clone() else {
            return;
        };
        if self.update_in_progress {
            return;
        }
        self.update_in_progress = true;
        self.status.headline = tr!("update.downloading", version = release.version);
        self.refresh_details();

        let download_dir = directories::UserDirs::new()
            .and_then(|dirs| dirs.download_dir().map(PathBuf::from))
            .unwrap_or_else(|| self.paths.cache_dir.clone());
        let input = sender.input_sender().clone();
        std::thread::spawn(move || {
            let result =
                fetch_update(&release, &download_dir).map_err(|error| format!("{error:#}"));
            input.emit(AppMsg::UpdateFetched(result));
        });
    }

    fn handle_update_fetched(&mut self, result: Result<UpdateOutcome, String>) {
        self.update_in_progress = false;
        let version = self
            .update
            .as_ref()
            .map(|release| release.version.clone())
            .unwrap_or_default();
        self.status.headline = match result {
            Ok(UpdateOutcome::PackageOpened) => tr!("update.downloaded", version = version),
            Ok(UpdateOutcome::PageOpened) => tr!("update.opened_page", version = version),
            Err(error) => {
                tracing::error!(%error, "launcher upgrade failed");
                tr!("update.failed", error = error)
            }
        };
        self.refresh_details();
    }

    fn save_settings(&mut self) {
        if let Err(error) = self.config.save(&self.paths.config_file) {
            tracing::error!(error = %format!("{error:#}"), "failed to save settings");
            self.status.headline = tr!("settings.save_failed", error = format!("{error:#}"));
        }
    }

    fn handle_action(&mut self, output: ActionBarOutput, sender: ComponentSender<Self>) {
        match output {
            ActionBarOutput::Install => self.handle_install_pressed(sender),
            ActionBarOutput::Uninstall => self.handle_uninstall_pressed(sender),
            ActionBarOutput::Account => self.handle_account_pressed(sender),
            ActionBarOutput::Logout => self.handle_logout(),
            ActionBarOutput::SwitchAccount => self.handle_switch_account(sender),
        }
    }

    fn handle_install_pressed(&mut self, sender: ComponentSender<Self>) {
        if let InstallState::Running(_) = self.install_state {
            self.stop_install();
            return;
        }
        if self.install_state == InstallState::Stopping {
            return;
        }

        let action = if self.paths.game_dir.join("Bin/Hearthstone.x86_64").exists() {
            InstallAction::Update
        } else {
            InstallAction::Install
        };
        let cancel = CancellationToken::new();
        self.install_state = InstallState::Running(action);
        self.install_cancel = Some(cancel.clone());
        self.progress = ProgressState {
            visible: true,
            fraction: Some(0.0),
            text: Some("0%".into()),
        };
        self.status.headline = tr!("install.preparing");

        tracing::info!(?action, "install/update requested from UI");
        let paths = self.paths.clone();
        let mut config = self.config.clone();
        let cancel_for_thread = cancel.clone();
        let input = sender.input_sender().clone();
        std::thread::spawn(move || {
            let manager = InstallManager::new(paths);
            let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
            let result = runtime.block_on(manager.install_or_update_cancellable(
                &mut config,
                |event| input.emit(AppMsg::InstallEvent(event)),
                cancel_for_thread.clone(),
            ));
            if let Err(error) = result {
                tracing::error!(error = %format!("{error:#}"), "install/update failed");
                let event = if cancel_for_thread.is_cancelled() {
                    TaskEvent::Cancelled(tr!("install.cancelled"))
                } else {
                    TaskEvent::Failed(format!("{error:#}"))
                };
                input.emit(AppMsg::InstallEvent(event));
            }
        });
    }

    fn stop_install(&mut self) {
        tracing::info!("install stop requested from UI");
        if let Some(cancel) = self.install_cancel.as_ref() {
            cancel.cancel();
            self.install_state = InstallState::Stopping;
            self.status.headline = tr!("install.stopping");
        }
    }

    fn handle_uninstall_pressed(&mut self, sender: ComponentSender<Self>) {
        if self.game_session.is_some() {
            self.status.headline = tr!("install.stop_game_first");
            self.refresh_details();
            return;
        }
        if self.install_state != InstallState::Idle || !self.is_installed() {
            return;
        }

        self.install_state = InstallState::Uninstalling;
        self.progress = ProgressState {
            visible: true,
            fraction: None,
            text: Some(tr!("install.removing")),
        };
        self.status.headline = tr!("install.uninstalling");

        tracing::info!("uninstall requested from UI");
        let paths = self.paths.clone();
        let mut config = self.config.clone();
        let input = sender.input_sender().clone();
        std::thread::spawn(move || {
            let manager = InstallManager::new(paths);
            let result = manager
                .uninstall(&mut config)
                .map_err(|error| format!("{error:#}"));
            input.emit(AppMsg::UninstallFinished(result));
        });
    }

    fn handle_uninstall_finished(&mut self, result: Result<(), String>) {
        self.install_state = InstallState::Idle;
        self.install_cancel = None;
        self.progress.visible = false;

        match result {
            Ok(()) => {
                auth::sync_config_from_disk(&self.paths, &mut self.config);
                self.refresh_status_with_headline(tr!("install.uninstalled"));
                self.toast(&tr!("install.uninstalled"));
            }
            Err(message) => {
                tracing::error!(error = %message, "uninstall failed in UI");
                self.status.headline = tr!("install.uninstall_failed", error = message);
                self.refresh_details();
                self.toast(&tr!("install.uninstall_failed_toast"));
            }
        }
    }

    fn handle_install_event(&mut self, event: TaskEvent) {
        match event {
            TaskEvent::Started(message) => {
                self.status.headline = message;
                self.progress.visible = true;
                self.progress.fraction = None;
                self.progress.text = None;
            }
            TaskEvent::Progress { message, fraction } => {
                self.status.headline = message;
                self.progress.visible = true;
                self.progress.fraction = fraction;
                self.progress.text = fraction.map(|value| format!("{:.0}%", value * 100.0));
            }
            TaskEvent::Finished(message) => {
                tracing::info!("install/update finished");
                self.install_state = InstallState::Idle;
                self.install_cancel = None;
                self.progress = ProgressState {
                    visible: false,
                    fraction: Some(1.0),
                    text: Some("100%".into()),
                };
                auth::sync_config_from_disk(&self.paths, &mut self.config);
                self.refresh_status_with_headline(message);
                self.toast(&tr!("install.ready"));
            }
            TaskEvent::Failed(message) => {
                tracing::error!(error = %message, "install/update failed in UI");
                self.install_state = InstallState::Idle;
                self.install_cancel = None;
                self.progress.visible = false;
                self.status.headline = tr!("install.failed", error = message);
                self.refresh_details();
                self.toast(&tr!("install.failed_toast"));
            }
            TaskEvent::Cancelled(message) => {
                tracing::info!("install/update cancelled");
                self.install_state = InstallState::Idle;
                self.install_cancel = None;
                self.progress.visible = false;
                auth::sync_config_from_disk(&self.paths, &mut self.config);
                self.refresh_status_with_headline(message);
            }
        }
    }

    /// The combined account button: stop a running game, cancel a pending
    /// login, play when logged in, otherwise log in.
    fn handle_account_pressed(&mut self, sender: ComponentSender<Self>) {
        if self.game_session.is_some() {
            self.handle_launch_pressed(sender);
            return;
        }

        if let Some(session) = self.login_session.take() {
            tracing::info!("login wait cancelled from UI");
            session.cancel.cancel();
            self.status.headline = tr!("login.cancelled");
            self.refresh_details();
            return;
        }

        if self.paths.game_token().exists() {
            self.handle_launch_pressed(sender);
        } else {
            self.begin_login(sender);
        }
    }

    fn handle_logout(&mut self) {
        if self.game_session.is_some() {
            return;
        }
        match auth::mark_logged_out(&self.paths, &mut self.config) {
            Ok(()) => {
                auth::sync_config_from_disk(&self.paths, &mut self.config);
                self.refresh_status_with_headline(tr!("status.logged_out"));
            }
            Err(error) => {
                tracing::error!(error = %format!("{error:#}"), "logout failed");
                self.status.headline = tr!("login.logout_failed", error = format!("{error:#}"));
                self.refresh_details();
            }
        }
    }

    fn handle_switch_account(&mut self, sender: ComponentSender<Self>) {
        match auth::mark_logged_out(&self.paths, &mut self.config) {
            Ok(()) => self.begin_login(sender),
            Err(error) => {
                tracing::error!(error = %format!("{error:#}"), "failed to clear previous login");
                self.status.headline = tr!("login.switch_failed", error = format!("{error:#}"));
                self.refresh_details();
            }
        }
    }

    fn begin_login(&mut self, sender: ComponentSender<Self>) {
        if let Some(session) = self.login_session.take() {
            session.cancel.cancel();
        }

        let mut current = self.config.clone();
        auth::preserve_install_metadata(&self.paths, &mut current);
        current.game_dir = Some(self.paths.game_dir.clone());
        if let Err(error) = current.save(&self.paths.config_file) {
            tracing::error!(error = %format!("{error:#}"), "failed to save login settings");
            self.status.headline = tr!("login.setup_failed", error = format!("{error:#}"));
            self.refresh_details();
            return;
        }

        let login_url = current.region.login_url().to_string();
        let cancel = CancellationToken::new();
        self.next_login_id += 1;
        let session_id = self.next_login_id;
        self.login_session = Some(LoginSession {
            id: session_id,
            cancel: cancel.clone(),
            url: login_url.clone(),
        });

        if let Err(error) = auth::ensure_auth_scheme_handlers() {
            tracing::warn!(error = %format!("{error:#}"), "failed to register auth URI handlers");
            self.status.headline = tr!("login.handler_failed");
        } else {
            self.status.headline = tr!("login.opening");
        }
        self.refresh_details();

        tracing::info!(
            region = %current.region,
            url = %login_url,
            "opening browser login with desktop callback handler"
        );
        // Openers are waited on to detect failures, so keep them off the GTK
        // main thread.
        let input = sender.input_sender().clone();
        std::thread::spawn(move || {
            let result = browser::open_url(&login_url).map_err(|error| format!("{error:#}"));
            input.emit(AppMsg::BrowserOpened {
                session: session_id,
                result,
            });
        });

        Self::start_poll(&sender, AppMsg::LoginPoll, cancel);
    }

    fn handle_browser_opened(&mut self, session: u64, result: Result<String, String>) {
        let Some(current) = self.login_session.as_ref() else {
            return;
        };
        if current.id != session || current.cancel.is_cancelled() {
            return;
        }
        match result {
            Ok(opener) => {
                tracing::info!(opener = %opener, "opened login URL");
                self.status.headline = tr!("login.waiting");
            }
            Err(error) => {
                tracing::error!(
                    url = %current.url,
                    error = %error,
                    "failed to open browser login"
                );
                self.status.headline = tr!("login.no_browser");
            }
        }
        self.refresh_details();
    }

    fn handle_login_poll(&mut self) {
        let Some(session) = self.login_session.as_ref() else {
            return;
        };
        if session.cancel.is_cancelled() {
            return;
        }

        let token_exists = self.paths.game_token().exists();
        let config_logged_in = AppConfig::load_or_default(&self.paths.config_file)
            .map(|config| config.logged_in)
            .unwrap_or(false);
        if token_exists || config_logged_in {
            tracing::info!("browser login completed");
            if let Some(session) = self.login_session.take() {
                session.cancel.cancel();
            }
            auth::sync_config_from_disk(&self.paths, &mut self.config);
            self.refresh_status_with_headline(tr!("login.complete"));
            self.toast(&tr!("login.complete"));
        }
    }

    fn handle_launch_pressed(&mut self, sender: ComponentSender<Self>) {
        if let Some(session) = self.game_session.as_mut() {
            tracing::info!("game stop requested from UI");
            match session.child.kill() {
                Ok(()) => {
                    session.stop_requested = true;
                    self.status.headline = tr!("game.stopping");
                    self.refresh_details();
                }
                Err(error) => {
                    tracing::error!(error = %error, "failed to stop game");
                    self.status.headline = tr!("game.stop_failed", error = error);
                    self.refresh_details();
                }
            }
            return;
        }

        let game_dir = self
            .config
            .game_dir
            .clone()
            .unwrap_or_else(|| self.paths.game_dir.clone());
        tracing::info!(game_dir = %game_dir.display(), "launch requested from UI");
        let output_log = self.paths.log_dir.join("game.log");
        match launcher::launch_game(&game_dir, (&self.config).into(), Some(&output_log)) {
            Ok(child) => {
                let poll_cancel = CancellationToken::new();
                self.game_session = Some(GameSession {
                    child,
                    poll_cancel: poll_cancel.clone(),
                    started: Instant::now(),
                    stop_requested: false,
                    output_log,
                });
                self.status.headline = tr!("game.running");
                self.refresh_details();

                Self::start_poll(&sender, AppMsg::GamePoll, poll_cancel);
            }
            Err(error) => {
                tracing::error!(error = %format!("{error:#}"), "launch failed");
                self.status.headline = tr!("game.launch_failed", error = format!("{error:#}"));
                self.refresh_details();
            }
        }
    }

    fn handle_game_poll(&mut self) {
        let Some(session) = self.game_session.as_mut() else {
            return;
        };

        match session.child.try_wait() {
            Ok(Some(exit)) => {
                let elapsed = session.started.elapsed();
                tracing::info!(
                    status = %exit,
                    elapsed_secs = elapsed.as_secs(),
                    "game exited"
                );
                session.poll_cancel.cancel();
                let message = if session.stop_requested {
                    None
                } else {
                    let player_log = launcher::player_log_path();
                    launcher::unexpected_exit_message(
                        &exit,
                        elapsed,
                        player_log.as_deref(),
                        Some(&session.output_log),
                    )
                };
                self.game_session = None;
                match message {
                    Some(message) => {
                        tracing::warn!(%message, "game exited unexpectedly");
                        self.status.headline = message;
                        self.toast(&tr!("game.exited_toast"));
                    }
                    None => self.status.headline = tr!("game.stopped"),
                }
                self.refresh_details();
            }
            Ok(None) => {}
            Err(error) => {
                tracing::error!(error = %error, "failed to poll game process");
                session.poll_cancel.cancel();
                self.game_session = None;
                self.status.headline = tr!("game.status_error", error = error);
                self.refresh_details();
            }
        }
    }

    fn refresh_from_disk(&mut self) {
        tracing::debug!("refresh requested from UI");
        let (config, snapshot) = status::reconcile(&self.paths);
        self.config = config;
        self.status = snapshot;
        self.settings_panel.emit(SettingsPanelInput::DetectTools);
    }

    fn refresh_status_with_headline(&mut self, headline: impl Into<String>) {
        self.status = status::snapshot(headline, &self.config, self.paths.game_token().exists());
    }

    fn refresh_details(&mut self) {
        let headline = self.status.headline.clone();
        self.refresh_status_with_headline(headline);
    }

    fn render_children(&self) {
        self.details.set_label(&self.status.details);
        self.status_panel
            .emit(StatusPanelInput::Render(StatusPanelState {
                snapshot: self.status.clone(),
                progress: self.progress.clone(),
                login_url: self
                    .login_session
                    .as_ref()
                    .map(|session| session.url.clone()),
            }));
        self.settings_panel
            .emit(SettingsPanelInput::SetConfig(self.config.clone()));
        let game_running = self.game_session.is_some();
        self.game_settings_button.set_sensitive(!game_running);
        self.game_settings_button.set_tooltip_text(
            game_running
                .then(|| tr!("game_settings.running_tooltip"))
                .as_deref(),
        );
        self.update_button.set_visible(self.update.is_some());
        self.update_button.set_sensitive(!self.update_in_progress);
        self.actions.emit(ActionBarInput::Render(ActionBarState {
            install: self.install_state.clone(),
            installed: self.is_installed(),
            login: self.login_state(),
            launch: self.launch_state(),
        }));
    }

    fn is_installed(&self) -> bool {
        self.paths.game_dir.join("Bin/Hearthstone.x86_64").exists()
    }

    fn login_state(&self) -> LoginState {
        if self.login_session.is_some() {
            LoginState::Waiting
        } else if self.paths.game_token().exists() {
            LoginState::LoggedIn
        } else {
            LoginState::Idle
        }
    }

    fn launch_state(&self) -> LaunchState {
        match self.game_session.as_ref() {
            Some(session) if session.stop_requested => LaunchState::Stopping,
            Some(_) => LaunchState::Running,
            None => LaunchState::Idle,
        }
    }

    fn toast(&self, title: &str) {
        self.toaster.add_toast(adw::Toast::new(title));
    }

    fn start_poll(sender: &ComponentSender<Self>, message: AppMsg, cancel: CancellationToken) {
        let input = sender.input_sender().clone();
        glib::timeout_add_local(Duration::from_secs(1), move || {
            if cancel.is_cancelled() {
                glib::ControlFlow::Break
            } else {
                input.emit(message.clone());
                glib::ControlFlow::Continue
            }
        });
    }
}
