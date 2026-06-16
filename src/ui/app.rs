use super::{
    auth, browser,
    components::{
        ActionBar, ActionBarInput, ActionBarOutput, ActionBarState, InstallState, LaunchState,
        LoginState, SettingsOutput, SettingsPanel, SettingsPanelInput, StatusPanel,
        StatusPanelInput, StatusPanelState,
    },
    status,
};
use hearthstone_linux::{
    config::AppConfig,
    install::{
        launcher,
        manager::{InstallManager, TaskEvent},
    },
    paths::AppPaths,
};
use relm4::adw::prelude::*;
use relm4::{
    abstractions::Toaster, adw, gtk, gtk::glib, prelude::*, ComponentController, Controller,
    RelmApp,
};
use std::{process::Child, time::Duration};
use tokio_util::sync::CancellationToken;

const PROJECT_URL: &str = "https://github.com/DawnMagnet/hearthstone-linux-gui";
const COPYRIGHT_TEXT: &str = "Copyright (c) 2025 DawnMagnet";

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
    game_session: Option<GameSession>,
    status_panel: Controller<StatusPanel>,
    settings_panel: Controller<SettingsPanel>,
    actions: Controller<ActionBar>,
    toaster: Toaster,
}

#[derive(Clone, Debug)]
pub enum AppMsg {
    Settings(SettingsOutput),
    Action(ActionBarOutput),
    InstallEvent(TaskEvent),
    LoginPoll,
    GamePoll,
}

struct LoginSession {
    cancel: CancellationToken,
}

struct GameSession {
    child: Child,
    poll_cancel: CancellationToken,
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
            set_title: Some("hearthstone-linux-gui"),
            set_default_size: (620, 360),

            #[local_ref]
            toast_overlay -> adw::ToastOverlay {
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,

                    adw::HeaderBar {
                        #[wrap(Some)]
                        set_title_widget = &adw::WindowTitle {
                            set_title: "hearthstone-linux-gui",
                        }
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 12,
                        set_margin_top: 18,
                        set_margin_bottom: 18,
                        set_margin_start: 18,
                        set_margin_end: 18,

                        #[local_ref]
                        status_panel -> gtk::Box {},

                        #[local_ref]
                        settings_panel -> gtk::Grid {},

                        #[local_ref]
                        actions -> gtk::Box {},

                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 2,
                            set_halign: gtk::Align::Center,
                            set_margin_top: 4,

                            gtk::Label {
                                add_css_class: relm4::css::DIM_LABEL,
                                set_label: COPYRIGHT_TEXT,
                            },

                            gtk::LinkButton {
                                set_label: "github.com/DawnMagnet/hearthstone-linux-gui",
                                set_uri: PROJECT_URL,
                                set_tooltip_text: Some(PROJECT_URL),
                            },
                        },
                    }
                }
            }
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        tracing::debug!("building Relm4 main window");

        let status_panel = StatusPanel::builder()
            .launch(StatusPanelState {
                snapshot: init.snapshot.clone(),
                progress: ProgressState::default(),
            })
            .detach();
        let settings_panel = SettingsPanel::builder()
            .launch(init.config.clone())
            .forward(sender.input_sender(), AppMsg::Settings);
        let actions = ActionBar::builder()
            .launch(ActionBarState::default())
            .forward(sender.input_sender(), AppMsg::Action);
        let toaster = Toaster::default();

        let model = MainWindow {
            paths: init.paths,
            config: init.config,
            status: init.snapshot,
            progress: ProgressState::default(),
            install_state: InstallState::Idle,
            install_cancel: None,
            login_session: None,
            game_session: None,
            status_panel,
            settings_panel,
            actions,
            toaster,
        };

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
            AppMsg::Action(output) => self.handle_action(output, sender),
            AppMsg::InstallEvent(event) => self.handle_install_event(event),
            AppMsg::LoginPoll => self.handle_login_poll(),
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
        }
        if let Err(error) = self.config.save(&self.paths.config_file) {
            tracing::error!(error = %format!("{error:#}"), "failed to save settings");
            self.status.headline = format!("Settings save failed: {error:#}");
        }
        self.refresh_details();
    }

    fn handle_action(&mut self, output: ActionBarOutput, sender: ComponentSender<Self>) {
        match output {
            ActionBarOutput::Install => self.handle_install_pressed(sender),
            ActionBarOutput::Login => self.handle_login_pressed(sender),
            ActionBarOutput::Launch => self.handle_launch_pressed(sender),
            ActionBarOutput::Refresh => self.refresh_from_disk(),
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
            "Update"
        } else {
            "Install"
        }
        .to_string();
        let cancel = CancellationToken::new();
        self.install_state = InstallState::Running(action.clone());
        self.install_cancel = Some(cancel.clone());
        self.progress = ProgressState {
            visible: true,
            fraction: Some(0.0),
            text: Some("0%".into()),
        };
        self.status.headline = "Preparing".into();

        tracing::info!(action = action, "install/update requested from UI");
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
                    TaskEvent::Cancelled("Installation cancelled".into())
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
            self.status.headline = "Stopping installation".into();
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
                self.toast("Ready to play");
            }
            TaskEvent::Failed(message) => {
                tracing::error!(error = %message, "install/update failed in UI");
                self.install_state = InstallState::Idle;
                self.install_cancel = None;
                self.progress.visible = false;
                self.status.headline = format!("Failed: {message}");
                self.refresh_details();
                self.toast("Install failed");
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
            self.status.headline = "Login cancelled".into();
            self.refresh_details();
            return;
        }

        if self.paths.game_token().exists() {
            self.actions.emit(ActionBarInput::ShowAccountMenu);
            return;
        }

        self.begin_login(sender);
    }

    fn handle_logout(&mut self) {
        match auth::mark_logged_out(&self.paths, &mut self.config) {
            Ok(()) => {
                auth::sync_config_from_disk(&self.paths, &mut self.config);
                self.refresh_status_with_headline("Logged out");
            }
            Err(error) => {
                tracing::error!(error = %format!("{error:#}"), "logout failed");
                self.status.headline = format!("Logout failed: {error:#}");
                self.refresh_details();
            }
        }
    }

    fn handle_switch_account(&mut self, sender: ComponentSender<Self>) {
        match auth::mark_logged_out(&self.paths, &mut self.config) {
            Ok(()) => self.begin_login(sender),
            Err(error) => {
                tracing::error!(error = %format!("{error:#}"), "failed to clear previous login");
                self.status.headline = format!("Switch account failed: {error:#}");
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
            self.status.headline = format!("Login setup failed: {error:#}");
            self.refresh_details();
            return;
        }

        let cancel = CancellationToken::new();
        self.login_session = Some(LoginSession {
            cancel: cancel.clone(),
        });

        if let Err(error) = auth::ensure_auth_scheme_handlers() {
            tracing::warn!(error = %format!("{error:#}"), "failed to register auth URI handlers");
            self.status.headline =
                "Login handler registration failed; continuing with browser login".into();
        } else {
            self.status.headline = "Complete login in browser; waiting for desktop callback".into();
        }
        self.refresh_details();

        let login_url = current.region.login_url().to_string();
        tracing::info!(
            region = %current.region,
            url = %login_url,
            "opening browser login with desktop callback handler"
        );
        if let Err(error) = browser::open_login_url(&login_url) {
            tracing::error!(
                url = login_url,
                error = %format!("{error:#}"),
                "failed to open browser login"
            );
            self.status.headline = format!("Could not open browser. URL: {login_url}");
        }

        Self::start_poll(&sender, AppMsg::LoginPoll, cancel);
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
            self.refresh_status_with_headline("Login complete");
            self.toast("Login complete");
        }
    }

    fn handle_launch_pressed(&mut self, sender: ComponentSender<Self>) {
        if let Some(session) = self.game_session.as_mut() {
            tracing::info!("game stop requested from UI");
            match session.child.kill() {
                Ok(()) => {
                    self.status.headline = "Stopping game".into();
                    self.refresh_details();
                }
                Err(error) => {
                    tracing::error!(error = %error, "failed to stop game");
                    self.status.headline = format!("Failed to stop game: {error}");
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
        match launcher::launch_game(&game_dir, self.config.use_discrete_gpu) {
            Ok(child) => {
                let poll_cancel = CancellationToken::new();
                self.game_session = Some(GameSession {
                    child,
                    poll_cancel: poll_cancel.clone(),
                });
                self.status.headline = "Game running".into();
                self.refresh_details();

                Self::start_poll(&sender, AppMsg::GamePoll, poll_cancel);
            }
            Err(error) => {
                tracing::error!(error = %format!("{error:#}"), "launch failed");
                self.status.headline = format!("Launch failed: {error:#}");
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
                tracing::info!(status = %exit, "game exited");
                session.poll_cancel.cancel();
                self.game_session = None;
                self.status.headline = "Game stopped".into();
                self.refresh_details();
            }
            Ok(None) => {}
            Err(error) => {
                tracing::error!(error = %error, "failed to poll game process");
                session.poll_cancel.cancel();
                self.game_session = None;
                self.status.headline = format!("Game status error: {error}");
                self.refresh_details();
            }
        }
    }

    fn refresh_from_disk(&mut self) {
        tracing::debug!("refresh requested from UI");
        let (config, snapshot) = status::reconcile(&self.paths);
        self.config = config;
        self.status = snapshot;
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
            }));
        self.settings_panel
            .emit(SettingsPanelInput::SetConfig(self.config.clone()));
        self.actions.emit(ActionBarInput::Render(ActionBarState {
            install: self.install_state.clone(),
            login: self.login_state(),
            launch: self.launch_state(),
        }));
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
        match self.status.headline.as_str() {
            "Stopping game" if self.game_session.is_some() => LaunchState::Stopping,
            _ if self.game_session.is_some() => LaunchState::Running,
            _ => LaunchState::Idle,
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
