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
    game_settings_button: gtk::Button,
}

#[derive(Clone, Debug)]
pub enum AppMsg {
    Settings(SettingsOutput),
    UiLanguageChanged(Option<&'static str>),
    OpenGameSettings,
    GameSettingsSaved,
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
            set_default_size: (680, 360),

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

                        #[local_ref]
                        logo -> gtk::Image {
                            set_pixel_size: 128,
                        },

                        #[local_ref]
                        status_panel -> gtk::Box {},

                        #[local_ref]
                        settings_panel -> gtk::Grid {},

                        #[local_ref]
                        actions -> gtk::Box {},

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

                            // Launcher preferences sit in the bottom right corner.
                            set_end_widget: Some(&preferences),
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
            preferences_menu(
                init.config.ui_language.as_deref(),
                move || open_input.emit(AppMsg::OpenGameSettings),
                move |language| language_input.emit(AppMsg::UiLanguageChanged(language)),
            )
        };
        let preferences = preferences_menu.button.clone();

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
            game_settings_button: preferences_menu.game_settings,
        };

        let logo = embedded_image(include_bytes!("../../data/icons/logo.png"));
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
        self.config.ui_language = language.map(str::to_owned);
        self.save_settings();
        self.refresh_details();
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
            ActionBarOutput::Login => self.handle_login_pressed(sender),
            ActionBarOutput::Launch => self.handle_launch_pressed(sender),
            ActionBarOutput::Refresh => self.refresh_from_disk(),
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

    fn handle_login_pressed(&mut self, sender: ComponentSender<Self>) {
        if let Some(session) = self.login_session.take() {
            tracing::info!("login wait cancelled from UI");
            session.cancel.cancel();
            self.status.headline = tr!("login.cancelled");
            self.refresh_details();
            return;
        }

        if self.paths.game_token().exists() {
            self.handle_logout();
            return;
        }

        self.begin_login(sender);
    }

    fn handle_logout(&mut self) {
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
