//! Keeps widgets in step with the UI language.
//!
//! [`Bindings::watch`] runs a closure immediately and again whenever
//! [`hearthstone_linux::i18n::select`] changes the catalog. The closure sets
//! the widget's text from `tr!` or from a [`Phrase`](hearthstone_linux::i18n::Phrase).
//! Dropping the bindings (or the widget they are stored on) unregisters the
//! closures. Relm4 `#[watch]` properties do not need this: the main window
//! re-renders its children on the language change, and a watch that calls
//! `tr!` picks up the new catalog.

use hearthstone_linux::i18n::{self, Subscription};
use relm4::gtk::{prelude::*, Widget};

/// Closures that refresh widget text. Keep the value as long as the widgets.
#[derive(Default)]
pub struct Bindings {
    subscriptions: Vec<Subscription>,
}

impl Bindings {
    pub fn watch(&mut self, apply: impl Fn() + 'static) {
        apply();
        self.subscriptions.push(i18n::subscribe(apply));
    }
}

/// Drops `bindings` together with `widget`, which unregisters its closures.
pub fn retain(widget: &impl IsA<Widget>, bindings: Bindings) {
    // The handler lives on the widget, so the bindings do too.
    let bindings = std::rc::Rc::new(bindings);
    widget.as_ref().connect_destroy(move |_| {
        let _keep = &bindings;
    });
}

#[cfg(test)]
mod tests {
    use super::Bindings;
    use hearthstone_linux::{i18n::LanguageHold, phrase, tr};
    use relm4::adw::prelude::*;
    use relm4::{adw, gtk};

    /// The same window object gets new text. Skipped without a display: GTK
    /// will not open a window in the headless CI job.
    #[test]
    fn language_switch_updates_the_open_window() {
        if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
            return;
        }
        let hold = LanguageHold::acquire();
        hold.select(Some("en"));

        gtk::init().expect("gtk");
        let _ = adw::init();
        let app = adw::Application::builder()
            .application_id("io.github.hearthstone_linux_gui.i18n_test")
            .build();
        app.connect_activate(move |app| {
            super::super::components::assert_choice_dropdown_follows_language(&hold);
            let window = adw::ApplicationWindow::builder()
                .application(app)
                .default_width(320)
                .default_height(120)
                .build();
            let label = gtk::Label::new(None);
            let label_for_text = label.clone();
            let mut bindings = Bindings::default();
            bindings.watch(move || label_for_text.set_label(&tr!("status.ready")));
            window.set_content(Some(&label));
            window.present();
            let before = window.as_ptr();
            assert_eq!(label.label().as_str(), "Ready");
            assert!(hold.select(Some("de")));
            assert_eq!(label.label().as_str(), "Bereit");
            assert!(hold.select(Some("fr")));
            assert_eq!(label.label().as_str(), "Prêt");
            // A phrase kept by the caller rerenders without being replaced.
            let headline = phrase!("game.running");
            assert_eq!(headline.render(), "Jeu en cours");
            assert_eq!(window.as_ptr(), before);
            app.quit();
        });
        // Cargo's test arguments must not be parsed as application arguments.
        app.run_with_args::<&str>(&[]);
    }
}
