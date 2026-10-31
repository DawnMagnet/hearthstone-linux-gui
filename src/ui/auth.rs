use hearthstone_linux::{config::AppConfig, host_env, paths::AppPaths};
use std::path::{Path, PathBuf};

pub fn mark_logged_out(paths: &AppPaths, config: &mut AppConfig) -> anyhow::Result<()> {
    let token = paths.game_token();
    if token.exists() {
        std::fs::remove_file(&token)?;
    }

    preserve_install_metadata(paths, config);
    config.game_dir = Some(paths.game_dir.clone());
    config.logged_in = false;
    config.last_login_at = None;
    config.save(&paths.config_file)
}

pub fn sync_config_from_disk(paths: &AppPaths, config: &mut AppConfig) {
    if let Ok(saved) = AppConfig::load_or_default(&paths.config_file) {
        *config = saved;
    }
}

pub fn preserve_install_metadata(paths: &AppPaths, config: &mut AppConfig) {
    let Ok(saved) = AppConfig::load_or_default(&paths.config_file) else {
        return;
    };
    if saved.installed_version.is_some() {
        config.installed_version = saved.installed_version;
    }
    if saved.unity_version.is_some() {
        config.unity_version = saved.unity_version;
    }
}

pub fn ensure_auth_scheme_handlers() -> std::io::Result<()> {
    // Inside Flatpak the exported desktop file already claims the login URL
    // schemes on the host, and a helper written into the sandbox could not be
    // started by the host anyway.
    if host_env::is_flatpak() {
        return Ok(());
    }
    let exe = auth_handler_executable()?;
    let paths = AppPaths::discover().map_err(std::io::Error::other)?;
    let helper = install_auth_callback_helper(&paths, &exe)?;
    let Some(applications_dir) = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".local/share"))
        })
        .map(|data_home| data_home.join("applications"))
    else {
        return Ok(());
    };
    std::fs::create_dir_all(&applications_dir)?;

    let desktop_id = "io.github.hearthstone_linux_gui.auth-callback.desktop";
    let desktop_file = applications_dir.join(desktop_id);
    std::fs::write(&desktop_file, user_desktop_entry(&helper))?;
    make_executable(&desktop_file)?;

    let _ = host_env::host_command("update-desktop-database")
        .arg(&applications_dir)
        .status();
    for mime in [
        "x-scheme-handler/wtcg",
        "x-scheme-handler/blizzard-hearthstone",
        "x-scheme-handler/hearthstone-linux",
        "x-scheme-handler/hearthstone-linux-gui",
    ] {
        let _ = host_env::host_command("xdg-mime")
            .args(["default", desktop_id, mime])
            .status();
        let _ = host_env::host_command("gio")
            .args(["mime", mime, desktop_id])
            .status();
    }
    write_mimeapps_defaults(desktop_id)?;

    Ok(())
}

fn install_auth_callback_helper(paths: &AppPaths, exe: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(&paths.state_dir)?;
    std::fs::create_dir_all(&paths.log_dir)?;
    let helper = paths.state_dir.join("auth-callback-handler");
    // Earlier helpers logged the full callback URI, including its login token.
    match std::fs::remove_file(paths.log_dir.join("auth-callback.log")) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    hearthstone_linux::auth::write_private_file(
        &helper,
        format!(
            "#!/usr/bin/env sh\nset -eu\numask 077\nexec {} --auth-callback \"${{1:-}}\"\n",
            shell_quote_path(exe),
        )
        .as_bytes(),
    )
    .map_err(std::io::Error::other)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&helper)?.permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&helper, permissions)?;
    }

    Ok(helper)
}

fn user_desktop_entry(helper: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=hearthstone-linux-gui Login Callback\nExec={} %u\nIcon=io.github.hearthstone_linux_gui\nCategories=Game;\nMimeType=x-scheme-handler/wtcg;x-scheme-handler/blizzard-hearthstone;x-scheme-handler/hearthstone-linux;x-scheme-handler/hearthstone-linux-gui;\nNoDisplay=true\nTerminal=false\nStartupNotify=false\n",
        desktop_exec_arg(helper)
    )
}

fn desktop_exec_arg(path: &Path) -> String {
    let value = path.to_string_lossy().replace('%', "%%");
    if value.chars().any(char::is_whitespace) {
        format!("\"{value}\"")
    } else {
        value
    }
}

fn write_mimeapps_defaults(desktop_id: &str) -> std::io::Result<()> {
    let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    else {
        return Ok(());
    };
    std::fs::create_dir_all(&config_home)?;
    let path = config_home.join("mimeapps.list");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let mut output = Vec::new();
    let mut in_default = false;
    let mut saw_default = false;
    let schemes = [
        "x-scheme-handler/wtcg",
        "x-scheme-handler/blizzard-hearthstone",
        "x-scheme-handler/hearthstone-linux",
        "x-scheme-handler/hearthstone-linux-gui",
    ];

    for line in existing.lines() {
        if line.trim() == "[Default Applications]" {
            in_default = true;
            saw_default = true;
            output.push(line.to_string());
            continue;
        }
        if line.starts_with('[') && line.trim() != "[Default Applications]" {
            if in_default {
                for scheme in schemes {
                    output.push(format!("{scheme}={desktop_id};"));
                }
            }
            in_default = false;
            output.push(line.to_string());
            continue;
        }
        if in_default
            && schemes
                .iter()
                .any(|scheme| line.trim_start().starts_with(&format!("{scheme}=")))
        {
            continue;
        }
        output.push(line.to_string());
    }

    if in_default {
        for scheme in schemes {
            output.push(format!("{scheme}={desktop_id};"));
        }
    } else if !saw_default {
        if !output.is_empty() {
            output.push(String::new());
        }
        output.push("[Default Applications]".to_string());
        for scheme in schemes {
            output.push(format!("{scheme}={desktop_id};"));
        }
    }

    std::fs::write(path, format!("{}\n", output.join("\n")))
}

fn auth_handler_executable() -> std::io::Result<PathBuf> {
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        let appimage = PathBuf::from(appimage);
        if appimage.exists() {
            return Ok(appimage);
        }
    }

    std::env::current_exe()
}

/// Replaces the mode with `0o755`. [`crate::util::make_executable`] only adds
/// execute bits, which would leave a group-writable desktop file writable.
#[cfg(unix)]
fn make_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

fn shell_quote_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    let escaped = value.replace('\'', "'\\''");
    format!("'{escaped}'")
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn paths_in(root: &Path) -> AppPaths {
        AppPaths {
            config_dir: root.join("config"),
            data_dir: root.join("data"),
            cache_dir: root.join("cache"),
            state_dir: root.join("state"),
            config_file: root.join("config/config.toml"),
            game_dir: root.join("game"),
            ngdp_dir: root.join("cache/ngdp"),
            unity_cache_dir: root.join("cache/unity"),
            log_dir: root.join("state/logs"),
        }
    }

    #[test]
    fn helper_passes_the_callback_without_logging_or_evaluating_its_executable_path() {
        let root = tempfile::tempdir().unwrap();
        let paths = paths_in(root.path());
        paths.ensure().unwrap();
        let log = paths.log_dir.join("auth-callback.log");
        std::fs::write(&log, "old callback credential").unwrap();
        let exe = root
            .path()
            .join("launcher ' $(touch injected) `touch injected`");
        std::fs::write(&exe, "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$0.args\"\n").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o700)).unwrap();
        let helper = install_auth_callback_helper(&paths, &exe).unwrap();
        let uri =
            "blizzard-hearthstone://localhost:0/?ST=US-0123456789ABCDEFGHIJKLMNOPQRSTUV-1234567";
        let output = std::process::Command::new("/bin/sh")
            .arg(&helper)
            .arg(uri)
            .current_dir(root.path())
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let mut args_file = exe.as_os_str().to_owned();
        args_file.push(".args");
        assert_eq!(
            std::fs::read_to_string(PathBuf::from(args_file)).unwrap(),
            format!("--auth-callback\n{uri}\n")
        );
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
        assert!(!log.exists());
        assert!(!root.path().join("injected").exists());
        assert_eq!(
            std::fs::metadata(helper).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }

    #[test]
    fn helper_preserves_the_launcher_exit_status() {
        let root = tempfile::tempdir().unwrap();
        let paths = paths_in(root.path());
        let exe = root.path().join("launcher");
        std::fs::write(&exe, "#!/bin/sh\nexit 23\n").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o700)).unwrap();
        let helper = install_auth_callback_helper(&paths, &exe).unwrap();
        let status = std::process::Command::new("/bin/sh")
            .arg(helper)
            .arg("callback")
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(23));
    }
}
