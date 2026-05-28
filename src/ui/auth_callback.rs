use crate::ui::browser::shell_quote_path;
use hearthstone_linux::paths::AppPaths;
use std::path::{Path, PathBuf};

pub(crate) fn ensure_auth_scheme_handlers() -> std::io::Result<()> {
    let exe = auth_handler_executable()?;
    let paths = AppPaths::discover().map_err(std::io::Error::other)?;
    let helper = install_auth_callback_helper(&paths, &exe)?;
    let Some(applications_dir) = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
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

    let _ = std::process::Command::new("update-desktop-database")
        .arg(&applications_dir)
        .status();
    for mime in [
        "x-scheme-handler/wtcg",
        "x-scheme-handler/blizzard-hearthstone",
        "x-scheme-handler/hearthstone-linux",
        "x-scheme-handler/hearthstone-linux-gui",
    ] {
        let _ = std::process::Command::new("xdg-mime")
            .args(["default", desktop_id, mime])
            .status();
    }
    write_mimeapps_defaults(desktop_id)?;

    Ok(())
}

fn install_auth_callback_helper(paths: &AppPaths, exe: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(&paths.state_dir)?;
    std::fs::create_dir_all(&paths.log_dir)?;
    let helper = paths.state_dir.join("auth-callback-handler");
    let log = paths.log_dir.join("auth-callback.log");
    std::fs::write(
        &helper,
        format!(
            "#!/usr/bin/env sh\nset -u\nlog={}\nprintf '%s callback %s\\n' \"$(date -Is 2>/dev/null || date)\" \"$*\" >> \"$log\"\n{} --auth-callback \"${{1:-}}\" >> \"$log\" 2>&1\nstatus=$?\nprintf '%s exit %s\\n' \"$(date -Is 2>/dev/null || date)\" \"$status\" >> \"$log\"\nexit \"$status\"\n",
            shell_quote_path(&log),
            shell_quote_path(exe),
        ),
    )?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&helper)?.permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&helper, permissions)?;
    }

    Ok(helper)
}

fn user_desktop_entry(helper: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=hearthstone-linux-gui Login Callback\nExec=sh -c \"exec \\\"$1\\\" \\\"${{2:-}}\\\"\" hearthstone-linux-auth {} %u\nIcon=io.github.hearthstone_linux_gui\nCategories=Game;\nMimeType=x-scheme-handler/wtcg;x-scheme-handler/blizzard-hearthstone;x-scheme-handler/hearthstone-linux;x-scheme-handler/hearthstone-linux-gui;\nNoDisplay=true\nTerminal=false\nStartupNotify=false\n",
        desktop_exec_arg(helper)
    )
}

fn desktop_exec_arg(path: &Path) -> String {
    let value = path.to_string_lossy();
    if value
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '"' | '\\' | '$' | '`'))
    {
        shell_quote_path(path)
    } else {
        value.into_owned()
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
