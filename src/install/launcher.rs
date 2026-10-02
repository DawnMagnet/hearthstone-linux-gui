use super::compatibility;
use anyhow::{Context, Result};
use std::{
    collections::HashSet,
    env,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::{Child, Command},
};
use tracing::{debug, info, warn};

pub fn launch_game(game_dir: &Path, use_discrete_gpu: bool) -> Result<Child> {
    let exe = game_dir.join("Bin/Hearthstone.x86_64");
    anyhow::ensure!(exe.exists(), "{} does not exist", exe.display());
    anyhow::ensure!(
        game_dir.join("Bin/UnityPlayer.so").exists(),
        "UnityPlayer.so is missing; run Install / Update to repair the Unity runtime"
    );
    anyhow::ensure!(
        game_dir
            .join("Bin/Hearthstone_Data/MonoBleedingEdge/x86_64/libmonobdwgc-2.0.so")
            .exists(),
        "Unity Mono runtime is missing; run Install / Update to repair the Unity runtime"
    );
    anyhow::ensure!(game_dir.join("token").exists(), "login token is missing");
    anyhow::ensure!(
        game_dir.join("client.config").exists(),
        "client.config is missing"
    );
    compatibility::patch_corefoundation_imports(game_dir)?;
    ensure_bundled_interpreter(&exe);

    info!(
        exe = %exe.display(),
        game_dir = %game_dir.display(),
        use_discrete_gpu,
        "launching Hearthstone"
    );
    let library_path = game_library_path(game_dir);
    let graphics_env = graphics_env(use_discrete_gpu);
    debug!(ld_library_path = ?library_path, "configured game library path");
    if let Some(runner) = find_runtime_runner() {
        info!(runner = %runner.display(), "launching Hearthstone through runtime");
        let mut command = Command::new(&runner);
        command
            .arg(&exe)
            .current_dir(game_dir)
            .env("LD_LIBRARY_PATH", library_path)
            .envs(graphics_env);
        return command
            .spawn()
            .with_context(|| format!("failed to launch Hearthstone through {}", runner.display()));
    }

    debug!("no runtime runner configured; launching directly");
    let mut command = Command::new(exe);
    command
        .current_dir(game_dir)
        .env("LD_LIBRARY_PATH", library_path)
        .envs(graphics_env);
    command.spawn().context("failed to launch Hearthstone")
}

fn game_library_path(game_dir: &Path) -> OsString {
    let mut paths = Vec::from([
        game_dir.join("Bin"),
        game_dir.join("Bin/Hearthstone_Data/Plugins"),
        game_dir.join(
            "Bin/Hearthstone_Data/Plugins/System/Library/Frameworks/CoreFoundation.framework",
        ),
        game_dir.join("Bin/Hearthstone_Data/MonoBleedingEdge/x86_64"),
    ]);
    paths.extend(runtime_library_paths());
    push_existing(&mut paths, "/run/opengl-driver/lib");
    push_existing(&mut paths, "/run/current-system/sw/share/nix-ld/lib");
    push_existing(&mut paths, "/run/current-system/sw/lib");
    paths.extend(nix_ld_library_paths());
    if let Some(existing) = env::var_os("NIX_LD_LIBRARY_PATH") {
        paths.extend(env::split_paths(&existing));
    }
    if let Some(existing) = env::var_os("LD_LIBRARY_PATH") {
        paths.extend(env::split_paths(&existing));
    }

    dedupe_paths(&mut paths);
    std::env::join_paths(paths).unwrap_or_default()
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
fn ensure_bundled_interpreter(exe: &Path) {
    let Some(interpreter) = bundled_interpreter() else {
        return;
    };
    if !interpreter.exists() {
        warn!(
            interpreter = %interpreter.display(),
            "bundled ELF interpreter was configured but does not exist"
        );
        return;
    }

    let candidates = patchelf_candidates();
    if candidates.is_empty() {
        warn!("bundled ELF interpreter was configured but patchelf was not found");
        return;
    }

    if let Some(current) = print_interpreter(&candidates, exe) {
        if current == interpreter.to_string_lossy() {
            return;
        }
        if env::var_os("HEARTHSTONE_LINUX_FORCE_BUNDLED_INTERPRETER").is_none()
            && Path::new(&current).exists()
        {
            debug!(
                exe = %exe.display(),
                interpreter = %current,
                "Unity player ELF interpreter exists on this system"
            );
            return;
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
        match Command::new(patchelf)
            .arg("--set-interpreter")
            .arg(&interpreter)
            .arg(exe)
            .status()
        {
            Ok(status) if status.success() => return,
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
        match Command::new(patchelf)
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

fn find_in_path(command: &str) -> Option<PathBuf> {
    env::split_paths(&env::var_os("PATH")?).find_map(|dir| {
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
    if let Some(existing) = env::var_os(name) {
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
