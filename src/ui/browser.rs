//! Opening URLs (the Battle.net login page in particular) in the user's
//! browser.
//!
//! Every opener runs with the host environment restored (see
//! [`hearthstone_linux::host_env`]): host programs must not inherit the
//! AppImage's bundled glibc/GLib, or they crash before showing anything
//! (issue #22). The XDG Desktop Portal is tried first among the system
//! openers because it hands the URL to the host session over D-Bus and never
//! starts a process from our environment at all.

use hearthstone_linux::host_env::{self, HostVar};
use relm4::gtk::{
    gio::{self, prelude::*},
    glib,
};
use std::{
    process::{Child, Command},
    time::{Duration, Instant},
};

/// How long to wait for an opener such as `xdg-open` to report failure before
/// assuming it handed the URL to a browser.
const OPENER_GRACE: Duration = Duration::from_millis(1500);

/// D-Bus timeout for the portal call. The portal may need to be
/// D-Bus-activated first, so this is a bit longer than the opener grace.
const PORTAL_TIMEOUT_MS: i32 = 5000;

/// Command-line openers, tried in order after `$BROWSER` and the portal.
const OPENERS: &[(&str, &[&str])] = &[
    ("xdg-open", &[]),
    ("gio", &["open"]),
    ("kioclient", &["exec"]),
    ("kioclient5", &["exec"]),
    ("kde-open5", &[]),
    ("kde-open", &[]),
    ("exo-open", &[]),
    ("gvfs-open", &[]),
    ("sensible-browser", &[]),
    ("firefox", &[]),
    ("firefox-esr", &[]),
    ("librewolf", &[]),
    ("chromium", &[]),
    ("google-chrome", &[]),
];

/// Tries every known way to open `url` and returns the name of the opener
/// that accepted it.
///
/// This blocks for up to a few seconds per failing opener, so call it off the
/// GTK main thread.
pub fn open_url(url: &str) -> anyhow::Result<String> {
    let mut errors = Vec::new();

    if let Some(browser) = std::env::var_os("BROWSER") {
        let browser = browser.to_string_lossy().into_owned();
        for command in browser.split(':').filter(|command| !command.is_empty()) {
            match try_launch(browser_shell(command, url)) {
                Ok(()) => return Ok(format!("$BROWSER ({command})")),
                Err(error) => {
                    tracing::debug!(opener = command, %error, "$BROWSER entry failed");
                    errors.push(format!("{command}: {error}"));
                }
            }
        }
    }

    match open_with_portal(url) {
        Ok(()) => return Ok("XDG desktop portal".into()),
        Err(error) => {
            tracing::debug!(%error, "desktop portal OpenURI failed");
            errors.push(format!("portal: {error}"));
        }
    }

    for (command, args) in OPENERS {
        let mut process = host_env::host_command(command);
        process.args(*args).arg(url);
        match try_launch(process) {
            Ok(()) => return Ok((*command).to_string()),
            Err(error) => {
                tracing::debug!(opener = command, %error, "browser opener failed");
                errors.push(format!("{command}: {error}"));
            }
        }
    }

    let context = gio::AppLaunchContext::new();
    for var in host_env::host_vars() {
        match var {
            HostVar::Set(name, value) => context.setenv(name, value),
            HostVar::Unset(name) => context.unsetenv(name),
        }
    }
    match gio::AppInfo::launch_default_for_uri(url, Some(&context)) {
        Ok(()) => return Ok("GIO default handler".into()),
        Err(error) => errors.push(format!("gio: {error}")),
    }

    anyhow::bail!("could not open a browser ({})", errors.join("; "))
}

/// Asks `org.freedesktop.portal.OpenURI` to open `url`.
///
/// A successful call means the portal accepted the request; the browser is
/// then started by the host session, outside our environment.
fn open_with_portal(url: &str) -> anyhow::Result<()> {
    let connection = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)?;
    let options = glib::VariantDict::new(None);
    let parameters =
        glib::Variant::tuple_from_iter(["".to_variant(), url.to_variant(), options.end()]);
    connection.call_sync(
        Some("org.freedesktop.portal.Desktop"),
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.OpenURI",
        "OpenURI",
        Some(&parameters),
        Some(glib::VariantTy::new("(o)")?),
        gio::DBusCallFlags::NONE,
        PORTAL_TIMEOUT_MS,
        gio::Cancellable::NONE,
    )?;
    Ok(())
}

/// Spawns `command` and treats a quick non-zero exit as failure, so a broken
/// opener (for example `xdg-open` failing to find a handler, or crashing on a
/// library mismatch) falls through to the next candidate instead of silently
/// doing nothing.
fn try_launch(mut command: Command) -> std::io::Result<()> {
    let child = command.spawn()?;
    wait_briefly(child, OPENER_GRACE)
}

fn wait_briefly(mut child: Child, grace: Duration) -> std::io::Result<()> {
    let deadline = Instant::now() + grace;
    loop {
        if let Some(status) = child.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(std::io::Error::other(format!("exited with {status}")))
            };
        }
        if Instant::now() >= deadline {
            // Still running: most likely a browser started in the foreground.
            // Reap it in the background so it does not linger as a zombie.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Runs one `$BROWSER` entry. Entries may contain `%s` for the URL (the
/// convention used by `sensible-browser`); otherwise the URL is appended.
fn browser_shell(command: &str, url: &str) -> Command {
    let mut process = host_env::host_command("sh");
    process.args(["-c", &browser_script(command), "sh", url]);
    process
}

fn browser_script(command: &str) -> String {
    if command.contains("%s") {
        format!("exec {}", command.replace("%s", "\"$1\""))
    } else {
        format!("exec {command} \"$1\"")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_failure_is_reported() {
        let child = Command::new("sh").args(["-c", "exit 3"]).spawn().unwrap();
        assert!(wait_briefly(child, Duration::from_secs(5)).is_err());
    }

    #[test]
    fn quick_success_is_accepted() {
        let child = Command::new("true").spawn().unwrap();
        assert!(wait_briefly(child, Duration::from_secs(5)).is_ok());
    }

    #[test]
    fn long_running_process_is_accepted() {
        let child = Command::new("sleep").arg("2").spawn().unwrap();
        assert!(wait_briefly(child, Duration::from_millis(100)).is_ok());
    }

    #[test]
    fn browser_entries_get_the_url() {
        assert_eq!(browser_script("firefox"), "exec firefox \"$1\"");
        assert_eq!(
            browser_script("firefox --new-tab %s"),
            "exec firefox --new-tab \"$1\""
        );
    }

    #[test]
    fn browser_shell_passes_url_as_argument() {
        let output = browser_shell("printf %s", "https://example.com/?a=1&b=2")
            .output()
            .unwrap();
        assert_eq!(output.stdout, b"https://example.com/?a=1&b=2");
    }
}
