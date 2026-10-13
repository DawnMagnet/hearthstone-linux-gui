//! Battle.net login inside the launcher window.
//!
//! Blizzard serves the Hearthstone login page (`app=wtcg`) without its own
//! background because it is meant to be embedded by the game. Shown in a
//! regular browser it looks unfinished, so this view embeds the page the way
//! it was designed for. The page is Blizzard's own and is loaded from
//! Blizzard; the launcher only watches for the redirect that carries the
//! login token and never reads what is typed into the page.

use hearthstone_linux::{i18n, tr};
use relm4::gtk::{self, gdk, prelude::*};
use std::{cell::Cell, rc::Rc};
use webkit6::prelude::*;

/// URL schemes Blizzard redirects to once the login succeeded.
const CALLBACK_SCHEMES: [&str; 2] = ["blizzard-hearthstone:", "wtcg:"];

/// The page is laid out for a full browser window; scaled down it matches
/// the launcher's own controls.
const ZOOM: f64 = 0.6;
/// Width of the view; the window grows to it while the login is shown.
const WIDTH: i32 = 480;
/// Height the page is laid out for. Only the part holding Blizzard's form is
/// shown: the page centres the form and leaves the rest empty.
const PAGE_HEIGHT: i32 = 640;
/// Space kept above and below the form.
const PADDING: f64 = 10.0;

/// The view is sized to the page, so its scrollbar is only clutter. It can
/// be switched back on from the context menu.
const HIDE_SCROLLBAR: &str = "::-webkit-scrollbar { display: none; }";
/// Reports where the form sits: top and bottom of the visible content, left
/// edge of the first input and the viewport width, all in page pixels.
const FORM_BOUNDS_SCRIPT: &str = "(function () {
    var top = 1e9, bottom = 0;
    var parts = document.querySelectorAll('h1, h2, p, a, label, button, img, svg, input:not([type=hidden])');
    for (var i = 0; i < parts.length; i++) {
        var box = parts[i].getBoundingClientRect();
        if (box.width < 2 || box.height < 2) continue;
        top = Math.min(top, box.top + window.scrollY);
        bottom = Math.max(bottom, box.bottom + window.scrollY);
    }
    var input = document.querySelector('input:not([type=hidden])');
    var left = input ? input.getBoundingClientRect().left : 0;
    return [top, bottom, left, window.innerWidth].join(',');
})()";

/// Class the window carries while the login is shown.
pub const WINDOW_CLASS: &str = "login-active";

/// While the login is shown the whole window takes the background colour of
/// Blizzard's login page, so the embedded page blends in without an edge.
pub const CSS: &str = "
window.login-active,
window.login-active headerbar {
    background-color: #15171e;
}
";

/// Builds the login view for `url`. `on_callback` receives the redirect URI
/// with the login token, `on_cancel` runs when the cancel button is pressed,
/// and `on_form_indent` is told how far the login field sits from the view's
/// left edge, so other widgets can line up with it.
pub fn build(
    url: &str,
    on_callback: impl Fn(String) + 'static,
    on_cancel: impl Fn() + 'static,
    on_form_indent: impl Fn(i32) + 'static,
) -> gtk::Widget {
    // Nothing of the login is kept on disk.
    let session = webkit6::NetworkSession::new_ephemeral();
    let context = webkit6::WebContext::new();
    context.set_preferred_languages(&[i18n::current()]);
    let web_view = webkit6::WebView::builder()
        .network_session(&session)
        .web_context(&context)
        .build();
    // The window behind the view shows through the transparent page.
    web_view.set_background_color(&gdk::RGBA::new(0.0, 0.0, 0.0, 0.0));
    web_view.set_zoom_level(ZOOM);
    web_view.set_vexpand(true);
    web_view.set_hexpand(true);

    let on_callback = Rc::new(on_callback);
    {
        let on_callback = on_callback.clone();
        web_view.connect_decide_policy(move |_, decision, _| {
            let Some(navigation) = decision.downcast_ref::<webkit6::NavigationPolicyDecision>()
            else {
                return false;
            };
            let uri = navigation
                .navigation_action()
                .and_then(|mut action| action.request())
                .and_then(|request| request.uri());
            match uri {
                Some(uri) if is_callback(&uri) => {
                    decision.ignore();
                    on_callback(uri.to_string());
                    true
                }
                _ => false,
            }
        });
    }
    // A server-side redirect to the callback scheme surfaces as a failed load.
    web_view.connect_load_failed(move |_, _, uri, _| {
        if is_callback(uri) {
            on_callback(uri.to_string());
            return true;
        }
        false
    });
    web_view.load_uri(url);

    web_view.set_size_request(WIDTH, PAGE_HEIGHT);

    // Window onto the part of the page that holds the form: a scroller
    // without scrollbars, positioned from the code below.
    let frame = gtk::ScrolledWindow::new();
    frame.set_policy(gtk::PolicyType::Never, gtk::PolicyType::External);
    frame.set_halign(gtk::Align::Center);
    frame.set_size_request(WIDTH, PAGE_HEIGHT / 2);
    frame.set_child(Some(&web_view));

    // The form changes between the login steps, so keep following it for as
    // long as the view exists.
    let on_form_indent = Rc::new(on_form_indent);
    let weak_view = web_view.downgrade();
    let weak_frame = frame.downgrade();
    gtk::glib::timeout_add_local(std::time::Duration::from_millis(400), move || {
        let (Some(view), Some(frame)) = (weak_view.upgrade(), weak_frame.upgrade()) else {
            return gtk::glib::ControlFlow::Break;
        };
        let on_form_indent = on_form_indent.clone();
        let width = f64::from(view.width());
        view.evaluate_javascript(
            FORM_BOUNDS_SCRIPT,
            None,
            None,
            gtk::gio::Cancellable::NONE,
            move |result| {
                let Some(bounds) = result
                    .ok()
                    .and_then(|value| FormBounds::parse(&value.to_str()))
                else {
                    return;
                };
                // Page pixels to widget pixels, whatever zoom and font
                // scaling are in effect.
                let scale = width / bounds.viewport_width;
                let top = (bounds.top * scale - PADDING).max(0.0);
                let height = (bounds.bottom - bounds.top) * scale + 2.0 * PADDING;
                frame.set_size_request(WIDTH, (height.ceil() as i32).min(PAGE_HEIGHT));
                frame.vadjustment().set_value(top);
                on_form_indent((bounds.left * scale).round() as i32);
            },
        );
        gtk::glib::ControlFlow::Continue
    });

    let scrollbar_hidden = Rc::new(Cell::new(true));
    apply_scrollbar(&web_view, true);
    web_view.connect_context_menu(move |web_view, menu, _| {
        let hidden = scrollbar_hidden.clone();
        let view = web_view.clone();
        let action = gtk::gio::SimpleAction::new("toggle-scrollbar", None);
        action.connect_activate(move |_, _| {
            hidden.set(!hidden.get());
            apply_scrollbar(&view, hidden.get());
        });
        let label = if scrollbar_hidden.get() {
            tr!("login.show_scrollbar")
        } else {
            tr!("login.hide_scrollbar")
        };
        menu.append(&webkit6::ContextMenuItem::from_gaction(
            &action, &label, None,
        ));
        false
    });

    // Below the page rather than on top of it: the gap in Blizzard's form
    // holds the privacy notice, which must stay readable.
    let cancel = gtk::Button::with_label(&tr!("action.cancel_login"));
    cancel.add_css_class(relm4::css::DESTRUCTIVE_ACTION);
    cancel.set_halign(gtk::Align::Center);
    cancel.connect_clicked(move |_| on_cancel());

    let view = gtk::Box::new(gtk::Orientation::Vertical, 12);
    view.append(&frame);
    view.append(&cancel);
    view.upcast()
}

/// Shows or hides the page's scrollbar through a user style sheet.
fn apply_scrollbar(web_view: &webkit6::WebView, hidden: bool) {
    let Some(content) = web_view.user_content_manager() else {
        return;
    };
    content.remove_all_style_sheets();
    if hidden {
        content.add_style_sheet(&webkit6::UserStyleSheet::new(
            HIDE_SCROLLBAR,
            webkit6::UserContentInjectedFrames::AllFrames,
            webkit6::UserStyleLevel::User,
            &[],
            &[],
        ));
    }
}

/// Position of Blizzard's form inside the page, in page pixels.
struct FormBounds {
    top: f64,
    bottom: f64,
    left: f64,
    viewport_width: f64,
}

impl FormBounds {
    /// Parses the `top,bottom,left,width` answer of [`FORM_BOUNDS_SCRIPT`].
    fn parse(text: &str) -> Option<Self> {
        let mut numbers = text.split(',').map(|part| part.trim().parse::<f64>());
        let bounds = Self {
            top: numbers.next()?.ok()?,
            bottom: numbers.next()?.ok()?,
            left: numbers.next()?.ok()?,
            viewport_width: numbers.next()?.ok()?,
        };
        (bounds.bottom > bounds.top && bounds.viewport_width > 0.0).then_some(bounds)
    }
}

fn is_callback(uri: &str) -> bool {
    CALLBACK_SCHEMES
        .iter()
        .any(|scheme| uri.starts_with(scheme))
}
