//! Restores the host environment for processes spawned from the AppImage.
//!
//! `AppRun` points `XDG_DATA_DIRS`, `GIO_MODULE_DIR`, `GTK_PATH` and friends at
//! data and modules bundled inside the AppImage.  Every child process would
//! inherit them, so host programs such as `xdg-open`, `gio`, a browser or the
//! Unity player would load the bundled GLib/GTK modules into the host's
//! libraries.  Older AppImages even exported the bundled glibc in
//! `LD_LIBRARY_PATH`, which made those programs fail or crash silently on
//! hosts with a different glibc (issue #22).
//!
//! `AppRun` saves the original value of every variable it changes as
//! `HEARTHSTONE_LINUX_HOST_<NAME>` (or `HEARTHSTONE_LINUX_HOST_UNSET_<NAME>`
//! when it was unset) and sets `HEARTHSTONE_LINUX_APPIMAGE_ENV=1`.  This module
//! turns that back into the host environment for child processes.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

/// Variables `AppRun` overrides with AppImage-internal paths.
pub const OVERRIDDEN_VARS: &[&str] = &[
    "PATH",
    "LD_LIBRARY_PATH",
    "XDG_DATA_DIRS",
    "XKB_CONFIG_ROOT",
    "LIBGL_DRIVERS_PATH",
    "__EGL_VENDOR_LIBRARY_DIRS",
    "GI_TYPELIB_PATH",
    "GIO_MODULE_DIR",
    "GDK_PIXBUF_MODULE_FILE",
    "GSETTINGS_SCHEMA_DIR",
    "GTK_EXE_PREFIX",
    "GTK_PATH",
];

const MARKER: &str = "HEARTHSTONE_LINUX_APPIMAGE_ENV";

/// One variable as it should be seen by host processes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostVar {
    Set(&'static str, OsString),
    Unset(&'static str),
}

/// Returns true when running inside a Flatpak sandbox.
pub fn is_flatpak() -> bool {
    std::path::Path::new("/.flatpak-info").exists()
}

/// Returns true when running from an AppImage whose `AppRun` recorded the
/// host environment.
pub fn is_active() -> bool {
    std::env::var_os(MARKER).is_some()
}

/// The host value of every variable overridden by `AppRun`, or an empty list
/// when not running from such an AppImage.
pub fn host_vars() -> Vec<HostVar> {
    host_vars_from(|name| std::env::var_os(name))
}

fn host_vars_from(get: impl Fn(&str) -> Option<OsString>) -> Vec<HostVar> {
    if get(MARKER).is_none() {
        return Vec::new();
    }
    OVERRIDDEN_VARS
        .iter()
        .map(
            |&name| match get(&format!("HEARTHSTONE_LINUX_HOST_{name}")) {
                Some(value) => HostVar::Set(name, value),
                None if get(&format!("HEARTHSTONE_LINUX_HOST_UNSET_{name}")).is_some() => {
                    HostVar::Unset(name)
                }
                // Not recorded (older AppRun): drop the AppImage value rather
                // than leaking bundled paths, except for PATH which must exist.
                None if name == "PATH" => HostVar::Set(name, OsString::from(DEFAULT_PATH)),
                None => HostVar::Unset(name),
            },
        )
        .collect()
}

const DEFAULT_PATH: &str = "/usr/local/bin:/usr/bin:/bin";

/// The host value of a single overridden variable, falling back to the
/// current process value outside the AppImage.
pub fn host_var(name: &str) -> Option<OsString> {
    if !is_active() {
        return std::env::var_os(name);
    }
    host_vars()
        .into_iter()
        .find_map(|var| match var {
            HostVar::Set(var_name, value) if var_name == name => Some(Some(value)),
            HostVar::Unset(var_name) if var_name == name => Some(None),
            _ => None,
        })
        .unwrap_or_else(|| std::env::var_os(name))
}

/// Makes `command` run with the host environment instead of the AppImage one.
pub fn apply(command: &mut Command) -> &mut Command {
    for var in host_vars() {
        match var {
            HostVar::Set(name, value) => {
                command.env(name, value);
            }
            HostVar::Unset(name) => {
                command.env_remove(name);
            }
        }
    }
    command
}

/// `path` when it is a regular file with an execute bit set.
pub fn executable_file(path: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = path.metadata().ok()?;
    (metadata.is_file() && metadata.permissions().mode() & 0o111 != 0).then(|| path.to_path_buf())
}

/// Looks `command` up in the host `PATH`, so an AppImage's own `usr/bin`
/// is not mistaken for a system installation.
pub fn find_in_path(command: &str) -> Option<PathBuf> {
    std::env::split_paths(&host_var("PATH")?).find_map(|dir| executable_file(&dir.join(command)))
}

/// A `Command` for a host program, with the AppImage environment undone.
pub fn host_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    apply(&mut command);
    command
}

/// Spawns a host child (`env`) and prints the environment variables that
/// matter for issue #22: every name in [`OVERRIDDEN_VARS`], as the child
/// actually sees them after [`apply`].
///
/// Hidden CLI: `hearthstone-linux-gui --print-child-env`. Packaging smoke
/// uses this to prove AppImage children do not inherit bundled
/// `LD_LIBRARY_PATH` / GTK / GLib module paths.
pub fn print_child_env() -> anyhow::Result<()> {
    let mut command = host_command("env");
    let output = command
        .output()
        .map_err(|error| anyhow::anyhow!("failed to spawn host `env`: {error}"))?;
    if !output.status.success() {
        anyhow::bail!(
            "host `env` exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let mut child = std::collections::BTreeMap::<String, String>::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some((key, value)) = line.split_once('=') {
            child.insert(key.to_string(), value.to_string());
        }
    }

    println!("# hearthstone-linux-gui --print-child-env");
    println!("# host_env_active={}", if is_active() { "1" } else { "0" });
    if let Some(bundled) = std::env::var_os("HEARTHSTONE_LINUX_BUNDLED_LIB_DIR") {
        println!("# bundled_lib_dir={}", bundled.to_string_lossy());
    }
    for name in OVERRIDDEN_VARS {
        match child.get(*name) {
            Some(value) => println!("{name}={value}"),
            None => println!("{name}=<unset>"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup<'a>(map: &'a HashMap<&str, &str>) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| map.get(name).map(OsString::from)
    }

    #[test]
    fn inactive_without_marker() {
        let env = HashMap::from([("HEARTHSTONE_LINUX_HOST_PATH", "/usr/bin")]);
        assert!(host_vars_from(lookup(&env)).is_empty());
    }

    #[test]
    fn restores_recorded_and_unset_variables() {
        let env = HashMap::from([
            (MARKER, "1"),
            ("HEARTHSTONE_LINUX_HOST_PATH", "/usr/bin:/bin"),
            ("HEARTHSTONE_LINUX_HOST_UNSET_LD_LIBRARY_PATH", "1"),
            ("HEARTHSTONE_LINUX_HOST_XDG_DATA_DIRS", "/usr/share"),
        ]);
        let vars = host_vars_from(lookup(&env));
        assert!(vars.contains(&HostVar::Set("PATH", "/usr/bin:/bin".into())));
        assert!(vars.contains(&HostVar::Unset("LD_LIBRARY_PATH")));
        assert!(vars.contains(&HostVar::Set("XDG_DATA_DIRS", "/usr/share".into())));
        // Unrecorded variables are dropped instead of leaking bundled paths.
        assert!(vars.contains(&HostVar::Unset("GIO_MODULE_DIR")));
    }

    #[test]
    fn keeps_empty_host_values() {
        let env = HashMap::from([
            (MARKER, "1"),
            ("HEARTHSTONE_LINUX_HOST_LD_LIBRARY_PATH", ""),
        ]);
        let vars = host_vars_from(lookup(&env));
        assert!(vars.contains(&HostVar::Set("LD_LIBRARY_PATH", "".into())));
    }
}
