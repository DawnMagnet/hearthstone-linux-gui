//! Runs the optional login window, a separate program
//! (`hearthstone-linux-gui-login`, see `login-helper/`).
//!
//! It is only used when it is installed: the launcher itself never depends on
//! WebKitGTK. The window reports the login callback on stdout; whenever it is
//! missing, fails, or the user asks for it, the launcher falls back to the
//! system browser.

use hearthstone_linux::{host_env, i18n, tr};
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

/// Program name of the login window.
const PROGRAM: &str = "hearthstone-linux-gui-login";
/// Overrides where the program is looked up, for development builds.
const PROGRAM_ENV: &str = "HEARTHSTONE_LINUX_LOGIN_WINDOW";

const EXIT_OPEN_BROWSER: i32 = 3;
const EXIT_CLOSED: i32 = 1;

/// How the login window ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    /// The login finished; the redirect URI carries the token.
    Callback(String),
    /// The user asked for the system browser.
    OpenBrowser,
    /// The window was closed without a login.
    Closed,
    /// The window could not run; the browser is used instead.
    Failed(String),
}

/// The installed login window, if any: next to the launcher's executable
/// first, then on the host `PATH`.
pub fn find() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(PROGRAM_ENV) {
        return executable(Path::new(&path));
    }
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(PROGRAM)));
    beside.and_then(|path| executable(&path)).or_else(|| {
        std::env::split_paths(&host_env::host_var("PATH")?)
            .find_map(|dir| executable(&dir.join(PROGRAM)))
    })
}

fn executable(path: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = path.metadata().ok()?;
    (metadata.is_file() && metadata.permissions().mode() & 0o111 != 0).then(|| path.to_path_buf())
}

/// Shows the login window for `url` and waits for it. Cancelling `cancel`
/// closes the window; the result is then `Closed`. Blocks, so call it off
/// the GTK main thread.
pub fn run(program: &Path, url: &str, cancel: &CancellationToken) -> Outcome {
    let mut command = Command::new(program);
    host_env::apply(&mut command);
    // The login window has no catalogs of its own; it gets the language and
    // its texts from here.
    command
        .arg(url)
        .env("HEARTHSTONE_LOGIN_LANGUAGE", i18n::current())
        .env("HEARTHSTONE_LOGIN_TITLE", tr!("login_window.title"))
        .env(
            "HEARTHSTONE_LOGIN_OPEN_BROWSER",
            tr!("login_window.open_browser"),
        )
        .env(
            "HEARTHSTONE_LOGIN_OPEN_BROWSER_TOOLTIP",
            tr!("login_window.open_browser_tooltip"),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return Outcome::Failed(format!("{}: {error}", program.display())),
    };

    let status = loop {
        if cancel.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Outcome::Closed;
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(error) => return Outcome::Failed(error.to_string()),
        }
    };
    let mut stdout = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        let _ = pipe.read_to_string(&mut stdout);
    }
    outcome(status, &stdout)
}

/// Interprets the exit status and output of the login window.
fn outcome(status: ExitStatus, stdout: &str) -> Outcome {
    if status.success() {
        return stdout
            .lines()
            .find_map(|line| line.strip_prefix("callback "))
            .map(|uri| Outcome::Callback(uri.trim().to_string()))
            .unwrap_or_else(|| Outcome::Failed("no login callback reported".into()));
    }
    match status.code() {
        Some(EXIT_OPEN_BROWSER) => Outcome::OpenBrowser,
        Some(EXIT_CLOSED) => Outcome::Closed,
        _ => Outcome::Failed(status.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{outcome, Outcome};
    use std::{os::unix::process::ExitStatusExt, process::ExitStatus};

    fn exit(code: i32) -> ExitStatus {
        ExitStatus::from_raw(code << 8)
    }

    #[test]
    fn reads_the_result_of_the_login_window() {
        assert_eq!(
            outcome(
                exit(0),
                "callback blizzard-hearthstone://localhost:0/?ST=EU-x-1\n"
            ),
            Outcome::Callback("blizzard-hearthstone://localhost:0/?ST=EU-x-1".into())
        );
        assert!(matches!(outcome(exit(0), ""), Outcome::Failed(_)));
        assert_eq!(outcome(exit(3), ""), Outcome::OpenBrowser);
        assert_eq!(outcome(exit(1), ""), Outcome::Closed);
        assert!(matches!(outcome(exit(2), ""), Outcome::Failed(_)));
    }
}
