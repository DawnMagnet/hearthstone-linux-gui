use super::compatibility;
use crate::{config::AppConfig, host_env, tr};
use anyhow::{Context, Result};
use std::{
    collections::HashSet,
    env,
    ffi::{OsStr, OsString},
    fs::File,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    time::Duration,
};
use tracing::{debug, info, warn};

/// A game that exits sooner than this after launch most likely crashed while
/// starting (missing library, loader mismatch, ...), so the UI reports why.
pub const QUICK_EXIT_THRESHOLD: Duration = Duration::from_secs(15);

/// Launches Hearthstone.
///
/// When `output_log` is given, the game's stdout and stderr are written to
/// that file, so dynamic-loader errors that happen before Unity opens
/// `Player.log` are not lost. Otherwise the game inherits the caller's stdio.
pub fn launch_game(
    game_dir: &Path,
    options: LaunchOptions,
    output_log: Option<&Path>,
) -> Result<Child> {
    let exe = game_dir.join("Bin/Hearthstone.x86_64");
    anyhow::ensure!(
        exe.exists(),
        tr!("game.missing_executable", path = exe.display())
    );
    anyhow::ensure!(
        game_dir.join("Bin/UnityPlayer.so").exists(),
        tr!("game.missing_unity_player")
    );
    anyhow::ensure!(
        game_dir
            .join("Bin/Hearthstone_Data/MonoBleedingEdge/x86_64/libmonobdwgc-2.0.so")
            .exists(),
        tr!("game.missing_mono")
    );
    anyhow::ensure!(game_dir.join("token").exists(), tr!("game.missing_token"));
    anyhow::ensure!(
        game_dir.join("client.config").exists(),
        tr!("game.missing_client_config")
    );
    compatibility::patch_corefoundation_imports(game_dir)?;
    let bundled_interpreter = ensure_bundled_interpreter(&exe);

    info!(
        exe = %exe.display(),
        game_dir = %game_dir.display(),
        ?options,
        bundled_interpreter,
        "launching Hearthstone"
    );
    let library_path = game_library_path(game_dir, bundled_interpreter);
    let graphics_env = graphics_env(options.use_discrete_gpu);
    let wrappers = wrapper_chain(options, find_in_path);
    debug!(ld_library_path = ?library_path, "configured game library path");
    if let Some(runner) = find_runtime_runner() {
        info!(runner = %runner.display(), "launching Hearthstone through runtime");
        let mut command = wrapped_command(&wrappers, &runner);
        host_env::apply(&mut command);
        command
            .arg(&exe)
            .current_dir(game_dir)
            .env("LD_LIBRARY_PATH", library_path)
            .envs(graphics_env);
        redirect_output(&mut command, output_log);
        return command
            .spawn()
            .with_context(|| format!("failed to launch Hearthstone through {}", runner.display()));
    }

    debug!("no runtime runner configured; launching directly");
    let mut command = wrapped_command(&wrappers, &exe);
    host_env::apply(&mut command);
    command
        .current_dir(game_dir)
        .env("LD_LIBRARY_PATH", library_path)
        .envs(graphics_env);
    redirect_output(&mut command, output_log);
    command.spawn().context("failed to launch Hearthstone")
}

/// Per-launch switches from the launcher settings.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LaunchOptions {
    pub use_discrete_gpu: bool,
    pub use_gamemode: bool,
    pub use_mangohud: bool,
}

impl From<&AppConfig> for LaunchOptions {
    fn from(config: &AppConfig) -> Self {
        Self {
            use_discrete_gpu: config.use_discrete_gpu,
            use_gamemode: config.use_gamemode,
            use_mangohud: config.use_mangohud,
        }
    }
}

/// Wrapper program of Feral GameMode.
pub const GAMEMODE: &str = "gamemoderun";
/// Wrapper program of the MangoHud overlay.
pub const MANGOHUD: &str = "mangohud";

/// Whether a wrapper program is installed on the host.
pub fn host_tool_available(tool: &str) -> bool {
    find_in_path(tool).is_some()
}

/// The enabled wrappers in the order they are put in front of the game:
/// `gamemoderun mangohud <game>`. A wrapper that is enabled but not
/// installed is skipped, so a removed tool never prevents the launch.
fn wrapper_chain(options: LaunchOptions, find: impl Fn(&str) -> Option<PathBuf>) -> Vec<PathBuf> {
    [
        (options.use_gamemode, GAMEMODE),
        (options.use_mangohud, MANGOHUD),
    ]
    .into_iter()
    .filter(|(enabled, _)| *enabled)
    .filter_map(|(_, tool)| {
        let path = find(tool);
        if path.is_none() {
            warn!(
                tool,
                "launch wrapper is enabled but not installed; skipping"
            );
        }
        path
    })
    .collect()
}

/// `wrappers... program`, as one command.
fn wrapped_command(wrappers: &[PathBuf], program: &Path) -> Command {
    let mut argv = wrappers.iter().map(PathBuf::as_path).chain([program]);
    let mut command = Command::new(argv.next().unwrap_or(program));
    command.args(argv);
    command
}

fn redirect_output(command: &mut Command, output_log: Option<&Path>) {
    let Some(path) = output_log else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match File::create(path).and_then(|file| Ok((file.try_clone()?, file))) {
        Ok((stdout, stderr)) => {
            command
                .stdin(Stdio::null())
                .stdout(Stdio::from(stdout))
                .stderr(Stdio::from(stderr));
        }
        Err(error) => warn!(
            path = %path.display(),
            %error,
            "could not create game output log; inheriting stdio"
        ),
    }
}

/// Unity's own log for Hearthstone (`~/.config/unity3d/<company>/<product>`).
pub fn player_log_path() -> Option<PathBuf> {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(config_home.join("unity3d/Blizzard Entertainment/Hearthstone/Player.log"))
}

/// Describes how the game process ended, e.g. `exit code 1` or
/// `signal 11 (SIGSEGV)`.
pub fn describe_exit_status(status: &ExitStatus) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return match signal_name(signal) {
                Some(name) => tr!("game.signal_named", signal = signal, name = name),
                None => tr!("game.signal", signal = signal),
            };
        }
    }
    match status.code() {
        Some(code) => tr!("game.exit_code", code = code),
        None => status.to_string(),
    }
}

fn signal_name(signal: i32) -> Option<&'static str> {
    Some(match signal {
        1 => "SIGHUP",
        2 => "SIGINT",
        4 => "SIGILL",
        6 => "SIGABRT",
        7 => "SIGBUS",
        8 => "SIGFPE",
        9 => "SIGKILL",
        11 => "SIGSEGV",
        13 => "SIGPIPE",
        15 => "SIGTERM",
        _ => return None,
    })
}

/// Returns a user-facing explanation when the game ended suspiciously: it
/// failed, or it exited within [`QUICK_EXIT_THRESHOLD`] of being launched.
/// Returns `None` for a normal exit after a regular play session.
pub fn unexpected_exit_message(
    status: &ExitStatus,
    elapsed: Duration,
    player_log: Option<&Path>,
    output_log: Option<&Path>,
) -> Option<String> {
    let quick = elapsed < QUICK_EXIT_THRESHOLD;
    if status.success() && !quick {
        return None;
    }

    let message = tr!(
        "game.exited",
        seconds = elapsed.as_secs(),
        status = describe_exit_status(status)
    );
    let logs: Vec<String> = [player_log, output_log]
        .into_iter()
        .flatten()
        .map(|path| path.display().to_string())
        .collect();
    if logs.is_empty() {
        return Some(message);
    }
    Some(tr!(
        "game.check_logs",
        message = message,
        logs = logs.join(&tr!("game.logs_and"))
    ))
}

/// Builds the game's `LD_LIBRARY_PATH`.
///
/// Libraries bundled with the launcher (AppImage glibc, GLib, X11, ...) are
/// only added when the game runs through the bundled ELF interpreter.  With
/// the host interpreter they must stay out: mixing the host `ld-linux` with a
/// bundled `libc.so.6` makes the game exit immediately (issue #22).
fn game_library_path(game_dir: &Path, bundled_interpreter: bool) -> OsString {
    let mut paths = Vec::from([
        game_dir.join("Bin"),
        game_dir.join("Bin/Hearthstone_Data/Plugins"),
        game_dir.join(
            "Bin/Hearthstone_Data/Plugins/System/Library/Frameworks/CoreFoundation.framework",
        ),
        game_dir.join("Bin/Hearthstone_Data/MonoBleedingEdge/x86_64"),
    ]);
    if bundled_interpreter {
        paths.extend(runtime_library_paths());
        paths.extend(bundled_library_paths());
    }
    push_existing(&mut paths, "/run/opengl-driver/lib");
    push_existing(&mut paths, "/run/current-system/sw/share/nix-ld/lib");
    push_existing(&mut paths, "/run/current-system/sw/lib");
    paths.extend(nix_ld_library_paths());
    if let Some(existing) = env::var_os("NIX_LD_LIBRARY_PATH") {
        paths.extend(env::split_paths(&existing));
    }
    if let Some(existing) = host_env::host_var("LD_LIBRARY_PATH") {
        paths.extend(env::split_paths(&existing));
    }

    if !bundled_interpreter {
        let bundled = bundled_library_paths();
        paths.retain(|path| !bundled.contains(path));
    }
    dedupe_paths(&mut paths);
    std::env::join_paths(paths).unwrap_or_default()
}

/// Library directories shipped inside the AppImage.
fn bundled_library_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(dir) = env::var_os("HEARTHSTONE_LINUX_BUNDLED_LIB_DIR") {
        paths.push(PathBuf::from(dir));
    }
    if let Some(runtime_dir) =
        bundled_interpreter().and_then(|path| path.parent().map(Path::to_path_buf))
    {
        paths.push(runtime_dir);
    }
    paths
}

fn runtime_library_paths() -> Vec<PathBuf> {
    let Some(runtime_dir) = env::var_os("HEARTHSTONE_LINUX_RUNTIME_DIR") else {
        return Vec::new();
    };

    env::split_paths(&runtime_dir)
        .flat_map(|path| [path.clone(), path.join("lib")])
        .filter(|path| path.exists())
        .collect()
}

fn nix_ld_library_paths() -> Vec<PathBuf> {
    let Some(flags) = env::var_os("NIX_LDFLAGS") else {
        return Vec::new();
    };
    flags
        .to_string_lossy()
        .split_whitespace()
        .filter_map(|flag| flag.strip_prefix("-L").map(PathBuf::from))
        .collect()
}

fn find_runtime_runner() -> Option<PathBuf> {
    if env::var_os("HEARTHSTONE_LINUX_DIRECT_LAUNCH").is_some() {
        return None;
    }
    if let Some(runner) = env::var_os("HEARTHSTONE_LINUX_RUNNER") {
        let runner = PathBuf::from(runner);
        if runner.exists() {
            return Some(runner);
        }
    }

    find_in_path("hearthstone-linux-gui-runtime")
}

/// Points the game binary at the bundled ELF interpreter when its current one
/// cannot work on this system.
///
/// Patching is best-effort by design: a broken or missing `patchelf` (for
/// example the bundled Nix build aborting with a stack smash on some hosts,
/// issue #9) must never prevent the game from launching, because the binary's
/// existing interpreter is usually fine.  When inspection fails, system
/// `patchelf` is preferred over the bundled one, which is only tried first
/// because it is known to match the bundled runtime.
///
/// Returns true when the game binary ends up using the bundled interpreter,
/// in which case it also needs the bundled libraries on its library path.
fn ensure_bundled_interpreter(exe: &Path) -> bool {
    let Some(interpreter) = bundled_interpreter() else {
        return false;
    };
    if !interpreter.exists() {
        warn!(
            interpreter = %interpreter.display(),
            "bundled ELF interpreter was configured but does not exist"
        );
        return false;
    }

    let candidates = patchelf_candidates();
    if candidates.is_empty() {
        warn!("bundled ELF interpreter was configured but patchelf was not found");
        return false;
    }

    if let Some(current) = print_interpreter(&candidates, exe) {
        if current == interpreter.to_string_lossy() {
            return true;
        }
        if env::var_os("HEARTHSTONE_LINUX_FORCE_BUNDLED_INTERPRETER").is_none()
            && Path::new(&current).exists()
        {
            debug!(
                exe = %exe.display(),
                interpreter = %current,
                "Unity player ELF interpreter exists on this system"
            );
            return false;
        }
    } else {
        warn!(
            exe = %exe.display(),
            "could not inspect the game binary's ELF interpreter; \
             attempting to set the bundled interpreter anyway"
        );
    }

    info!(
        exe = %exe.display(),
        interpreter = %interpreter.display(),
        "patching Unity player ELF interpreter"
    );
    for patchelf in &candidates {
        match host_env::host_command(patchelf)
            .arg("--set-interpreter")
            .arg(&interpreter)
            .arg(exe)
            .status()
        {
            Ok(status) if status.success() => return true,
            Ok(status) => warn!(
                patchelf = %patchelf.display(),
                %status,
                "patchelf failed to set the interpreter"
            ),
            Err(error) => warn!(
                patchelf = %patchelf.display(),
                %error,
                "failed to run patchelf"
            ),
        }
    }
    warn!(
        exe = %exe.display(),
        "could not rewrite the ELF interpreter; launching with the existing one"
    );
    false
}

fn bundled_interpreter() -> Option<PathBuf> {
    env::var_os("HEARTHSTONE_LINUX_ELF_INTERPRETER")
        .map(PathBuf::from)
        .or_else(|| {
            let runtime_dir = env::var_os("HEARTHSTONE_LINUX_RUNTIME_DIR")?;
            Some(PathBuf::from(runtime_dir).join("ld-linux-x86-64.so.2"))
        })
}

/// patchelf binaries to try, most preferred first: the configured one (the
/// AppImage ships one matching its bundled runtime), then any system one.
fn patchelf_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("HEARTHSTONE_LINUX_PATCHELF") {
        let path = PathBuf::from(path);
        if path.exists() {
            candidates.push(path);
        }
    }
    if let Some(path) = find_in_path("patchelf") {
        if !candidates.contains(&path) {
            candidates.push(path);
        }
    }
    candidates
}

/// Returns the interpreter reported by the first patchelf candidate that can
/// inspect the binary, or `None` when every candidate fails.
fn print_interpreter(candidates: &[PathBuf], exe: &Path) -> Option<String> {
    for patchelf in candidates {
        match host_env::host_command(patchelf)
            .arg("--print-interpreter")
            .arg(exe)
            .output()
        {
            Ok(output) if output.status.success() => {
                let interpreter = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !interpreter.is_empty() {
                    return Some(interpreter);
                }
                debug!(
                    patchelf = %patchelf.display(),
                    "patchelf printed no ELF interpreter"
                );
            }
            Ok(output) => debug!(
                patchelf = %patchelf.display(),
                status = %output.status,
                "patchelf could not inspect the interpreter"
            ),
            Err(error) => debug!(
                patchelf = %patchelf.display(),
                %error,
                "failed to run patchelf"
            ),
        }
    }
    None
}

/// Looks `command` up in the host `PATH`, so the AppImage's own `usr/bin`
/// is not mistaken for a system installation.
fn find_in_path(command: &str) -> Option<PathBuf> {
    env::split_paths(&host_env::host_var("PATH")?).find_map(|dir| {
        let path = dir.join(command);
        path.exists().then_some(path)
    })
}

fn graphics_env(use_discrete_gpu: bool) -> Vec<(&'static str, OsString)> {
    let mut envs = Vec::new();
    push_env_path(
        &mut envs,
        "LIBGL_DRIVERS_PATH",
        OsStr::new("/run/opengl-driver/lib/dri"),
    );
    push_env_path(
        &mut envs,
        "__EGL_VENDOR_LIBRARY_DIRS",
        OsStr::new("/run/opengl-driver/share/glvnd/egl_vendor.d"),
    );
    if use_discrete_gpu {
        envs.extend([
            ("DRI_PRIME", OsString::from("1")),
            ("__NV_PRIME_RENDER_OFFLOAD", OsString::from("1")),
            ("__GLX_VENDOR_LIBRARY_NAME", OsString::from("nvidia")),
            ("__VK_LAYER_NV_optimus", OsString::from("NVIDIA_only")),
        ]);
    }
    envs
}

fn push_env_path(envs: &mut Vec<(&'static str, OsString)>, name: &'static str, path: &OsStr) {
    let path = Path::new(path);
    if !path.exists() {
        return;
    }

    let mut paths = vec![path.to_path_buf()];
    if let Some(existing) = host_env::host_var(name) {
        paths.extend(env::split_paths(&existing));
    }
    dedupe_paths(&mut paths);
    if let Ok(joined) = env::join_paths(paths) {
        envs.push((name, joined));
    }
}

fn push_existing(paths: &mut Vec<PathBuf>, path: impl AsRef<Path>) {
    let path = path.as_ref();
    if path.exists() {
        paths.push(path.to_path_buf());
    }
}

fn dedupe_paths(paths: &mut Vec<PathBuf>) {
    let mut seen = HashSet::new();
    paths.retain(|path| seen.insert(path.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn builds_wrapper_chain_in_order_and_skips_missing_tools() {
        use super::{wrapped_command, wrapper_chain, LaunchOptions};
        use std::path::{Path, PathBuf};

        let installed = |tool: &str| Some(PathBuf::from("/usr/bin").join(tool));
        let both = LaunchOptions {
            use_gamemode: true,
            use_mangohud: true,
            ..LaunchOptions::default()
        };
        let chain = wrapper_chain(both, installed);
        assert_eq!(
            chain,
            [
                PathBuf::from("/usr/bin/gamemoderun"),
                PathBuf::from("/usr/bin/mangohud")
            ]
        );

        let command = wrapped_command(&chain, Path::new("/game/Hearthstone.x86_64"));
        assert_eq!(command.get_program(), "/usr/bin/gamemoderun");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["/usr/bin/mangohud", "/game/Hearthstone.x86_64"]
        );

        let only_mangohud =
            |tool: &str| (tool == "mangohud").then(|| PathBuf::from("/bin/mangohud"));
        assert_eq!(
            wrapper_chain(both, only_mangohud),
            [PathBuf::from("/bin/mangohud")]
        );
        assert!(wrapper_chain(LaunchOptions::default(), installed).is_empty());

        let plain = wrapped_command(&[], Path::new("/game/Hearthstone.x86_64"));
        assert_eq!(plain.get_program(), "/game/Hearthstone.x86_64");
        assert_eq!(plain.get_args().count(), 0);
    }

    #[test]
    fn describes_exit_codes_and_signals() {
        assert_eq!(
            describe_exit_status(&ExitStatus::from_raw(1 << 8)),
            "exit code 1"
        );
        assert_eq!(
            describe_exit_status(&ExitStatus::from_raw(11)),
            "signal 11 (SIGSEGV)"
        );
        assert_eq!(describe_exit_status(&ExitStatus::from_raw(42)), "signal 42");
    }

    #[test]
    fn quick_exit_is_reported_with_logs() {
        let message = unexpected_exit_message(
            &ExitStatus::from_raw(0),
            Duration::from_secs(1),
            Some(Path::new("/tmp/Player.log")),
            Some(Path::new("/tmp/game.log")),
        )
        .unwrap();
        assert_eq!(
            message,
            "Game exited after 1s (exit code 0). Check /tmp/Player.log and /tmp/game.log"
        );
    }

    #[test]
    fn failure_after_long_session_is_reported() {
        let message = unexpected_exit_message(
            &ExitStatus::from_raw(6),
            Duration::from_secs(600),
            None,
            None,
        )
        .unwrap();
        assert_eq!(message, "Game exited after 600s (signal 6 (SIGABRT))");
    }

    #[test]
    fn normal_exit_after_long_session_is_quiet() {
        assert!(unexpected_exit_message(
            &ExitStatus::from_raw(0),
            Duration::from_secs(600),
            None,
            None
        )
        .is_none());
    }
}
