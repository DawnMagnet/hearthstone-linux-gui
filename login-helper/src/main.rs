//! Optional Battle.net login window for hearthstone-linux-gui.
//!
//! Blizzard serves the Hearthstone login page (`app=wtcg`) for the game's
//! built-in browser: light text on a transparent background. In a regular
//! browser it looks blank, so this small program shows the page the way it
//! is meant to be shown, on a dark window.
//!
//! The page is Blizzard's own and is loaded from Blizzard. The program only
//! waits for the redirect that carries the login token and reports it to the
//! launcher; it injects nothing into the page and reads nothing that is typed
//! into it. The web session is ephemeral.
//!
//! Usage: `hearthstone-linux-gui-login <login-url>`
//!
//! The launcher passes its language and the translated texts in the
//! environment (see [`Texts`]), so this program does not depend on the
//! launcher crate and its lock file stays independent of the launcher's
//! dependencies. Without them it shows English texts.
//!
//! Result, read by the launcher:
//! - exit 0 and `callback <uri>` on stdout: the login finished;
//! - exit 3: the user asked for the system browser instead;
//! - exit 1: the window was closed without a login;
//! - exit 2: wrong usage (including a URL that is not a Battle.net https URL).

use adw::prelude::*;
use std::{cell::Cell, process::ExitCode, rc::Rc};
use webkit6::prelude::*;

/// URL schemes Blizzard redirects to once the login succeeded.
const CALLBACK_SCHEMES: [&str; 2] = ["blizzard-hearthstone:", "wtcg:"];

const EXIT_LOGGED_IN: u8 = 0;
const EXIT_CLOSED: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_OPEN_BROWSER: u8 = 3;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(url), None) = (args.next(), args.next()) else {
        eprintln!("usage: hearthstone-linux-gui-login <login-url>");
        return ExitCode::from(EXIT_USAGE);
    };
    if !is_login_url(&url) {
        eprintln!("refusing to open {url}: only https://*.battle.net and https://*.battlenet.com.cn login URLs are accepted");
        return ExitCode::from(EXIT_USAGE);
    }

    let texts = Rc::new(Texts::from_env());

    let app = adw::Application::builder()
        .application_id("io.github.hearthstone_linux_gui.Login")
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let outcome = Rc::new(Cell::new(EXIT_CLOSED));
    {
        let outcome = outcome.clone();
        app.connect_activate(move |app| build_window(app, &url, &texts, outcome.clone()));
    }
    // GTK must not see the login URL as a file to open.
    app.run_with_args::<&str>(&[]);
    ExitCode::from(outcome.get())
}

/// Texts and language from the launcher, English when run on its own.
struct Texts {
    /// Language for the login page, such as `de` or `zh-CN`.
    language: Option<String>,
    title: String,
    open_browser: String,
    open_browser_tooltip: String,
}

impl Texts {
    fn from_env() -> Self {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let var = |name: &str, default: &str| {
            lookup(name)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| default.to_string())
        };
        Self {
            language: lookup("HEARTHSTONE_LOGIN_LANGUAGE").filter(|value| !value.is_empty()),
            title: var("HEARTHSTONE_LOGIN_TITLE", "Battle.net login"),
            open_browser: var("HEARTHSTONE_LOGIN_OPEN_BROWSER", "Open in browser"),
            open_browser_tooltip: var(
                "HEARTHSTONE_LOGIN_OPEN_BROWSER_TOOLTIP",
                "Continue in the system browser, for example for Google or Apple sign-in, \
                 a password manager or a security key",
            ),
        }
    }
}

fn build_window(app: &adw::Application, url: &str, texts: &Texts, outcome: Rc<Cell<u8>>) {
    // The page uses light text and has no background of its own.
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);

    // Nothing of the login is kept on disk.
    let session = webkit6::NetworkSession::new_ephemeral();
    let context = webkit6::WebContext::new();
    if let Some(language) = texts.language.as_deref() {
        context.set_preferred_languages(&[language]);
    }
    let web_view = webkit6::WebView::builder()
        .network_session(&session)
        .web_context(&context)
        .build();
    // The dark window shows through the page's transparent background.
    web_view.set_background_color(&gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 0.0));
    web_view.set_vexpand(true);

    let window = adw::ApplicationWindow::new(app);
    window.set_title(Some(&texts.title));
    window.set_default_size(520, 760);

    {
        let window = window.clone();
        let outcome = outcome.clone();
        web_view.connect_decide_policy(move |web_view, decision, decision_type| {
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
                    finish_with_callback(&window, &outcome, &uri);
                    true
                }
                // `window.open` and `target=_blank` would ask for a second
                // web view, which this window does not have, and the link
                // would do nothing. Load it in this window so the login
                // carries on.
                Some(uri)
                    if decision_type == webkit6::PolicyDecisionType::NewWindowAction
                        && is_https(&uri) =>
                {
                    decision.ignore();
                    web_view.load_uri(&uri);
                    true
                }
                _ => false,
            }
        });
    }
    {
        // A server-side redirect to the callback scheme surfaces as a
        // failed load.
        let window = window.clone();
        let outcome = outcome.clone();
        web_view.connect_load_failed(move |_, _, uri, _| {
            if is_callback(uri) {
                finish_with_callback(&window, &outcome, uri);
                return true;
            }
            false
        });
    }
    web_view.load_uri(url);

    // Always available: sign-ins through Google, Apple and other providers,
    // password managers and security keys work better in the real browser.
    let open_browser = gtk::Button::with_label(&texts.open_browser);
    open_browser.set_tooltip_text(Some(&texts.open_browser_tooltip));
    {
        let window = window.clone();
        let outcome = outcome.clone();
        open_browser.connect_clicked(move |_| {
            // A callback may have arrived just before the click.
            if outcome.get() != EXIT_LOGGED_IN {
                outcome.set(EXIT_OPEN_BROWSER);
            }
            window.close();
        });
    }
    let header = adw::HeaderBar::new();
    header.pack_start(&open_browser);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&web_view);
    window.set_content(Some(&content));
    window.present();
}

fn finish_with_callback(window: &adw::ApplicationWindow, outcome: &Cell<u8>, uri: &str) {
    if outcome.get() == EXIT_LOGGED_IN {
        return;
    }
    println!("callback {uri}");
    outcome.set(EXIT_LOGGED_IN);
    window.close();
}

fn is_callback(uri: &str) -> bool {
    CALLBACK_SCHEMES.iter().any(|scheme| {
        uri.get(..scheme.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(scheme))
    })
}

fn is_https(uri: &str) -> bool {
    uri.get(..8)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"))
}

/// Whether `url` is a login page on a Battle.net host over https. The
/// launcher only passes its own Blizzard login URLs; anything else is a
/// mistake or someone else calling the program.
fn is_login_url(url: &str) -> bool {
    let Ok(uri) = gtk::glib::Uri::parse(url, gtk::glib::UriFlags::NONE) else {
        return false;
    };
    if !uri.scheme().eq_ignore_ascii_case("https") || uri.userinfo().is_some() {
        return false;
    }
    let Some(host) = uri.host() else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    ["battle.net", "battlenet.com.cn"]
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

#[cfg(test)]
mod tests {
    use super::{is_callback, is_https, is_login_url, Texts};

    #[test]
    fn accepts_only_https_battle_net_login_urls() {
        assert!(is_login_url(
            "https://battle.net/login/?ref=blizzard-hearthstone://localhost:0/&app=wtcg"
        ));
        assert!(is_login_url(
            "https://account.battlenet.com.cn/login/zh/?app=wtcg-and"
        ));
        assert!(is_login_url("HTTPS://EU.Battle.NET/login/"));
        assert!(!is_login_url("http://battle.net/login/"));
        assert!(!is_login_url("file:///etc/passwd"));
        assert!(!is_login_url("https://evil.example/login/"));
        assert!(!is_login_url("https://battle.net.evil.example/"));
        assert!(!is_login_url("https://evilbattle.net/"));
        assert!(!is_login_url("https://battle.net@evil.example/"));
        assert!(!is_login_url("https://user:pass@battle.net/login/"));
        assert!(!is_login_url("not a url"));
        assert!(!is_login_url(""));
    }

    #[test]
    fn recognises_only_the_callback_schemes() {
        assert!(is_callback(
            "blizzard-hearthstone://localhost:0/?ST=EU-abc-1"
        ));
        assert!(is_callback("wtcg://localhost:0/?ST=EU-abc-1"));
        assert!(is_callback("WTCG://localhost:0/?ST=EU-abc-1"));
        assert!(!is_callback("https://account.battle.net/login/"));
        assert!(!is_callback("about:blank"));
        assert!(!is_callback("wtcgx://localhost:0/"));
    }

    #[test]
    fn only_https_opens_links_in_the_window() {
        assert!(is_https("https://account.battle.net/login/"));
        assert!(is_https("HTTPS://account.battle.net/login/"));
        assert!(!is_https("http://account.battle.net/login/"));
        assert!(!is_https("blizzard-hearthstone://localhost:0/"));
    }

    #[test]
    fn falls_back_to_english_texts() {
        let texts = Texts::from_lookup(|_| None);
        assert!(texts.language.is_none());
        assert_eq!(texts.title, "Battle.net login");
        assert_eq!(texts.open_browser, "Open in browser");
        assert!(texts.open_browser_tooltip.contains("security key"));
    }

    #[test]
    fn takes_language_and_texts_from_the_launcher() {
        let texts = Texts::from_lookup(|name| match name {
            "HEARTHSTONE_LOGIN_LANGUAGE" => Some("de".to_string()),
            "HEARTHSTONE_LOGIN_TITLE" => Some("Battle.net-Anmeldung".to_string()),
            "HEARTHSTONE_LOGIN_OPEN_BROWSER" => Some("Im Browser öffnen".to_string()),
            "HEARTHSTONE_LOGIN_OPEN_BROWSER_TOOLTIP" => Some("Tooltip".to_string()),
            _ => None,
        });
        assert_eq!(texts.language.as_deref(), Some("de"));
        assert_eq!(texts.title, "Battle.net-Anmeldung");
        assert_eq!(texts.open_browser, "Im Browser öffnen");
        assert_eq!(texts.open_browser_tooltip, "Tooltip");
    }

    #[test]
    fn empty_environment_values_use_the_defaults() {
        let texts = Texts::from_lookup(|name| match name {
            "HEARTHSTONE_LOGIN_LANGUAGE" | "HEARTHSTONE_LOGIN_TITLE" => Some(String::new()),
            _ => None,
        });
        assert!(texts.language.is_none());
        assert_eq!(texts.title, "Battle.net login");
    }
}
