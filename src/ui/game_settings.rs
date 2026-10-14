//! Dialog for the game's own settings: typed controls for frame rate, VSync
//! and graphics quality, plus plain editors for `options.txt` and for
//! additional `client.config` entries.

use super::browser;
use hearthstone_linux::{
    config::AppConfig,
    game_options::{self, GameOptions, Quality},
    install::compatibility,
    paths::AppPaths,
    tr,
};
use relm4::adw::prelude::*;
use relm4::{adw, gtk};
use std::{cell::Cell, path::Path, rc::Rc};

const DEFAULT_FRAME_RATE: u32 = 60;

/// Opens the dialog on top of `parent`. `on_saved` runs after the files were
/// written.
pub fn open(
    parent: &impl IsA<gtk::Window>,
    paths: &AppPaths,
    config: &AppConfig,
    on_saved: impl Fn() + 'static,
) {
    let Some(options_path) = game_options::options_path() else {
        tracing::error!("HOME is not set; cannot locate options.txt");
        return;
    };
    let overrides_path = paths.client_config_overrides();
    let game_dir = config
        .game_dir
        .clone()
        .unwrap_or_else(|| paths.game_dir.clone());
    let (region, locale) = (config.region, config.locale);

    let options = GameOptions::load(&options_path).unwrap_or_else(|error| {
        tracing::warn!(error = %format!("{error:#}"), "failed to read options.txt");
        GameOptions::default()
    });
    let options_buffer = gtk::TextBuffer::new(None);
    options_buffer.set_text(&options.to_text());
    let overrides_buffer = gtk::TextBuffer::new(None);
    overrides_buffer.set_text(&std::fs::read_to_string(&overrides_path).unwrap_or_default());

    let frame_rate = gtk::SpinButton::with_range(30.0, 360.0, 1.0);
    frame_rate.set_valign(gtk::Align::Center);
    let vsync = gtk::Switch::new();
    vsync.set_valign(gtk::Align::Center);
    let quality = gtk::DropDown::from_strings(&[
        &tr!("game_settings.quality_low"),
        &tr!("game_settings.quality_medium"),
        &tr!("game_settings.quality_high"),
    ]);
    quality.set_valign(gtk::Align::Center);

    // The text of `options.txt` is the single source of truth. Controls write
    // into it and follow it; `syncing` keeps the two directions from looping.
    let syncing = Rc::new(Cell::new(false));
    let show_options = {
        let (frame_rate, vsync, quality) = (frame_rate.clone(), vsync.clone(), quality.clone());
        move |options: &GameOptions| {
            frame_rate.set_value(f64::from(
                options.frame_rate().unwrap_or(DEFAULT_FRAME_RATE),
            ));
            vsync.set_active(options.vsync());
            let level = options.quality().unwrap_or(Quality::High);
            quality.set_selected(Quality::ALL.iter().position(|q| *q == level).unwrap_or(2) as u32);
        }
    };
    show_options(&options);

    let edit_options = {
        let (buffer, syncing) = (options_buffer.clone(), syncing.clone());
        move |edit: &dyn Fn(&mut GameOptions)| {
            if syncing.replace(true) {
                return;
            }
            let mut options = GameOptions::parse(&buffer_text(&buffer));
            edit(&mut options);
            buffer.set_text(&options.to_text());
            syncing.set(false);
        }
    };
    {
        let edit_options = edit_options.clone();
        frame_rate.connect_value_changed(move |spin| {
            let fps = spin.value_as_int().max(1) as u32;
            edit_options(&|options| options.set_frame_rate(fps));
        });
    }
    {
        let edit_options = edit_options.clone();
        vsync.connect_active_notify(move |switch| {
            let enabled = switch.is_active();
            edit_options(&|options| options.set_vsync(enabled));
        });
    }
    {
        let edit_options = edit_options.clone();
        quality.connect_selected_notify(move |dropdown| {
            if let Some(level) = Quality::ALL.get(dropdown.selected() as usize).copied() {
                edit_options(&|options| options.set_quality(level));
            }
        });
    }
    {
        let (syncing, show_options) = (syncing.clone(), show_options.clone());
        options_buffer.connect_changed(move |buffer| {
            if syncing.replace(true) {
                return;
            }
            show_options(&GameOptions::parse(&buffer_text(buffer)));
            syncing.set(false);
        });
    }

    let graphics = adw::PreferencesGroup::new();
    graphics.set_title(&tr!("game_settings.graphics"));
    graphics.add(&row(
        &tr!("game_settings.frame_rate"),
        &tr!("game_settings.frame_rate_subtitle"),
        &frame_rate,
    ));
    graphics.add(&row(
        &tr!("game_settings.vsync"),
        &tr!("game_settings.vsync_subtitle"),
        &vsync,
    ));
    graphics.add(&row(&tr!("game_settings.quality"), "", &quality));

    let advanced = adw::PreferencesGroup::new();
    advanced.set_title(&tr!("game_settings.advanced"));
    advanced.add(&editor_row(
        "options.txt",
        &tr!("game_settings.options_subtitle"),
        &options_buffer,
    ));
    advanced.add(&editor_row(
        "client.config",
        &tr!("game_settings.client_config_subtitle"),
        &overrides_buffer,
    ));

    let status = gtk::Label::new(None);
    status.set_wrap(true);
    status.add_css_class(relm4::css::ERROR);
    status.set_visible(false);

    let reset = gtk::Button::with_label(&tr!("game_settings.reset"));
    let open_external = gtk::Button::with_label(&tr!("game_settings.open_external"));
    let save = gtk::Button::with_label(&tr!("game_settings.save"));
    save.add_css_class(relm4::css::SUGGESTED_ACTION);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::Center);
    buttons.append(&reset);
    buttons.append(&open_external);
    buttons.append(&save);

    let page = gtk::Box::new(gtk::Orientation::Vertical, 18);
    page.set_margin_top(18);
    page.set_margin_bottom(18);
    page.set_margin_start(18);
    page.set_margin_end(18);
    page.append(&graphics);
    page.append(&advanced);
    page.append(&status);
    page.append(&buttons);
    let clamp = adw::Clamp::new();
    clamp.set_child(Some(&page));
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_vexpand(true);
    scrolled.set_hscrollbar_policy(gtk::PolicyType::Never);
    scrolled.set_child(Some(&clamp));

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new(
        &tr!("game_settings.title"),
        "",
    )));
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&scrolled);

    let window = adw::Window::new();
    window.set_title(Some(&tr!("game_settings.title")));
    window.set_transient_for(Some(parent));
    window.set_modal(true);
    window.set_default_size(520, 560);
    window.set_content(Some(&content));

    {
        let (options_buffer, overrides_buffer) = (options_buffer.clone(), overrides_buffer.clone());
        reset.connect_clicked(move |_| {
            let mut options = GameOptions::parse(&buffer_text(&options_buffer));
            options.reset_graphics();
            options_buffer.set_text(&options.to_text());
            overrides_buffer.set_text("");
        });
    }
    {
        let (options_buffer, options_path, status) =
            (options_buffer.clone(), options_path.clone(), status.clone());
        open_external.connect_clicked(move |_| {
            // An external editor needs the file on disk, with the current text.
            let saved = GameOptions::parse(&buffer_text(&options_buffer)).save(&options_path);
            if let Err(error) = saved {
                show_error(&status, &error);
                return;
            }
            let Ok(url) = url::Url::from_file_path(&options_path) else {
                return;
            };
            std::thread::spawn(move || {
                if let Err(error) = browser::open_url(url.as_str()) {
                    tracing::warn!(error = %format!("{error:#}"), "failed to open options.txt");
                }
            });
        });
    }
    {
        let window = window.clone();
        save.connect_clicked(move |_| {
            let overrides = buffer_text(&overrides_buffer);
            let saved = GameOptions::parse(&buffer_text(&options_buffer))
                .save(&options_path)
                .and_then(|()| save_overrides(&overrides_path, &overrides))
                .and_then(|()| {
                    // Not installed yet: Install / Update applies the entries.
                    if !game_dir.join("client.config").exists() {
                        return Ok(());
                    }
                    compatibility::write_client_config(&game_dir, region, locale, &overrides)
                });
            match saved {
                Ok(()) => {
                    on_saved();
                    window.close();
                }
                Err(error) => show_error(&status, &error),
            }
        });
    }

    window.present();
}

fn row(title: &str, subtitle: &str, control: &impl IsA<gtk::Widget>) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title(title);
    row.set_subtitle(subtitle);
    row.add_suffix(control);
    row
}

/// Collapsed row that expands to a plain text editor for `buffer`.
fn editor_row(title: &str, subtitle: &str, buffer: &gtk::TextBuffer) -> adw::ExpanderRow {
    let view = gtk::TextView::with_buffer(buffer);
    view.set_monospace(true);
    view.set_top_margin(8);
    view.set_bottom_margin(8);
    view.set_left_margin(8);
    view.set_right_margin(8);
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_min_content_height(180);
    scrolled.set_child(Some(&view));

    let row = adw::ExpanderRow::new();
    row.set_title(title);
    row.set_subtitle(subtitle);
    row.add_row(&scrolled);
    row
}

fn buffer_text(buffer: &gtk::TextBuffer) -> String {
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .to_string()
}

fn save_overrides(path: &Path, overrides: &str) -> anyhow::Result<()> {
    if overrides.trim().is_empty() {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, overrides)?;
    Ok(())
}

fn show_error(status: &gtk::Label, error: &anyhow::Error) {
    tracing::error!(error = %format!("{error:#}"), "failed to save game settings");
    status.set_label(&tr!(
        "game_settings.save_failed",
        error = format!("{error:#}")
    ));
    status.set_visible(true);
}
