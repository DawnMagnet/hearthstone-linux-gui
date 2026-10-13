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
use std::rc::Rc;
use webkit6::prelude::*;

/// URL schemes Blizzard redirects to once the login succeeded.
const CALLBACK_SCHEMES: [&str; 2] = ["blizzard-hearthstone:", "wtcg:"];

/// The page is laid out for a full browser window; scaled down it matches
/// the launcher's own controls.
const ZOOM: f64 = 0.6;
const HEIGHT: i32 = 350;

/// Class the window carries while the login is shown.
pub const WINDOW_CLASS: &str = "login-active";

/// While the login is shown the whole window takes the background colour of
/// Blizzard's login page, so the embedded page blends in without an edge.
pub const CSS: &str = "
window.login-active,
window.login-active headerbar {
    background-color: #15171e;
}
/* Pulls the page up over its own empty top area, so its heading follows the
   launcher's headline directly. */
.login-page {
    margin-top: -56px;
}
";

/// Builds the login view for `url`. `on_callback` receives the redirect URI
/// with the login token, `on_cancel` runs when the cancel button is pressed.
pub fn build(
    url: &str,
    on_callback: impl Fn(String) + 'static,
    on_cancel: impl Fn() + 'static,
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

    web_view.set_size_request(-1, HEIGHT);
    web_view.add_css_class("login-page");

    // Below the page rather than on top of it: the gap in Blizzard's form
    // holds the privacy notice, which must stay readable.
    let cancel = gtk::Button::with_label(&tr!("action.cancel_login"));
    cancel.add_css_class(relm4::css::DESTRUCTIVE_ACTION);
    cancel.set_halign(gtk::Align::Center);
    cancel.connect_clicked(move |_| on_cancel());

    let view = gtk::Box::new(gtk::Orientation::Vertical, 12);
    view.append(&web_view);
    view.append(&cancel);
    view.upcast()
}

fn is_callback(uri: &str) -> bool {
    CALLBACK_SCHEMES
        .iter()
        .any(|scheme| uri.starts_with(scheme))
}
