//! Compact dialog with the facts a bug report needs, and a button that
//! copies them as text.

use super::text::Bindings;
use hearthstone_linux::{sysinfo::SystemInfo, tr};
use relm4::adw::prelude::*;
use relm4::{adw, gtk};
use std::{cell::Cell, rc::Rc};

/// Opens the dialog on top of `parent` and fills it once the information
/// has been collected in the background.
pub fn open(parent: &impl IsA<gtk::Window>) {
    let grid = gtk::Grid::new();
    grid.set_column_spacing(16);
    grid.set_row_spacing(6);
    grid.set_margin_top(14);
    grid.set_margin_bottom(18);
    grid.set_margin_start(18);
    grid.set_margin_end(18);
    let mut bindings = Bindings::default();
    let loading = gtk::Label::new(None);
    loading.add_css_class(relm4::css::DIM_LABEL);
    {
        let loading = loading.clone();
        bindings.watch(move || loading.set_label(&tr!("sysinfo.loading")));
    }
    grid.attach(&loading, 0, 0, 2, 1);

    let copy = gtk::Button::from_icon_name("edit-copy-symbolic");
    copy.set_sensitive(false);
    let copied = Rc::new(Cell::new(false));
    {
        let copy = copy.clone();
        let copied = Rc::clone(&copied);
        bindings.watch(move || {
            let text = if copied.get() {
                tr!("sysinfo.copied")
            } else {
                tr!("sysinfo.copy")
            };
            copy.set_tooltip_text(Some(&text));
        });
    }
    let header = adw::HeaderBar::new();
    let header_title = adw::WindowTitle::new("", "");
    header.set_title_widget(Some(&header_title));
    header.pack_start(&copy);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&grid);

    let window = adw::Window::new();
    {
        let window = window.clone();
        let header_title = header_title.clone();
        bindings.watch(move || {
            let title = tr!("sysinfo.title");
            window.set_title(Some(&title));
            header_title.set_title(&title);
        });
    }
    window.set_transient_for(Some(parent));
    window.set_modal(true);
    // Wide enough for the longest values; the height follows the content.
    window.set_default_size(560, -1);
    window.set_content(Some(&content));
    let bindings = Rc::new(std::cell::RefCell::new(bindings));
    {
        let bindings = Rc::clone(&bindings);
        window.connect_destroy(move |_| {
            let _keep = &bindings;
        });
    }
    window.present();

    // `glxinfo` and the package managers can take a moment.
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(SystemInfo::collect());
    });
    let bindings_for_rows = Rc::clone(&bindings);
    gtk::glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        // Stop waiting when the dialog was closed before the answer came.
        if !window.is_visible() {
            return gtk::glib::ControlFlow::Break;
        }
        let Ok(info) = receiver.try_recv() else {
            return gtk::glib::ControlFlow::Continue;
        };
        grid.remove(&loading);
        let info = Rc::new(info);
        let mut names = Vec::new();
        let mut values = Vec::new();
        for (index, _) in rows(&info).iter().enumerate() {
            let name = gtk::Label::new(None);
            name.set_xalign(0.0);
            name.set_valign(gtk::Align::Start);
            name.add_css_class(relm4::css::DIM_LABEL);
            let text = gtk::Label::new(None);
            text.set_xalign(0.0);
            text.set_wrap(true);
            text.set_width_chars(40);
            text.set_max_width_chars(56);
            text.set_selectable(true);
            grid.attach(&name, 0, index as i32, 1, 1);
            grid.attach(&text, 1, index as i32, 1, 1);
            names.push(name);
            values.push(text);
        }
        bindings_for_rows.borrow_mut().watch({
            let info = Rc::clone(&info);
            let names = names.clone();
            let values = values.clone();
            move || {
                for ((label, text), (name, value)) in
                    rows(&info).into_iter().zip(names.iter().zip(values.iter()))
                {
                    name.set_label(&label);
                    value.set_label(&text);
                }
            }
        });
        copy.set_sensitive(true);
        copy.connect_clicked({
            let info = Rc::clone(&info);
            let copied = Rc::clone(&copied);
            move |button| {
                let report = rows(&info)
                    .into_iter()
                    .map(|(label, value)| format!("{label}: {value}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                button.clipboard().set_text(&report);
                copied.set(true);
                button.set_tooltip_text(Some(&tr!("sysinfo.copied")));
            }
        });
        gtk::glib::ControlFlow::Break
    });
}

/// Label and value of every line, in the order they are shown and copied.
fn rows(info: &SystemInfo) -> Vec<(String, String)> {
    let unknown = || tr!("sysinfo.unknown");
    let missing = || tr!("sysinfo.not_installed");
    let toolkit = format!(
        "GTK {}.{}.{} · libadwaita {}.{}.{}",
        gtk::major_version(),
        gtk::minor_version(),
        gtk::micro_version(),
        adw::major_version(),
        adw::minor_version(),
        adw::micro_version(),
    );
    vec![
        (tr!("sysinfo.launcher"), info.launcher.clone()),
        (
            tr!("sysinfo.install"),
            info.install.clone().unwrap_or_else(unknown),
        ),
        (tr!("sysinfo.os"), info.os.clone().unwrap_or_else(unknown)),
        (
            tr!("sysinfo.kernel"),
            info.kernel.clone().unwrap_or_else(unknown),
        ),
        (
            tr!("sysinfo.session"),
            info.session.clone().unwrap_or_else(unknown),
        ),
        (
            tr!("sysinfo.gpu_driver"),
            info.gpu_driver.clone().unwrap_or_else(unknown),
        ),
        (
            tr!("sysinfo.opengl"),
            info.opengl.clone().unwrap_or_else(unknown),
        ),
        (
            "GameMode".into(),
            info.gamemode.clone().unwrap_or_else(missing),
        ),
        (
            "MangoHud".into(),
            info.mangohud.clone().unwrap_or_else(missing),
        ),
        (tr!("sysinfo.toolkit"), toolkit),
    ]
}
