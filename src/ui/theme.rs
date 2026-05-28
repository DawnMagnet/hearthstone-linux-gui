use crate::ui::window::UiWidgets;
use fltk::{
    app,
    button::Button,
    enums::{Color, FrameType},
    menu::Choice,
    prelude::*,
};
use std::process::Command;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ColorScheme {
    Light,
    Dark,
}

#[derive(Clone, Copy)]
pub(crate) enum ButtonRole {
    Normal,
    Suggested,
    Destructive,
}

#[derive(Clone, Copy)]
pub(crate) struct ThemeState {
    pub logged_in: bool,
    pub install_active: bool,
    pub login_active: bool,
    pub game_active: bool,
}

#[derive(Clone, Copy)]
struct Palette {
    background: (u8, u8, u8),
    surface: (u8, u8, u8),
    text: (u8, u8, u8),
    muted: (u8, u8, u8),
    button: (u8, u8, u8),
    accent: (u8, u8, u8),
    accent_text: (u8, u8, u8),
    destructive: (u8, u8, u8),
    destructive_text: (u8, u8, u8),
}

pub(crate) fn detect_color_scheme() -> ColorScheme {
    read_portal_color_scheme()
        .or_else(read_gsettings_color_scheme)
        .or_else(read_environment_color_scheme)
        .unwrap_or(ColorScheme::Light)
}

pub(crate) fn apply_theme(ui: &mut UiWidgets, scheme: ColorScheme, state: ThemeState) {
    let palette = palette(scheme);
    app::background(
        palette.background.0,
        palette.background.1,
        palette.background.2,
    );
    app::background2(palette.surface.0, palette.surface.1, palette.surface.2);
    app::foreground(palette.text.0, palette.text.1, palette.text.2);

    let background = color(palette.background);
    let surface = color(palette.surface);
    let text = color(palette.text);
    let muted = color(palette.muted);

    ui.window.set_color(background);
    ui.content_bg.set_frame(FrameType::FlatBox);
    ui.content_bg.set_color(background);
    ui.status.set_label_color(text);
    ui.version.set_label_color(muted);
    ui.region_label.set_label_color(text);
    ui.locale_label.set_label_color(text);
    style_choice(&mut ui.region, scheme);
    style_choice(&mut ui.locale, scheme);
    ui.progress.set_color(surface);
    ui.progress.set_selection_color(color(palette.accent));
    ui.progress.set_label_color(text);

    style_button(
        &mut ui.install_button,
        if state.install_active {
            ButtonRole::Destructive
        } else {
            ButtonRole::Suggested
        },
        scheme,
    );
    style_button(
        &mut ui.login_button,
        if state.login_active {
            ButtonRole::Destructive
        } else if state.logged_in {
            ButtonRole::Suggested
        } else {
            ButtonRole::Normal
        },
        scheme,
    );
    style_button(
        &mut ui.launch_button,
        if state.game_active {
            ButtonRole::Destructive
        } else {
            ButtonRole::Suggested
        },
        scheme,
    );
    style_button(&mut ui.refresh_button, ButtonRole::Normal, scheme);
    ui.window.redraw();
}

pub(crate) fn style_button(button: &mut Button, role: ButtonRole, scheme: ColorScheme) {
    let palette = palette(scheme);
    let (fill, label) = match role {
        ButtonRole::Normal => (palette.button, palette.text),
        ButtonRole::Suggested => (palette.accent, palette.accent_text),
        ButtonRole::Destructive => (palette.destructive, palette.destructive_text),
    };
    button.set_frame(FrameType::RFlatBox);
    button.set_color(color(fill));
    button.set_selection_color(color(fill));
    button.set_label_color(color(label));
    button.set_label_size(13);
}

pub(crate) fn style_choice(choice: &mut Choice, scheme: ColorScheme) {
    let palette = palette(scheme);
    choice.set_frame(FrameType::RFlatBox);
    choice.set_color(color(palette.surface));
    choice.set_selection_color(color(palette.accent));
    choice.set_label_color(color(palette.text));
    choice.set_text_color(color(palette.text));
    choice.set_text_size(13);
}

fn read_portal_color_scheme() -> Option<ColorScheme> {
    let output = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.portal.Desktop",
            "--object-path",
            "/org/freedesktop/portal/desktop",
            "--method",
            "org.freedesktop.portal.Settings.Read",
            "org.freedesktop.appearance",
            "color-scheme",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_portal_color_scheme(&String::from_utf8_lossy(&output.stdout))
}

fn parse_portal_color_scheme(output: &str) -> Option<ColorScheme> {
    if output.contains("uint32 1") || output.contains("<1>") {
        Some(ColorScheme::Dark)
    } else if output.contains("uint32 2") || output.contains("<2>") {
        Some(ColorScheme::Light)
    } else {
        None
    }
}

fn read_gsettings_color_scheme() -> Option<ColorScheme> {
    let output = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    if value.contains("prefer-dark") {
        Some(ColorScheme::Dark)
    } else if value.contains("prefer-light") || value.contains("default") {
        Some(ColorScheme::Light)
    } else {
        None
    }
}

fn read_environment_color_scheme() -> Option<ColorScheme> {
    let theme = std::env::var("GTK_THEME").ok()?.to_ascii_lowercase();
    if theme.contains("dark") {
        Some(ColorScheme::Dark)
    } else {
        None
    }
}

fn palette(scheme: ColorScheme) -> Palette {
    match scheme {
        ColorScheme::Light => Palette {
            background: (250, 250, 250),
            surface: (255, 255, 255),
            text: (34, 34, 34),
            muted: (98, 98, 98),
            button: (232, 232, 232),
            accent: (53, 132, 228),
            accent_text: (255, 255, 255),
            destructive: (224, 27, 36),
            destructive_text: (255, 255, 255),
        },
        ColorScheme::Dark => Palette {
            background: (36, 36, 36),
            surface: (48, 48, 48),
            text: (245, 245, 245),
            muted: (176, 176, 176),
            button: (66, 66, 66),
            accent: (120, 174, 237),
            accent_text: (20, 20, 20),
            destructive: (237, 91, 93),
            destructive_text: (20, 20, 20),
        },
    }
}

fn color(value: (u8, u8, u8)) -> Color {
    Color::from_rgb(value.0, value.1, value.2)
}
