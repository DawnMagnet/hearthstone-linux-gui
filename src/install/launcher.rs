use super::compatibility;
use crate::{config::AppConfig, game_options, host_env, tr};
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
    let mut graphics_env = graphics_env(options.use_discrete_gpu);
    if vsync_disabled_in_options() {
        let overrides = vsync_off_env(|name| host_env::host_var(name).is_some());
        info!(
            variables = ?overrides.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            "options.txt switches VSync off; overriding the driver"
        );
        graphics_env.extend(overrides);
    }
    let wrappers = wrapper_chain(options, find_in_path);
    let runner = find_runtime_runner();
    debug!(ld_library_path = ?library_path, "configured game library path");
    match runner.as_ref() {
        Some(runner) => {
            info!(runner = %runner.display(), "launching Hearthstone through runtime")
        }
        None => debug!("no runtime runner configured; launching directly"),
    }

    let mut command = game_command(runner.as_deref(), &wrappers, &exe, &library_path);
    host_env::apply(&mut command);
    command.current_dir(game_dir).envs(graphics_env);
    redirect_output(&mut command, output_log);
    command.spawn().with_context(|| match runner.as_ref() {
        Some(runner) => format!("failed to launch Hearthstone through {}", runner.display()),
        None => "failed to launch Hearthstone".to_string(),
    })
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

/// A host program put in front of the game.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchTool {
    /// Feral GameMode.
    GameMode,
    /// The MangoHud overlay.
    MangoHud,
}

impl LaunchTool {
    /// Name of the wrapper program.
    pub fn program(self) -> &'static str {
        match self {
            Self::GameMode => "gamemoderun",
            Self::MangoHud => "mangohud",
        }
    }

    /// Whether the program is installed on the host.
    pub fn is_installed(self) -> bool {
        find_in_path(self.program()).is_some()
    }
}

/// The enabled wrappers in the order they are put in front of the game:
/// `gamemoderun mangohud <game>`. A wrapper that is enabled but not
/// installed is skipped, so a removed tool never prevents the launch.
fn wrapper_chain(options: LaunchOptions, find: impl Fn(&str) -> Option<PathBuf>) -> Vec<PathBuf> {
    [
        (LaunchTool::GameMode, options.use_gamemode),
        (LaunchTool::MangoHud, options.use_mangohud),
    ]
    .into_iter()
    .filter(|(_, enabled)| *enabled)
    .filter_map(|(tool, _)| {
        let path = find(tool.program());
        if path.is_none() {
            warn!(
                tool = tool.program(),
                "launch wrapper is enabled but not installed; skipping"
            );
        }
        path
    })
    .collect()
}

/// `[runner] [wrappers...] game`, as one command.
///
/// Without wrappers the game's `LD_LIBRARY_PATH` goes on the command itself.
/// With wrappers it must not: `gamemoderun` and `mangohud` are host scripts,
/// and the game's library directories (possibly with a bundled glibc) would
/// break the host shell they run in (issue #22). `env` hands the variable to
/// the game alone. The wrappers sit behind the runner so they run inside its
/// environment, where their preloaded libraries can be found.
fn game_command(
    runner: Option<&Path>,
    wrappers: &[PathBuf],
    exe: &Path,
    library_path: &OsStr,
) -> Command {
    let mut argv: Vec<OsString> = runner.into_iter().map(|path| path.into()).collect();
    if wrappers.is_empty() {
        argv.push(exe.into());
    } else {
        let mut library_var = OsString::from("LD_LIBRARY_PATH=");
        library_var.push(library_path);
        argv.extend(wrappers.iter().map(OsString::from));
        argv.extend(["env".into(), library_var, exe.into()]);
    }

    let mut argv = argv.into_iter();
    let program = argv.next().unwrap_or_else(|| exe.into());
    let mut command = Command::new(program);
    command.args(argv);
    if wrappers.is_empty() {
        command.env("LD_LIBRARY_PATH", library_path);
    }
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
    env::split_paths(&host_env::host_var("PATH")?)
        .find_map(|dir| executable_file(&dir.join(command)))
}

/// `path` itself when it is a regular file that can be executed.
fn executable_file(path: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = path.metadata().ok()?;
    (metadata.is_file() && metadata.permissions().mode() & 0o111 != 0).then(|| path.to_path_buf())
}

/// Whether the game's `options.txt` switches VSync off (`vsync=0`).
fn vsync_disabled_in_options() -> bool {
    game_options::options_path()
        .and_then(|path| game_options::GameOptions::load(&path).ok())
        .is_some_and(|options| !options.vsync())
}

/// Driver settings that turn VSync off, for when the game keeps it on
/// despite `vsync=0`: the game requests a swap interval itself, which these
/// override. Mesa OpenGL (`vblank_mode`), NVIDIA OpenGL
/// (`__GL_SYNC_TO_VBLANK`) and Mesa Vulkan (`MESA_VK_WSI_PRESENT_MODE`).
/// A variable the user has set already is left alone.
fn vsync_off_env(is_set: impl Fn(&str) -> bool) -> Vec<(&'static str, OsString)> {
    [
        ("vblank_mode", "0"),
        ("__GL_SYNC_TO_VBLANK", "0"),
        ("MESA_VK_WSI_PRESENT_MODE", "immediate"),
    ]
    .into_iter()
    .filter(|(name, _)| !is_set(name))
    .map(|(name, value)| (name, OsString::from(value)))
    .collect()
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
    #[test]
    fn vsync_off_env_keeps_what_the_user_set() {
        let names = |envs: Vec<(&'static str, std::ffi::OsString)>| {
            envs.into_iter().map(|(name, _)| name).collect::<Vec<_>>()
        };
        assert_eq!(
            names(super::vsync_off_env(|_| false)),
            [
                "vblank_mode",
                "__GL_SYNC_TO_VBLANK",
                "MESA_VK_WSI_PRESENT_MODE"
            ]
        );
        assert_eq!(
            names(super::vsync_off_env(|name| name == "vblank_mode")),
            ["__GL_SYNC_TO_VBLANK", "MESA_VK_WSI_PRESENT_MODE"]
        );
    }

    use super::*;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn builds_wrapper_chain_in_order_and_skips_missing_tools() {
        use std::path::PathBuf;

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

        let only_mangohud =
            |tool: &str| (tool == "mangohud").then(|| PathBuf::from("/bin/mangohud"));
        assert_eq!(
            wrapper_chain(both, only_mangohud),
            [PathBuf::from("/bin/mangohud")]
        );
        assert!(wrapper_chain(LaunchOptions::default(), installed).is_empty());
    }

    fn argv(command: &std::process::Command) -> Vec<String> {
        std::iter::once(command.get_program())
            .chain(command.get_args())
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    fn library_path_of(command: &std::process::Command) -> Option<String> {
        command
            .get_envs()
            .find(|(name, _)| *name == "LD_LIBRARY_PATH")
            .and_then(|(_, value)| value)
            .map(|value| value.to_string_lossy().into_owned())
    }

    #[test]
    fn game_command_keeps_the_library_path_on_the_game_only() {
        use super::game_command;
        use std::path::{Path, PathBuf};

        let exe = Path::new("/game/Hearthstone.x86_64");
        let runner = Path::new("/run/runtime");
        let wrappers = [
            PathBuf::from("/usr/bin/gamemoderun"),
            PathBuf::from("/usr/bin/mangohud"),
        ];
        let libs = std::ffi::OsStr::new("/game/Bin:/lib");

        // Without wrappers nothing changes: the variable is on the command.
        let plain = game_command(None, &[], exe, libs);
        assert_eq!(argv(&plain), ["/game/Hearthstone.x86_64"]);
        assert_eq!(library_path_of(&plain).as_deref(), Some("/game/Bin:/lib"));
        let runner_only = game_command(Some(runner), &[], exe, libs);
        assert_eq!(
            argv(&runner_only),
            ["/run/runtime", "/game/Hearthstone.x86_64"]
        );
        assert_eq!(
            library_path_of(&runner_only).as_deref(),
            Some("/game/Bin:/lib")
        );

        // With wrappers only the game gets it, through `env`.
        let direct = game_command(None, &wrappers, exe, libs);
        assert_eq!(
            argv(&direct),
            [
                "/usr/bin/gamemoderun",
                "/usr/bin/mangohud",
                "env",
                "LD_LIBRARY_PATH=/game/Bin:/lib",
                "/game/Hearthstone.x86_64"
            ]
        );
        assert_eq!(library_path_of(&direct), None);

        // Behind a runner the wrappers run inside the runner's environment.
        let through_runner = game_command(Some(runner), &wrappers, exe, libs);
        assert_eq!(
            argv(&through_runner),
            [
                "/run/runtime",
                "/usr/bin/gamemoderun",
                "/usr/bin/mangohud",
                "env",
                "LD_LIBRARY_PATH=/game/Bin:/lib",
                "/game/Hearthstone.x86_64"
            ]
        );
        assert_eq!(library_path_of(&through_runner), None);
    }

    #[test]
    fn finds_only_executable_files_in_path() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let tool = dir.path().join("tool");
        std::fs::write(&tool, "#!/bin/sh\n").unwrap();
        let find = |name: &str| super::executable_file(&dir.path().join(name));
        assert_eq!(find("tool"), None, "not executable yet");
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(find("tool"), Some(tool));
        std::fs::create_dir(dir.path().join("folder")).unwrap();
        assert_eq!(find("folder"), None, "directories are no tools");
        assert_eq!(find("missing"), None);
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
