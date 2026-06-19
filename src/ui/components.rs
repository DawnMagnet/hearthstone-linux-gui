use super::{app::ProgressState, status::StatusSnapshot};
use hearthstone_linux::config::{AppConfig, Locale, Region};
use relm4::adw::prelude::*;
use relm4::{adw, gtk, prelude::*, ComponentController, Controller, RelmWidgetExt, Sender};
use relm4_components::simple_combo_box::{SimpleComboBox, SimpleComboBoxMsg};

#[derive(Clone, Debug)]
pub struct StatusPanelState {
    pub snapshot: StatusSnapshot,
    pub progress: ProgressState,
}

pub struct StatusPanel {
    state: StatusPanelState,
}

#[derive(Debug)]
pub enum StatusPanelInput {
    Render(StatusPanelState),
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
                set_xalign: 0.0,
                add_css_class: relm4::css::TITLE_3,

                #[watch]
                set_label: &model.state.snapshot.headline,
            },

            gtk::Label {
                set_xalign: 0.0,
                add_css_class: relm4::css::DIM_LABEL,

                #[watch]
                set_label: &model.state.snapshot.details,
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
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = StatusPanel { state };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Self::Input, _sender: ComponentSender<Self>) {
        match input {
            StatusPanelInput::Render(state) => self.state = state,
        }
    }
}

pub struct SettingsPanel {
    config: AppConfig,
    region: Controller<SimpleComboBox<Region>>,
    locale: Controller<SimpleComboBox<Locale>>,
}

#[derive(Debug)]
pub enum SettingsPanelInput {
    SetConfig(AppConfig),
    RegionChanged(usize),
    LocaleChanged(usize),
    DiscreteGpuChanged(bool),
}

#[derive(Clone, Copy, Debug)]
pub enum SettingsOutput {
    RegionChanged(Region),
    LocaleChanged(Locale),
    DiscreteGpuChanged(bool),
}

#[relm4::component(pub)]
impl SimpleComponent for SettingsPanel {
    type Init = AppConfig;
    type Input = SettingsPanelInput;
    type Output = SettingsOutput;

    view! {
        gtk::Grid {
            set_column_spacing: 12,
            set_row_spacing: 8,

            attach[0, 0, 1, 1] = &gtk::Label {
                set_label: "Region",
                set_xalign: 0.0,
            },

            attach[1, 0, 1, 1] = &gtk::Box {
                #[local_ref]
                region -> gtk::ComboBoxText {},
            },

            attach[0, 1, 1, 1] = &gtk::Label {
                set_label: "Locale",
                set_xalign: 0.0,
            },

            attach[1, 1, 1, 1] = &gtk::Box {
                #[local_ref]
                locale -> gtk::ComboBoxText {},
            },

            attach[0, 2, 1, 1] = &gtk::Label {
                set_label: "Graphics",
                set_xalign: 0.0,
            },

            attach[1, 2, 1, 1] = &gtk::CheckButton {
                set_label: Some("Discrete GPU"),
                set_tooltip_text: Some("Launch with PRIME/DRI GPU offload environment variables"),

                #[watch]
                set_active: model.config.use_discrete_gpu,

                connect_toggled[sender] => move |button| {
                    sender.input(SettingsPanelInput::DiscreteGpuChanged(button.is_active()));
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

impl SettingsPanel {
    fn region_index(&self) -> usize {
        index_of(&Region::ALL, self.config.region).unwrap_or(0)
    }

    fn locale_index(&self) -> usize {
        index_of(&Locale::ALL, self.config.locale).unwrap_or(0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstallState {
    Idle,
    Running(String),
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
            set_spacing: 8,

            #[name(install_button)]
            adw::SplitButton {
                set_tooltip_text: Some("Install actions"),
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
                set_tooltip_text: Some("Account actions"),
                #[watch]
                set_label: model.login_label(),
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
                set_label: "Refresh",
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
            InstallState::Idle => "Install / Update".into(),
            InstallState::Running(action) => format!("Stop {action}"),
            InstallState::Stopping => "Stopping...".into(),
            InstallState::Uninstalling => "Uninstalling...".into(),
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

    fn login_label(&self) -> &'static str {
        match self.state.login {
            LoginState::Idle => "Login",
            LoginState::Waiting => "Cancel Login",
            LoginState::LoggedIn => "Log Out",
        }
    }

    fn launch_label(&self) -> &'static str {
        match self.state.launch {
            LaunchState::Idle => "Play",
            LaunchState::Running => "Stop",
            LaunchState::Stopping => "Stopping...",
        }
    }
}

fn index_of<T: Copy + Eq>(items: &[T], value: T) -> Option<usize> {
    items.iter().position(|item| *item == value)
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

    let uninstall = gtk::Button::with_label("Uninstall");
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

    let switch_account = gtk::Button::with_label("Switch Account");
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
