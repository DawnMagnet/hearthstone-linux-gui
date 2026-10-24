//! Runs the optional login window, a separate program
//! (`hearthstone-linux-gui-login`, see `login-helper/`).
//!
//! It is only used when it is installed: the launcher itself never depends on
//! WebKitGTK. The window reports the login callback on stdout; whenever it is
//! missing, fails, or the user asks for it, the launcher falls back to the
//! system browser.

use hearthstone_linux::{host_env, i18n, tr};
use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread::JoinHandle,
    time::Duration,
};
use tokio_util::sync::CancellationToken;

/// Program name of the login window.
const PROGRAM: &str = "hearthstone-linux-gui-login";
/// Overrides where the program is looked up, for development builds.
const PROGRAM_ENV: &str = "HEARTHSTONE_LINUX_LOGIN_WINDOW";

/// URL schemes the login window may report after a successful login.
const CALLBACK_SCHEMES: [&str; 2] = ["blizzard-hearthstone:", "wtcg:"];

/// Upper bound of the login window output that is kept. The rest is drained,
/// not buffered, so a broken helper cannot block or exhaust memory.
const MAX_OUTPUT: usize = 64 * 1024;

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

    // Drain stdout while the window is open. Reading only after the process
    // exits would deadlock a helper that writes more than a pipe buffer.
    let reader = child
        .stdout
        .take()
        .map(|stdout| std::thread::spawn(move || read_bounded(stdout)));

    let status = match wait_for(&mut child, cancel) {
        Ok(Some(status)) => status,
        Ok(None) => {
            let _ = join_reader(reader);
            return Outcome::Closed;
        }
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = join_reader(reader);
            return Outcome::Failed(error.to_string());
        }
    };
    outcome(status, &join_reader(reader))
}

/// Waits for the login window to exit. `None` means `cancel` killed it.
fn wait_for(child: &mut Child, cancel: &CancellationToken) -> io::Result<Option<ExitStatus>> {
    loop {
        if cancel.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        match child.try_wait()? {
            Some(status) => return Ok(Some(status)),
            None => std::thread::sleep(Duration::from_millis(200)),
        }
    }
}

/// Reads all of `reader` but keeps only the first [`MAX_OUTPUT`] bytes.
fn read_bounded(mut reader: impl Read) -> String {
    let mut kept = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                let room = MAX_OUTPUT.saturating_sub(kept.len());
                kept.extend_from_slice(&buffer[..read.min(room)]);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&kept).into_owned()
}

fn join_reader(reader: Option<JoinHandle<String>>) -> String {
    reader
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default()
}

/// Interprets the exit status and output of the login window.
fn outcome(status: ExitStatus, stdout: &str) -> Outcome {
    if status.success() {
        return stdout
            .lines()
            .find_map(|line| line.strip_prefix("callback "))
            .map(str::trim)
            .filter(|uri| is_callback_uri(uri))
            .map(str::to_string)
            .map(Outcome::Callback)
            .unwrap_or_else(|| Outcome::Failed("no valid login callback reported".into()));
    }
    match status.code() {
        Some(EXIT_OPEN_BROWSER) => Outcome::OpenBrowser,
        Some(EXIT_CLOSED) => Outcome::Closed,
        _ => Outcome::Failed(status.to_string()),
    }
}

fn is_callback_uri(uri: &str) -> bool {
    CALLBACK_SCHEMES.iter().any(|scheme| {
        uri.get(..scheme.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(scheme))
    })
}

#[cfg(test)]
mod tests {
    use super::{outcome, read_bounded, run, Outcome, MAX_OUTPUT};
    use std::{
        fs,
        io::Cursor,
        os::unix::{fs::PermissionsExt, process::ExitStatusExt},
        path::PathBuf,
        process::ExitStatus,
        time::{Duration, Instant},
    };
    use tokio_util::sync::CancellationToken;

    const URL: &str = "https://battle.net/login/?app=wtcg";

    fn exit(code: i32) -> ExitStatus {
        ExitStatus::from_raw(code << 8)
    }

    fn helper_script(body: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hearthstone-linux-gui-login");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        (dir, path)
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
        assert_eq!(
            outcome(exit(0), "callback WTCG://localhost:0/?ST=EU-x-1\n"),
            Outcome::Callback("WTCG://localhost:0/?ST=EU-x-1".into())
        );
        assert!(matches!(outcome(exit(0), ""), Outcome::Failed(_)));
        assert!(matches!(
            outcome(exit(0), "callback https://example.com/?ST=EU-x-1\n"),
            Outcome::Failed(_)
        ));
        assert_eq!(outcome(exit(3), ""), Outcome::OpenBrowser);
        assert_eq!(outcome(exit(1), ""), Outcome::Closed);
        assert!(matches!(outcome(exit(2), ""), Outcome::Failed(_)));
    }

    #[test]
    fn keeps_only_the_first_part_of_the_output() {
        let output = read_bounded(Cursor::new(vec![b'x'; MAX_OUTPUT + 1024]));
        assert_eq!(output.len(), MAX_OUTPUT);
    }

    #[test]
    fn runs_a_helper_and_reads_its_callback() {
        let (_dir, program) = helper_script(
            "printf '%s\\n' 'callback blizzard-hearthstone://localhost:0/?ST=EU-x-1'",
        );
        assert_eq!(
            run(&program, URL, &CancellationToken::new()),
            Outcome::Callback("blizzard-hearthstone://localhost:0/?ST=EU-x-1".into())
        );
    }

    #[test]
    fn passes_the_helper_exit_codes_through() {
        let (_dir, program) = helper_script("exit 3");
        assert_eq!(
            run(&program, URL, &CancellationToken::new()),
            Outcome::OpenBrowser
        );

        let (_dir, program) = helper_script("exit 1");
        assert_eq!(
            run(&program, URL, &CancellationToken::new()),
            Outcome::Closed
        );
    }

    #[test]
    fn cancelling_kills_the_helper() {
        let (_dir, program) = helper_script("exec sleep 30");
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        let waiter = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            trigger.cancel();
        });

        let started = Instant::now();
        assert_eq!(run(&program, URL, &cancel), Outcome::Closed);
        waiter.join().unwrap();
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn drains_output_that_does_not_fit_in_a_pipe() {
        let (_dir, program) = helper_script("head -c 1048576 /dev/zero");
        let cancel = CancellationToken::new();
        let worker_cancel = cancel.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _ = sender.send(run(&program, URL, &worker_cancel));
        });

        let started = Instant::now();
        let received = receiver.recv_timeout(Duration::from_secs(5));
        let elapsed = started.elapsed();
        if received.is_err() {
            cancel.cancel();
        }
        worker.join().unwrap();

        let outcome = received
            .unwrap_or_else(|error| panic!("the login window output was not drained: {error}"));
        assert!(matches!(outcome, Outcome::Failed(_)), "{outcome:?}");
        assert!(elapsed < Duration::from_secs(4), "took {elapsed:?}");
    }
}
