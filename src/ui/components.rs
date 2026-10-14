use super::{app::ProgressState, status::StatusSnapshot};
use hearthstone_linux::{
    config::{AppConfig, Locale, Region},
    host_env, i18n,
    install::launcher,
    tr,
};
use relm4::adw::prelude::*;
use relm4::{adw, gtk, prelude::*, ComponentController, Controller, RelmWidgetExt, Sender};
use relm4_components::simple_combo_box::{SimpleComboBox, SimpleComboBoxMsg};

#[derive(Clone, Debug)]
pub struct StatusPanelState {
    pub snapshot: StatusSnapshot,
    pub progress: ProgressState,
    /// Login page URL, shown while a browser login is pending so it can be
    /// copied when no browser opened (or the wrong one did).
    pub login_url: Option<String>,
}

pub struct StatusPanel {
    state: StatusPanelState,
}

#[derive(Debug)]
pub enum StatusPanelInput {
    Render(StatusPanelState),
    CopyLoginUrl,
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
                set_margin_top: 12,
                add_css_class: relm4::css::TITLE_2,

                #[watch]
                set_label: &model.state.snapshot.headline,
            },

            gtk::Label {
                set_margin_top: 6,
                set_margin_bottom: 6,
                add_css_class: relm4::css::DIM_LABEL,

                #[watch]
                set_label: &model.state.snapshot.details,
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 4,

                #[watch]
                set_visible: model.state.login_url.is_some(),

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
        let model = StatusPanel { state };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Self::Input, _sender: ComponentSender<Self>) {
        match input {
            StatusPanelInput::Render(state) => self.state = state,
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
    GameModeChanged(bool),
    MangoHudChanged(bool),
    /// Look again for the optional launch tools on the host.
    DetectTools,
}

#[derive(Clone, Copy, Debug)]
pub enum SettingsOutput {
    RegionChanged(Region),
    LocaleChanged(Locale),
    DiscreteGpuChanged(bool),
    GameModeChanged(bool),
    MangoHudChanged(bool),
}

#[relm4::component(pub)]
impl SimpleComponent for SettingsPanel {
    type Init = AppConfig;
    type Input = SettingsPanelInput;
    type Output = SettingsOutput;

    view! {
        gtk::Grid {
            set_halign: gtk::Align::Center,
            set_column_spacing: 12,
            set_row_spacing: 8,

            attach[0, 0, 1, 1] = &gtk::Label {
                set_label: &tr!("settings.region"),
                set_xalign: 1.0,
                add_css_class: relm4::css::DIM_LABEL,
            },

            attach[1, 0, 1, 1] = &gtk::Box {
                #[local_ref]
                region -> gtk::ComboBoxText {},
            },

            attach[0, 1, 1, 1] = &gtk::Label {
                set_label: &tr!("settings.game_language"),
                set_xalign: 1.0,
                add_css_class: relm4::css::DIM_LABEL,
            },

            attach[1, 1, 1, 1] = &gtk::Box {
                #[local_ref]
                locale -> gtk::ComboBoxText {},
            },

            attach[0, 2, 1, 1] = &gtk::Label {
                set_label: &tr!("settings.graphics"),
                set_xalign: 1.0,
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
                set_xalign: 1.0,
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
                        launcher::GAMEMODE,
                        model.gamemode_available,
                        tr!("settings.gamemode_tooltip"),
                    )),
                    #[watch]
                    set_active: model.config.use_gamemode,

                    connect_toggled[sender] => move |button| {
                        sender.input(SettingsPanelInput::GameModeChanged(button.is_active()));
                    },
                },

                gtk::CheckButton {
                    set_label: Some("MangoHud"),

                    #[watch]
                    set_sensitive: model.mangohud_available,
                    #[watch]
                    set_tooltip_text: Some(&tool_tooltip(
                        launcher::MANGOHUD,
                        model.mangohud_available,
                        tr!("settings.mangohud_tooltip"),
                    )),
                    #[watch]
                    set_active: model.config.use_mangohud,

                    connect_toggled[sender] => move |button| {
                        sender.input(SettingsPanelInput::MangoHudChanged(button.is_active()));
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
            gamemode_available: launcher::host_tool_available(launcher::GAMEMODE),
            mangohud_available: launcher::host_tool_available(launcher::MANGOHUD),
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
            SettingsPanelInput::GameModeChanged(use_gamemode) => {
                if self.config.use_gamemode != use_gamemode {
                    self.config.use_gamemode = use_gamemode;
                    sender
                        .output(SettingsOutput::GameModeChanged(use_gamemode))
                        .ok();
                }
            }
            SettingsPanelInput::MangoHudChanged(use_mangohud) => {
                if self.config.use_mangohud != use_mangohud {
                    self.config.use_mangohud = use_mangohud;
                    sender
                        .output(SettingsOutput::MangoHudChanged(use_mangohud))
                        .ok();
                }
            }
            SettingsPanelInput::DetectTools => {
                self.gamemode_available = launcher::host_tool_available(launcher::GAMEMODE);
                self.mangohud_available = launcher::host_tool_available(launcher::MANGOHUD);
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
fn tool_tooltip(tool: &str, available: bool, description: String) -> String {
    if available {
        description
    } else if host_env::is_flatpak() {
        tr!("settings.tool_flatpak", tool = tool)
    } else {
        tr!("settings.tool_missing", tool = tool)
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
    login_popover: Option<gtk::Popover>,
    uninstall_button: Option<gtk::Button>,
    switch_account_button: Option<gtk::Button>,
}

#[derive(Debug)]
pub enum ActionBarInput {
    Render(ActionBarState),
}

#[derive(Clone, Copy, Debug)]
pub enum ActionBarOutput {
    Install,
    Uninstall,
    Login,
    Launch,
    Refresh,
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
            set_halign: gtk::Align::Center,
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

            #[name(login_button)]
            adw::SplitButton {
                set_tooltip_text: Some(&tr!("action.account_tooltip")),
                #[watch]
                set_label: &model.login_label(),
                #[watch]
                set_class_active: (relm4::css::DESTRUCTIVE_ACTION, model.state.login == LoginState::Waiting),
                connect_clicked[sender] => move |_| {
                    sender.output(ActionBarOutput::Login).ok();
                },
            },

            gtk::Button {
                #[watch]
                set_label: &model.launch_label(),
                #[watch]
                set_sensitive: model.state.launch != LaunchState::Stopping,
                #[watch]
                set_class_active: (relm4::css::SUGGESTED_ACTION, model.state.launch == LaunchState::Idle),
                #[watch]
                set_class_active: (relm4::css::DESTRUCTIVE_ACTION, model.state.launch != LaunchState::Idle),
                connect_clicked[sender] => move |_| {
                    sender.output(ActionBarOutput::Launch).ok();
                },
            },

            gtk::Button {
                set_label: &tr!("action.refresh"),
                connect_clicked[sender] => move |_| {
                    sender.output(ActionBarOutput::Refresh).ok();
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
            login_popover: None,
            uninstall_button: None,
            switch_account_button: None,
        };
        let widgets = view_output!();
        let install_menu = install_popover(sender.clone());
        widgets
            .install_button
            .set_popover(Some(&install_menu.popover));
        model.uninstall_button = Some(install_menu.button);
        model.install_popover = Some(install_menu.popover);

        let login_menu = login_popover(sender.clone());
        widgets.login_button.set_popover(Some(&login_menu.popover));
        model.switch_account_button = Some(login_menu.button);
        model.login_popover = Some(login_menu.popover);
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
        if let Some(popover) = self.login_popover.take() {
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
        if let Some(button) = self.switch_account_button.as_ref() {
            button.set_sensitive(self.state.login == LoginState::LoggedIn);
        }
    }

    fn login_label(&self) -> String {
        match self.state.login {
            LoginState::Idle => tr!("action.login"),
            LoginState::Waiting => tr!("action.cancel_login"),
            LoginState::LoggedIn => tr!("action.log_out"),
        }
    }

    fn launch_label(&self) -> String {
        match self.state.launch {
            LaunchState::Idle => tr!("action.play"),
            LaunchState::Running => tr!("action.stop"),
            LaunchState::Stopping => tr!("action.stopping"),
        }
    }
}

fn index_of<T: Copy + Eq>(items: &[T], value: T) -> Option<usize> {
    items.iter().position(|item| *item == value)
}

/// Gear menu with launcher-level preferences: the UI language, each with its
/// flag. `current` is the saved choice, `None` meaning the system language.
pub fn preferences_menu(
    current: Option<&str>,
    on_language: impl Fn(Option<&'static str>) + 'static,
) -> gtk::MenuButton {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    content.set_margin_start(10);
    content.set_margin_end(10);

    let title = gtk::Label::new(Some(&tr!("settings.language")));
    title.add_css_class(relm4::css::HEADING);
    content.append(&title);

    let system_icon = gtk::Image::from_icon_name("preferences-desktop-locale-symbolic");
    let system = (None, system_icon, tr!("settings.language_system"));
    let languages = i18n::available().into_iter().map(|language| {
        (
            Some(language.code),
            flag_image(language.flag),
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

    let popover = gtk::Popover::new();
    popover.set_child(Some(&content));

    let button = gtk::MenuButton::new();
    button.set_icon_name("emblem-system-symbolic");
    button.set_tooltip_text(Some(&tr!("settings.language")));
    button.set_direction(gtk::ArrowType::Up);
    button.set_valign(gtk::Align::End);
    button.add_css_class(relm4::css::FLAT);
    button.set_popover(Some(&popover));
    button
}

/// Decodes one of the embedded flag PNGs. PNG support is built into
/// gdk-pixbuf, so this does not depend on an installed image loader.
fn flag_image(png: &'static [u8]) -> gtk::Image {
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

fn login_popover(sender: ComponentSender<ActionBar>) -> PopoverButton {
    let popover = gtk::Popover::new();

    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    content.set_margin_start(10);
    content.set_margin_end(10);

    let switch_account = gtk::Button::with_label(&tr!("action.switch_account"));
    {
        let popover = popover.clone();
        let output = sender.output_sender().clone();
        switch_account.connect_clicked(move |_| {
            popover.popdown();
            output.emit(ActionBarOutput::SwitchAccount);
        });
    }

    content.append(&switch_account);
    popover.set_child(Some(&content));
    PopoverButton {
        popover,
        button: switch_account,
    }
}
