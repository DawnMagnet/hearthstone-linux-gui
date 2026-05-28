use crate::ui::{
    status::{set_install_idle, set_launch_idle, update_login_button},
    theme::{style_button, style_choice, ButtonRole, ColorScheme},
};
use fltk::{
    button::Button,
    enums::{Align, Font},
    frame::Frame,
    menu::Choice,
    misc::Progress,
    prelude::*,
    window::Window,
};
use hearthstone_linux::{
    config::{AppConfig, Locale, Region},
    paths::AppPaths,
};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone)]
pub(crate) struct UiWidgets {
    pub(crate) window: Window,
    pub(crate) content_bg: Frame,
    pub(crate) status: Frame,
    pub(crate) version: Frame,
    pub(crate) progress: Progress,
    pub(crate) region_label: Frame,
    pub(crate) locale_label: Frame,
    pub(crate) region: Choice,
    pub(crate) locale: Choice,
    pub(crate) install_button: Button,
    pub(crate) login_button: Button,
    pub(crate) launch_button: Button,
    pub(crate) refresh_button: Button,
}

pub(crate) fn build_window(paths: &AppPaths, config: &Rc<RefCell<AppConfig>>) -> UiWidgets {
    tracing::debug!("building main window");
    let mut window = Window::new(100, 100, 620, 360, "hearthstone-linux-gui");
    window.size_range(620, 360, 620, 360);

    let content_bg = Frame::new(0, 0, 620, 360, "");

    let mut status = Frame::new(32, 34, 556, 34, "");
    status.set_align(Align::Left | Align::Inside);
    status.set_label_font(Font::HelveticaBold);
    status.set_label_size(20);

    let mut version = Frame::new(32, 72, 556, 44, "");
    version.set_align(Align::Left | Align::Inside | Align::Wrap);
    version.set_label_size(13);

    let mut progress = Progress::new(32, 126, 556, 18, "");
    progress.set_minimum(0.0);
    progress.set_maximum(1.0);
    progress.set_value(0.0);
    progress.set_label("0%");
    progress.hide();

    let mut region_label = Frame::new(32, 162, 96, 32, "Region");
    region_label.set_align(Align::Left | Align::Inside);
    let mut region = Choice::new(144, 162, 190, 34, "");
    for item in Region::ALL {
        region.add_choice(item.as_str());
    }
    set_region_choice(&mut region, config.borrow().region);

    let mut locale_label = Frame::new(32, 208, 96, 32, "Locale");
    locale_label.set_align(Align::Left | Align::Inside);
    let mut locale = Choice::new(144, 208, 190, 34, "");
    for item in Locale::ALL {
        locale.add_choice(item.as_str());
    }
    set_locale_choice(&mut locale, config.borrow().locale);

    let install_button = Button::new(32, 286, 128, 36, "Install / Update");
    let login_button = Button::new(174, 286, 112, 36, "Login");
    let launch_button = Button::new(300, 286, 92, 36, "Play");
    let refresh_button = Button::new(406, 286, 92, 36, "Refresh");

    window.end();

    let mut ui = UiWidgets {
        window,
        content_bg,
        status,
        version,
        progress,
        region_label,
        locale_label,
        region,
        locale,
        install_button,
        login_button,
        launch_button,
        refresh_button,
    };
    set_install_idle(ui.install_button.clone(), ColorScheme::Light);
    update_login_button(ui.login_button.clone(), paths, ColorScheme::Light);
    set_launch_idle(ui.launch_button.clone(), ColorScheme::Light);
    style_choice(&mut ui.region, ColorScheme::Light);
    style_choice(&mut ui.locale, ColorScheme::Light);
    style_button(
        &mut ui.refresh_button,
        ButtonRole::Normal,
        ColorScheme::Light,
    );
    ui
}

fn set_region_choice(choice: &mut Choice, region: Region) {
    if let Some(index) = Region::ALL.iter().position(|item| *item == region) {
        choice.set_value(index as i32);
    }
}

fn set_locale_choice(choice: &mut Choice, locale: Locale) {
    if let Some(index) = Locale::ALL.iter().position(|item| *item == locale) {
        choice.set_value(index as i32);
    }
}
