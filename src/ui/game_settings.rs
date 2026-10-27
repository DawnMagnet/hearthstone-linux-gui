//! Dialog for the game's own settings: typed controls for frame rate, VSync
//! and graphics quality, plus plain editors for `options.txt` and for
//! additional `client.config` entries.

use super::{
    browser,
    text::{self, Bindings},
};
use hearthstone_linux::{
    config::AppConfig,
    game_options::{GameOptions, Quality},
    i18n::Phrase,
    install::compatibility,
    paths::AppPaths,
    phrase, tr,
};
use relm4::adw::prelude::*;
use relm4::{adw, gtk};
use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    rc::Rc,
};

const DEFAULT_FRAME_RATE: u32 = 60;

/// Opens the dialog on top of `parent`, editing the game's `options_path`.
/// `on_saved` runs after the files were written.
pub fn open(
    parent: &impl IsA<gtk::Window>,
    paths: &AppPaths,
    config: &AppConfig,
    options_path: PathBuf,
    on_saved: impl Fn() + 'static,
) {
    let on_saved = Rc::new(on_saved);
    let overrides_path = paths.client_config_overrides();
    let game_dir = config
        .game_dir
        .clone()
        .unwrap_or_else(|| paths.game_dir.clone());
    let (region, locale) = (config.region, config.locale);

    // A file that exists but cannot be read must not be replaced by the
    // edits alone; the dialog then only reports the problem.
    let (options, read_error) = match GameOptions::load(&options_path) {
        Ok(options) => (options, None),
        Err(error) => {
            tracing::warn!(error = %format!("{error:#}"), "failed to read options.txt");
            (GameOptions::default(), Some(error))
        }
    };
    let options_buffer = gtk::TextBuffer::new(None);
    options_buffer.set_text(&options.to_text());
    let overrides_buffer = gtk::TextBuffer::new(None);
    let initial_overrides = std::fs::read_to_string(&overrides_path).unwrap_or_default();
    overrides_buffer.set_text(&initial_overrides);

    let mut bindings = Bindings::default();
    // The text of `options.txt` is the single source of truth. Controls write
    // into it and follow it; `syncing` keeps the two directions from looping.
    let syncing = Rc::new(Cell::new(false));
    let frame_rate = gtk::SpinButton::with_range(30.0, 360.0, 1.0);
    frame_rate.set_valign(gtk::Align::Center);
    let vsync = gtk::Switch::new();
    vsync.set_valign(gtk::Align::Center);
    let quality_labels = gtk::StringList::new(&[]);
    let quality = gtk::DropDown::builder().model(&quality_labels).build();
    {
        let quality_labels = quality_labels.clone();
        let quality = quality.clone();
        let syncing = syncing.clone();
        bindings.watch(move || {
            // Replacing the strings notifies the dropdown; that must not
            // write the selected level back into options.txt.
            let selected = quality.selected();
            let _busy = syncing.replace(true);
            while quality_labels.n_items() > 0 {
                quality_labels.remove(0);
            }
            for text in [
                tr!("game_settings.quality_low"),
                tr!("game_settings.quality_medium"),
                tr!("game_settings.quality_high"),
            ] {
                quality_labels.append(&text);
            }
            quality.set_selected(selected);
            syncing.set(false);
        });
    }
    quality.set_valign(gtk::Align::Center);

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
    let frame_row = row("", "", &frame_rate);
    let vsync_row = row("", "", &vsync);
    let quality_row = row("", "", &quality);
    graphics.add(&frame_row);
    graphics.add(&vsync_row);
    graphics.add(&quality_row);
    {
        let graphics = graphics.clone();
        let frame_row = frame_row.clone();
        let vsync_row = vsync_row.clone();
        let quality_row = quality_row.clone();
        bindings.watch(move || {
            graphics.set_title(&tr!("game_settings.graphics"));
            frame_row.set_title(&tr!("game_settings.frame_rate"));
            frame_row.set_subtitle(&tr!("game_settings.frame_rate_subtitle"));
            vsync_row.set_title(&tr!("game_settings.vsync"));
            vsync_row.set_subtitle(&tr!("game_settings.vsync_subtitle"));
            quality_row.set_title(&tr!("game_settings.quality"));
        });
    }

    let advanced = adw::PreferencesGroup::new();
    let options_row = editor_row("options.txt", "", &options_buffer);
    let client_row = editor_row("client.config", "", &overrides_buffer);
    advanced.add(&options_row);
    advanced.add(&client_row);
    {
        let advanced = advanced.clone();
        let options_row = options_row.clone();
        let client_row = client_row.clone();
        bindings.watch(move || {
            advanced.set_title(&tr!("game_settings.advanced"));
            options_row.set_subtitle(&tr!("game_settings.options_subtitle"));
            client_row.set_subtitle(&tr!("game_settings.client_config_subtitle"));
        });
    }

    let status = gtk::Label::new(None);
    status.set_wrap(true);
    status.add_css_class(relm4::css::ERROR);
    status.set_visible(false);
    let status_phrase: Rc<RefCell<Option<Phrase>>> = Rc::new(RefCell::new(None));
    {
        let status = status.clone();
        let status_phrase = Rc::clone(&status_phrase);
        bindings.watch(move || {
            if let Some(phrase) = status_phrase.borrow().as_ref() {
                status.set_label(&phrase.render());
            }
        });
    }

    let reset = gtk::Button::new();
    let open_external = gtk::Button::new();
    let save = gtk::Button::new();
    {
        let reset = reset.clone();
        let open_external = open_external.clone();
        let save = save.clone();
        bindings.watch(move || {
            reset.set_label(&tr!("game_settings.reset"));
            open_external.set_label(&tr!("game_settings.open_external"));
            save.set_label(&tr!("game_settings.save"));
        });
    }
    save.add_css_class(relm4::css::SUGGESTED_ACTION);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::Center);
    buttons.append(&reset);
    buttons.append(&open_external);
    buttons.append(&save);

    if let Some(error) = read_error {
        let phrase = phrase!("game_settings.read_failed", error = format!("{error:#}"));
        status.set_label(&phrase.render());
        *status_phrase.borrow_mut() = Some(phrase);
        status.set_visible(true);
        graphics.set_sensitive(false);
        advanced.set_sensitive(false);
        for button in [&reset, &open_external, &save] {
            button.set_sensitive(false);
        }
    }

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
    let header_title = adw::WindowTitle::new("", "");
    header.set_title_widget(Some(&header_title));
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&scrolled);

    let window = adw::Window::new();
    {
        let window = window.clone();
        let header_title = header_title.clone();
        bindings.watch(move || {
            let title = tr!("game_settings.title");
            window.set_title(Some(&title));
            header_title.set_title(&title);
        });
    }
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
    // Writes everything the dialog edits. `client.config` is only rewritten
    // when its entries changed, so saving a graphics setting never applies a
    // Region / Language change that is waiting for the next Install.
    let save_files = {
        let (options_buffer, overrides_buffer, options_path) = (
            options_buffer.clone(),
            overrides_buffer.clone(),
            options_path.clone(),
        );
        Rc::new(move || -> anyhow::Result<()> {
            let overrides = buffer_text(&overrides_buffer);
            GameOptions::parse(&buffer_text(&options_buffer)).save(&options_path)?;
            save_overrides(&overrides_path, &overrides)?;
            // Not installed yet: Install / Update applies the entries.
            if overrides != initial_overrides && game_dir.join("client.config").exists() {
                compatibility::write_client_config(&game_dir, region, locale, &overrides)?;
            }
            Ok(())
        })
    };
    {
        let (save_files, on_saved, status, status_phrase, window) = (
            save_files.clone(),
            on_saved.clone(),
            status.clone(),
            Rc::clone(&status_phrase),
            window.clone(),
        );
        open_external.connect_clicked(move |_| {
            // The editor works on the file itself, so everything is saved
            // first and the dialog closes: its text would otherwise go stale
            // and a later Save would overwrite what the editor wrote.
            if let Err(error) = save_files() {
                show_error(&status, &status_phrase, &error);
                return;
            }
            let options_path = options_path.clone();
            std::thread::spawn(move || {
                if let Err(error) = browser::open_file(&options_path) {
                    tracing::warn!(error = %format!("{error:#}"), "failed to open options.txt");
                }
            });
            on_saved();
            window.close();
        });
    }
    {
        let window = window.clone();
        let status_phrase = Rc::clone(&status_phrase);
        save.connect_clicked(move |_| match save_files() {
            Ok(()) => {
                on_saved();
                window.close();
            }
            Err(error) => show_error(&status, &status_phrase, &error),
        });
    }

    text::retain(&window, bindings);
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

fn show_error(status: &gtk::Label, status_phrase: &RefCell<Option<Phrase>>, error: &anyhow::Error) {
    tracing::error!(error = %format!("{error:#}"), "failed to save game settings");
    let phrase = phrase!("game_settings.save_failed", error = format!("{error:#}"));
    status.set_label(&phrase.render());
    *status_phrase.borrow_mut() = Some(phrase);
    status.set_visible(true);
}
