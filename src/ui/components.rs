use super::{app::ProgressState, status::StatusSnapshot};
use hearthstone_linux::{
    config::{AppConfig, Locale, Region},
    host_env, i18n,
    install::launcher::LaunchTool,
    tr, update,
};
use relm4::adw::prelude::*;
use relm4::{adw, gtk, prelude::*, ComponentController, Controller, RelmWidgetExt, Sender};
use relm4_components::simple_combo_box::{SimpleComboBox, SimpleComboBoxMsg};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct StatusPanelState {
    pub snapshot: StatusSnapshot,
    pub progress: ProgressState,
    /// Login page URL, shown while a browser login is pending so it can be
    /// copied when no browser opened (or the wrong one did).
    pub login_url: Option<String>,
}

/// How long a status message stays up at least while a task runs. Steps
/// that finish sooner are skipped and the panel moves on to the newest one,
/// so a quick update check does not flash a dozen messages.
const MIN_MESSAGE_TIME: Duration = Duration::from_millis(600);

pub struct StatusPanel {
    state: StatusPanelState,
    root: gtk::Box,
    /// When the shown headline appeared.
    shown_at: Instant,
    /// Newest state waiting for the shown message's minimum time.
    pending: Option<StatusPanelState>,
}

impl StatusPanel {
    fn show(&mut self, state: StatusPanelState) {
        if state.snapshot.headline != self.state.snapshot.headline {
            self.shown_at = Instant::now();
        }
        let task_ended = self.state.progress.visible && !state.progress.visible;
        self.state = state;
        if task_ended {
            self.shrink_window();
        }
    }

    /// A window grows with its content but never shrinks on its own; once
    /// a task's lines and progress bar are gone, return to the smallest
    /// height (keeping the width the user chose).
    fn shrink_window(&self) {
        let root = self.root.clone();
        // After the view update, so the window measures the smaller layout.
        gtk::glib::idle_add_local_once(move || {
            let Some(window) = root.root().and_downcast::<gtk::Window>() else {
                return;
            };
            if !window.is_maximized() && !window.is_fullscreen() {
                window.set_default_size(window.width(), 1);
            }
        });
    }
}

#[derive(Debug)]
pub enum StatusPanelInput {
    Render(StatusPanelState),
    ShowPending,
    CopyLoginUrl,
}

/// Lines the headline keeps while an install or uninstall runs (action,
/// counts, speed). Shorter steps are padded, so everything below stays put
/// from the start of the task until it ends.
const RUNNING_HEADLINE_LINES: usize = 3;

fn headline_text(state: &StatusPanelState) -> String {
    let headline = &state.snapshot.headline;
    if !state.progress.visible {
        return headline.clone();
    }
    let padding = RUNNING_HEADLINE_LINES.saturating_sub(headline.lines().count().max(1));
    format!("{headline}{}", "\n".repeat(padding))
}

fn tabular_digits() -> gtk::pango::AttrList {
    let attributes = gtk::pango::AttrList::new();
    attributes.insert(gtk::pango::AttrFontFeatures::new("tnum 1"));
    attributes
}

#[relm4::component(pub)]
impl SimpleComponent for StatusPanel {
    type Init = StatusPanelState;
    type Input = StatusPanelInput;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 6,

            gtk::Label {
                set_justify: gtk::Justification::Center,
                set_wrap: true,
                set_max_width_chars: 48,
                add_css_class: relm4::css::TITLE_2,
                // Equal-width digits keep progress numbers from shifting the
                // text while they count up.
                set_attributes: Some(&tabular_digits()),

                #[watch]
                set_label: &headline_text(&model.state),
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 4,

                #[watch]
                set_visible: model.state.login_url.is_some(),

                // Blizzard serves the Hearthstone login page for the game's
                // built-in browser, with white text on a transparent
                // background; in a regular browser that reads as blank.
                gtk::Label {
                    set_justify: gtk::Justification::Center,
                    set_wrap: true,
                    set_margin_bottom: 6,
                    set_label: &tr!("login.page_hint"),
                },

                gtk::Label {
                    set_justify: gtk::Justification::Center,
                    set_wrap: true,
                    add_css_class: relm4::css::DIM_LABEL,
                    set_label: &tr!("login.url_hint"),
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 6,

                    gtk::Entry {
                        set_hexpand: true,
                        set_editable: false,
                        add_css_class: relm4::css::MONOSPACE,

                        #[watch]
                        set_text: model.state.login_url.as_deref().unwrap_or_default(),
                    },

                    gtk::Button {
                        set_label: &tr!("login.copy"),
                        set_tooltip_text: Some(&tr!("login.copy_tooltip")),
                        connect_clicked => StatusPanelInput::CopyLoginUrl,
                    },
                },
            },

            gtk::ProgressBar {
                set_show_text: true,

                #[watch]
                set_visible: model.state.progress.visible,

                #[watch]
                set_fraction: model.state.progress.fraction.unwrap_or_default(),

                #[watch]
                set_text: model.state.progress.text.as_deref(),
            },
        }
    }

    fn init(
        state: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = StatusPanel {
            state,
            root: root.clone(),
            shown_at: Instant::now(),
            pending: None,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Self::Input, sender: ComponentSender<Self>) {
        match input {
            StatusPanelInput::Render(state) => {
                let task_message = state.snapshot.headline != self.state.snapshot.headline
                    && (state.progress.visible || self.state.progress.visible);
                let wait = MIN_MESSAGE_TIME.saturating_sub(self.shown_at.elapsed());
                if self.pending.is_some() {
                    // A timer is already running; it shows the newest state.
                    self.pending = Some(state);
                } else if task_message && !wait.is_zero() {
                    self.pending = Some(state);
                    gtk::glib::timeout_add_local_once(wait, move || {
                        sender.input(StatusPanelInput::ShowPending);
                    });
                } else {
                    self.show(state);
                }
            }
            StatusPanelInput::ShowPending => {
                if let Some(state) = self.pending.take() {
                    self.show(state);
                }
            }
            StatusPanelInput::CopyLoginUrl => {
                if let (Some(url), Some(display)) = (
                    self.state.login_url.as_deref(),
                    gtk::gdk::Display::default(),
                ) {
                    display.clipboard().set_text(url);
                }
            }
        }
    }
}

pub struct SettingsPanel {
    config: AppConfig,
    gamemode_available: bool,
    mangohud_available: bool,
    region: Controller<SimpleComboBox<Region>>,
    locale: Controller<SimpleComboBox<Locale>>,
}

#[derive(Debug)]
pub enum SettingsPanelInput {
    SetConfig(AppConfig),
    RegionChanged(usize),
    LocaleChanged(usize),
    DiscreteGpuChanged(bool),
    LaunchToolChanged(LaunchTool, bool),
    /// Look again for the optional launch tools on the host.
    DetectTools,
}

#[derive(Clone, Copy, Debug)]
pub enum SettingsOutput {
    RegionChanged(Region),
    LocaleChanged(Locale),
    DiscreteGpuChanged(bool),
    LaunchToolChanged(LaunchTool, bool),
}

#[relm4::component(pub)]
impl SimpleComponent for SettingsPanel {
    type Init = AppConfig;
    type Input = SettingsPanelInput;
    type Output = SettingsOutput;

    view! {
        gtk::Grid {
            // Flush with the first action button below it.
            set_halign: gtk::Align::Start,
            set_column_spacing: 12,
            set_row_spacing: 8,

            attach[0, 0, 1, 1] = &gtk::Label {
                set_label: &tr!("settings.region"),
                set_xalign: 0.0,
                add_css_class: relm4::css::DIM_LABEL,
            },

            attach[1, 0, 1, 1] = &gtk::Box {
                #[local_ref]
                region -> gtk::ComboBoxText {},
            },

            attach[0, 1, 1, 1] = &gtk::Label {
                set_label: &tr!("settings.game_language"),
                set_xalign: 0.0,
                add_css_class: relm4::css::DIM_LABEL,
            },

            attach[1, 1, 1, 1] = &gtk::Box {
                #[local_ref]
                locale -> gtk::ComboBoxText {},
            },

            attach[0, 2, 1, 1] = &gtk::Label {
                set_label: &tr!("settings.graphics"),
                set_xalign: 0.0,
                add_css_class: relm4::css::DIM_LABEL,
            },

            attach[1, 2, 1, 1] = &gtk::CheckButton {
                set_label: Some(&tr!("settings.discrete_gpu")),
                set_tooltip_text: Some(&tr!("settings.discrete_gpu_tooltip")),

                #[watch]
                set_active: model.config.use_discrete_gpu,

                connect_toggled[sender] => move |button| {
                    sender.input(SettingsPanelInput::DiscreteGpuChanged(button.is_active()));
                },
            },

            attach[0, 3, 1, 1] = &gtk::Label {
                set_label: &tr!("settings.performance"),
                set_xalign: 0.0,
                add_css_class: relm4::css::DIM_LABEL,
            },

            attach[1, 3, 1, 1] = &gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 12,

                gtk::CheckButton {
                    set_label: Some("GameMode"),

                    #[watch]
                    set_sensitive: model.gamemode_available,
                    #[watch]
                    set_tooltip_text: Some(&tool_tooltip(
                        LaunchTool::GameMode,
                        model.gamemode_available,
                        tr!("settings.gamemode_tooltip"),
                    )),
                    // A tool that has gone missing shows as off, but the saved
                    // choice is kept for when it is installed again.
                    #[watch]
                    set_active: model.config.use_gamemode && model.gamemode_available,

                    connect_toggled[sender] => move |button| {
                        sender.input(SettingsPanelInput::LaunchToolChanged(
                            LaunchTool::GameMode,
                            button.is_active(),
                        ));
                    },
                },

                gtk::CheckButton {
                    set_label: Some("MangoHud"),

                    #[watch]
                    set_sensitive: model.mangohud_available,
                    #[watch]
                    set_tooltip_text: Some(&tool_tooltip(
                        LaunchTool::MangoHud,
                        model.mangohud_available,
                        tr!("settings.mangohud_tooltip"),
                    )),
                    #[watch]
                    set_active: model.config.use_mangohud && model.mangohud_available,

                    connect_toggled[sender] => move |button| {
                        sender.input(SettingsPanelInput::LaunchToolChanged(
                            LaunchTool::MangoHud,
                            button.is_active(),
                        ));
                    },
                },
            },
        }
    }

    fn init(
        config: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let region = SimpleComboBox::builder()
            .launch(SimpleComboBox {
                variants: Region::ALL.to_vec(),
                active_index: index_of(&Region::ALL, config.region),
            })
            .forward(sender.input_sender(), SettingsPanelInput::RegionChanged);
        let locale = SimpleComboBox::builder()
            .launch(SimpleComboBox {
                variants: Locale::ALL.to_vec(),
                active_index: index_of(&Locale::ALL, config.locale),
            })
            .forward(sender.input_sender(), SettingsPanelInput::LocaleChanged);
        let model = SettingsPanel {
            config,
            gamemode_available: LaunchTool::GameMode.is_installed(),
            mangohud_available: LaunchTool::MangoHud.is_installed(),
            region,
            locale,
        };

        let region = model.region.widget();
        let locale = model.locale.widget();
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Self::Input, sender: ComponentSender<Self>) {
        match input {
            SettingsPanelInput::SetConfig(config) => {
                let region_changed = self.config.region != config.region;
                let locale_changed = self.config.locale != config.locale;
                self.config = config;
                if region_changed {
                    self.region
                        .emit(SimpleComboBoxMsg::SetActiveIdx(self.region_index()));
                }
                if locale_changed {
                    self.locale
                        .emit(SimpleComboBoxMsg::SetActiveIdx(self.locale_index()));
                }
            }
            SettingsPanelInput::RegionChanged(idx) => {
                if let Some(region) = Region::ALL.get(idx).copied() {
                    self.config.region = region;
                    sender.output(SettingsOutput::RegionChanged(region)).ok();
                }
            }
            SettingsPanelInput::LocaleChanged(idx) => {
                if let Some(locale) = Locale::ALL.get(idx).copied() {
                    self.config.locale = locale;
                    sender.output(SettingsOutput::LocaleChanged(locale)).ok();
                }
            }
            SettingsPanelInput::LaunchToolChanged(tool, enabled) => {
                let (current, available) = match tool {
                    LaunchTool::GameMode => {
                        (&mut self.config.use_gamemode, self.gamemode_available)
                    }
                    LaunchTool::MangoHud => {
                        (&mut self.config.use_mangohud, self.mangohud_available)
                    }
                };
                // A switch shown as off because its tool is missing reports
                // itself too; that must not clear the saved choice.
                if available && *current != enabled {
                    *current = enabled;
                    sender
                        .output(SettingsOutput::LaunchToolChanged(tool, enabled))
                        .ok();
                }
            }
            SettingsPanelInput::DetectTools => {
                self.gamemode_available = LaunchTool::GameMode.is_installed();
                self.mangohud_available = LaunchTool::MangoHud.is_installed();
            }
            SettingsPanelInput::DiscreteGpuChanged(use_discrete_gpu) => {
                if self.config.use_discrete_gpu != use_discrete_gpu {
                    self.config.use_discrete_gpu = use_discrete_gpu;
                    sender
                        .output(SettingsOutput::DiscreteGpuChanged(use_discrete_gpu))
                        .ok();
                }
            }
        }
    }
}

/// Tooltip of a launch tool switch: what it does, or why it is disabled.
fn tool_tooltip(tool: LaunchTool, available: bool, description: String) -> String {
    if available {
        description
    } else if host_env::is_flatpak() {
        tr!("settings.tool_flatpak", tool = tool.program())
    } else {
        tr!("settings.tool_missing", tool = tool.program())
    }
}

impl SettingsPanel {
    fn region_index(&self) -> usize {
        index_of(&Region::ALL, self.config.region).unwrap_or(0)
    }

    fn locale_index(&self) -> usize {
        index_of(&Locale::ALL, self.config.locale).unwrap_or(0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallAction {
    Install,
    Update,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstallState {
    Idle,
    Running(InstallAction),
    Stopping,
    Uninstalling,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoginState {
    Idle,
    Waiting,
    LoggedIn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchState {
    Idle,
    Running,
    Stopping,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionBarState {
    pub install: InstallState,
    pub installed: bool,
    pub login: LoginState,
    pub launch: LaunchState,
}

impl Default for ActionBarState {
    fn default() -> Self {
        Self {
            install: InstallState::Idle,
            installed: false,
            login: LoginState::Idle,
            launch: LaunchState::Idle,
        }
    }
}

pub struct ActionBar {
    state: ActionBarState,
    install_popover: Option<gtk::Popover>,
    account_popover: Option<gtk::Popover>,
    uninstall_button: Option<gtk::Button>,
    account_menu_buttons: Vec<gtk::Button>,
}

#[derive(Debug)]
pub enum ActionBarInput {
    Render(ActionBarState),
}

#[derive(Clone, Copy, Debug)]
pub enum ActionBarOutput {
    Install,
    Uninstall,
    /// The combined account button: log in, play or stop, by state.
    Account,
    Logout,
    SwitchAccount,
}

#[relm4::component(pub)]
impl SimpleComponent for ActionBar {
    type Init = ActionBarState;
    type Input = ActionBarInput;
    type Output = ActionBarOutput;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_halign: gtk::Align::Start,
            set_spacing: 8,
            set_margin_top: 10,

            #[name(install_button)]
            adw::SplitButton {
                set_tooltip_text: Some(&tr!("action.install_tooltip")),
                #[watch]
                set_label: &model.install_label(),
                #[watch]
                set_sensitive: model.install_button_sensitive(),
                #[watch]
                set_class_active: (relm4::css::SUGGESTED_ACTION, model.state.install == InstallState::Idle),
                #[watch]
                set_class_active: (relm4::css::DESTRUCTIVE_ACTION, model.state.install != InstallState::Idle),
                connect_clicked[sender] => move |_| {
                    sender.output(ActionBarOutput::Install).ok();
                },
            },

            // One button for the account: "Login" while logged out, "Play"
            // once logged in. Log out and switch account are in its menu.
            #[name(account_button)]
            adw::SplitButton {
                #[watch]
                set_tooltip_text: Some(&model.account_tooltip()),
                #[watch]
                set_label: &model.account_label(),
                #[watch]
                set_sensitive: model.account_button_sensitive(),
                #[watch]
                set_class_active: (relm4::css::SUGGESTED_ACTION, model.can_play()),
                #[watch]
                set_class_active: (relm4::css::DESTRUCTIVE_ACTION, model.account_action_stops()),
                connect_clicked[sender] => move |_| {
                    sender.output(ActionBarOutput::Account).ok();
                },
            },
        }
    }

    fn init(
        state: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut model = ActionBar {
            state,
            install_popover: None,
            account_popover: None,
            uninstall_button: None,
            account_menu_buttons: Vec::new(),
        };
        let widgets = view_output!();
        let install_menu = install_popover(sender.clone());
        widgets
            .install_button
            .set_popover(Some(&install_menu.popover));
        model.uninstall_button = Some(install_menu.button);
        model.install_popover = Some(install_menu.popover);

        let (popover, buttons) = account_popover(sender.clone());
        widgets.account_button.set_popover(Some(&popover));
        model.account_menu_buttons = buttons;
        model.account_popover = Some(popover);
        model.render_menu_items();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Self::Input, _sender: ComponentSender<Self>) {
        match input {
            ActionBarInput::Render(state) => {
                self.state = state;
                self.render_menu_items();
            }
        }
    }

    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: Sender<Self::Output>) {
        if let Some(popover) = self.install_popover.take() {
            popover.popdown();
            popover.unparent();
        }
        if let Some(popover) = self.account_popover.take() {
            popover.popdown();
            popover.unparent();
        }
    }
}

impl ActionBar {
    fn install_label(&self) -> String {
        match &self.state.install {
            InstallState::Idle => tr!("action.install_update"),
            InstallState::Running(InstallAction::Install) => tr!("action.stop_install"),
            InstallState::Running(InstallAction::Update) => tr!("action.stop_update"),
            InstallState::Stopping => tr!("action.stopping"),
            InstallState::Uninstalling => tr!("action.uninstalling"),
        }
    }

    fn install_button_sensitive(&self) -> bool {
        matches!(
            &self.state.install,
            InstallState::Idle | InstallState::Running(_)
        )
    }

    fn install_menu_sensitive(&self) -> bool {
        self.state.installed
            && self.state.install == InstallState::Idle
            && self.state.launch == LaunchState::Idle
    }

    fn render_menu_items(&self) {
        if let Some(button) = self.uninstall_button.as_ref() {
            button.set_sensitive(self.install_menu_sensitive());
        }
        let account_menu_sensitive =
            self.state.login == LoginState::LoggedIn && self.state.launch == LaunchState::Idle;
        for button in &self.account_menu_buttons {
            button.set_sensitive(account_menu_sensitive);
        }
    }

    fn account_label(&self) -> String {
        match (self.state.launch, self.state.login) {
            (LaunchState::Running, _) => tr!("action.stop"),
            (LaunchState::Stopping, _) => tr!("action.stopping"),
            (LaunchState::Idle, LoginState::Waiting) => tr!("action.cancel_login"),
            (LaunchState::Idle, LoginState::LoggedIn) => tr!("action.play"),
            (LaunchState::Idle, LoginState::Idle) => tr!("action.login"),
        }
    }

    /// Logged in, nothing running: the button offers "Play".
    fn offers_play(&self) -> bool {
        self.state.launch == LaunchState::Idle && self.state.login == LoginState::LoggedIn
    }

    fn can_play(&self) -> bool {
        self.offers_play() && self.state.installed
    }

    fn account_action_stops(&self) -> bool {
        self.state.launch != LaunchState::Idle || self.state.login == LoginState::Waiting
    }

    /// Only the stopping phase disables the button. "Play" without an
    /// installed game stays enabled so that the menu next to it (log out,
    /// switch account) remains reachable; pressing it says to install first.
    fn account_button_sensitive(&self) -> bool {
        self.state.launch != LaunchState::Stopping
    }

    fn account_tooltip(&self) -> String {
        if self.offers_play() && !self.state.installed {
            tr!("action.play_not_installed")
        } else {
            tr!("action.account_tooltip")
        }
    }
}

fn index_of<T: Copy + Eq>(items: &[T], value: T) -> Option<usize> {
    items.iter().position(|item| *item == value)
}

/// Gear menu: the game settings dialog, the UI language (each language with
/// its flag) and the update check. `current` is the saved language, `None` meaning the system
/// language.
pub fn preferences_menu(
    current: Option<&str>,
    check_for_updates: bool,
    on_game_settings: impl Fn() + 'static,
    on_system_info: impl Fn() + 'static,
    on_language: impl Fn(Option<&'static str>) + 'static,
    on_check_for_updates: impl Fn(bool) + 'static,
) -> PreferencesMenu {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    content.set_margin_start(10);
    content.set_margin_end(10);

    let popover = gtk::Popover::new();
    let game_settings = gtk::Button::with_label(&tr!("game_settings.menu"));
    {
        let popover = popover.clone();
        game_settings.connect_clicked(move |_| {
            popover.popdown();
            on_game_settings();
        });
    }
    content.append(&game_settings);
    let system_info = gtk::Button::with_label(&tr!("sysinfo.menu"));
    {
        let popover = popover.clone();
        system_info.connect_clicked(move |_| {
            popover.popdown();
            on_system_info();
        });
    }
    content.append(&system_info);
    content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    let title = gtk::Label::new(Some(&tr!("settings.language")));
    title.add_css_class(relm4::css::HEADING);
    content.append(&title);

    let system_icon = gtk::Image::from_icon_name("preferences-desktop-locale-symbolic");
    let system = (None, system_icon, tr!("settings.language_system"));
    let languages = i18n::available().into_iter().map(|language| {
        (
            Some(language.code),
            embedded_image(language.flag),
            language.name,
        )
    });

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.add_css_class(relm4::css::BOXED_LIST);
    let mut codes = Vec::new();
    let mut checks = Vec::new();
    for (code, icon, name) in std::iter::once(system).chain(languages) {
        icon.set_pixel_size(24);
        let label = gtk::Label::new(Some(&name));
        label.set_xalign(0.0);
        label.set_hexpand(true);
        let check = gtk::Image::from_icon_name("object-select-symbolic");
        check.set_opacity(if code == current { 1.0 } else { 0.0 });

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.set_margin_top(8);
        row.set_margin_bottom(8);
        row.set_margin_start(10);
        row.set_margin_end(10);
        row.append(&icon);
        row.append(&label);
        row.append(&check);
        list.append(&row);
        codes.push(code);
        checks.push(check);
    }
    list.connect_row_activated(move |_, row| {
        let Some(code) = codes.get(row.index() as usize).copied() else {
            return;
        };
        for (check, candidate) in checks.iter().zip(&codes) {
            check.set_opacity(if *candidate == code { 1.0 } else { 0.0 });
        }
        on_language(code);
    });
    content.append(&list);

    let hint = gtk::Label::new(Some(&tr!("settings.language_tooltip")));
    hint.add_css_class(relm4::css::DIM_LABEL);
    hint.add_css_class(relm4::css::CAPTION);
    content.append(&hint);

    if update::enabled() {
        content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        // A separate, wrapping label: the built-in one cannot wrap on GTK 4.0.
        let updates = gtk::CheckButton::new();
        updates.set_valign(gtk::Align::Start);
        updates.set_active(check_for_updates);
        updates.connect_toggled(move |button| on_check_for_updates(button.is_active()));
        let updates_label = gtk::Label::new(Some(&tr!("settings.auto_update")));
        updates_label.set_wrap(true);
        updates_label.set_xalign(0.0);
        updates_label.set_max_width_chars(30);
        let label_click = gtk::GestureClick::new();
        {
            let updates = updates.clone();
            label_click
                .connect_released(move |_, _, _, _| updates.set_active(!updates.is_active()));
        }
        updates_label.add_controller(label_click);
        let updates_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        updates_row.append(&updates);
        updates_row.append(&updates_label);
        content.append(&updates_row);
    }

    popover.set_child(Some(&content));

    let button = gtk::MenuButton::new();
    button.set_icon_name("emblem-system-symbolic");
    button.set_tooltip_text(Some(&tr!("settings.menu")));
    button.add_css_class(relm4::css::FLAT);
    button.set_popover(Some(&popover));
    PreferencesMenu {
        button,
        game_settings,
    }
}

/// The gear button and the entries the main window keeps updating.
pub struct PreferencesMenu {
    pub button: gtk::MenuButton,
    pub game_settings: gtk::Button,
}

/// Decodes an embedded PNG. PNG support is built into gdk-pixbuf, so this
/// does not depend on an installed image loader.
pub fn embedded_image(png: &'static [u8]) -> gtk::Image {
    let loader = gtk::gdk_pixbuf::PixbufLoader::new();
    let decoded = loader.write(png).and_then(|()| loader.close());
    match (decoded, loader.pixbuf()) {
        (Ok(()), Some(pixbuf)) => {
            gtk::Image::from_paintable(Some(&gtk::gdk::Texture::for_pixbuf(&pixbuf)))
        }
        _ => gtk::Image::new(),
    }
}

struct PopoverButton {
    popover: gtk::Popover,
    button: gtk::Button,
}

fn install_popover(sender: ComponentSender<ActionBar>) -> PopoverButton {
    let popover = gtk::Popover::new();

    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    content.set_margin_start(10);
    content.set_margin_end(10);

    let uninstall = gtk::Button::with_label(&tr!("action.uninstall"));
    uninstall.add_css_class(relm4::css::DESTRUCTIVE_ACTION);
    {
        let popover = popover.clone();
        let output = sender.output_sender().clone();
        uninstall.connect_clicked(move |_| {
            popover.popdown();
            output.emit(ActionBarOutput::Uninstall);
        });
    }

    content.append(&uninstall);
    popover.set_child(Some(&content));
    PopoverButton {
        popover,
        button: uninstall,
    }
}

fn account_popover(sender: ComponentSender<ActionBar>) -> (gtk::Popover, Vec<gtk::Button>) {
    let popover = gtk::Popover::new();

    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    content.set_margin_start(10);
    content.set_margin_end(10);

    let entries = [
        (tr!("action.log_out"), ActionBarOutput::Logout),
        (tr!("action.switch_account"), ActionBarOutput::SwitchAccount),
    ];
    let mut buttons = Vec::new();
    for (label, action) in entries {
        let button = gtk::Button::with_label(&label);
        let popover = popover.clone();
        let output = sender.output_sender().clone();
        button.connect_clicked(move |_| {
            popover.popdown();
            output.emit(action);
        });
        content.append(&button);
        buttons.push(button);
    }

    popover.set_child(Some(&content));
    (popover, buttons)
}
