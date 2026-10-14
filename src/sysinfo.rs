//! Facts about the system that help with bug reports: how the launcher is
//! installed, the distribution, kernel, graphics driver and the optional
//! launch tools. Everything is read from files or from the `--version`
//! output of host programs; nothing is sent anywhere.

use crate::host_env;
use std::{path::Path, process::Stdio};

/// One collected value. `None` means it could not be determined (or the
/// program is not installed); the UI shows that in words.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SystemInfo {
    pub launcher: String,
    /// Install kind and the version of its package manager, e.g.
    /// `DEB · dpkg 1.22.6`.
    pub install: Option<String>,
    pub os: Option<String>,
    pub kernel: Option<String>,
    /// Display server and desktop, e.g. `wayland · GNOME`.
    pub session: Option<String>,
    /// Kernel modules driving the graphics cards, e.g. `amdgpu`.
    pub gpu_driver: Option<String>,
    /// OpenGL renderer and version as the driver reports them; the version
    /// names the Mesa or NVIDIA driver release.
    pub opengl: Option<String>,
    pub gamemode: Option<String>,
    pub mangohud: Option<String>,
}

impl SystemInfo {
    /// Collects everything. Runs a few short host programs, so call it off
    /// the GTK main thread.
    pub fn collect() -> Self {
        Self {
            launcher: concat!("v", env!("CARGO_PKG_VERSION")).to_string(),
            install: install_kind(),
            os: os_name(),
            kernel: read_trimmed("/proc/sys/kernel/osrelease"),
            session: session(),
            gpu_driver: gpu_driver(),
            opengl: opengl(),
            gamemode: first_line_of("gamemoded", &["--version"])
                .or_else(|| package_version("gamemode")),
            mangohud: first_line_of("mangohud", &["--version"])
                .or_else(|| package_version("mangohud")),
        }
    }
}

fn install_kind() -> Option<String> {
    if host_env::is_active() || std::env::var_os("APPIMAGE").is_some() {
        return Some("AppImage".into());
    }
    if host_env::is_flatpak() {
        return Some("Flatpak".into());
    }
    let exe = std::env::current_exe().ok()?;
    if exe.starts_with("/nix/store") {
        return Some("Nix".into());
    }
    let owners: [(&str, &str, &[&str], &str, &[&str]); 3] = [
        ("DEB", "dpkg-query", &["-S"], "dpkg", &["--version"]),
        ("RPM", "rpm", &["-qf"], "rpm", &["--version"]),
        ("Pacman", "pacman", &["-Qo"], "pacman", &["--version"]),
    ];
    for (kind, query, args, manager, version_args) in owners {
        if succeeds(query, args, &exe) {
            return Some(match first_line_of(manager, version_args) {
                Some(version) => format!("{kind} · {version}"),
                None => kind.into(),
            });
        }
    }
    Some(exe.display().to_string())
}

fn os_name() -> Option<String> {
    // Inside the Flatpak the host's file is mounted under /run/host.
    [
        "/run/host/os-release",
        "/etc/os-release",
        "/usr/lib/os-release",
    ]
    .into_iter()
    .find_map(|path| std::fs::read_to_string(path).ok())
    .and_then(|text| os_release_field(&text, "PRETTY_NAME"))
}

fn os_release_field(text: &str, key: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
        .map(|value| value.trim().trim_matches('"').to_string())
        .filter(|value| !value.is_empty())
}

fn session() -> Option<String> {
    let parts: Vec<String> = ["XDG_SESSION_TYPE", "XDG_CURRENT_DESKTOP"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .filter(|value| !value.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// Names of the kernel modules bound to the DRM cards, e.g. `amdgpu` or
/// `i915, nvidia`, with the NVIDIA module version when it is loaded.
fn gpu_driver() -> Option<String> {
    let mut drivers: Vec<String> = std::fs::read_dir("/sys/class/drm")
        .ok()?
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("card") && !name.contains('-')
        })
        .filter_map(|entry| std::fs::read_link(entry.path().join("device/driver")).ok())
        .filter_map(|link| {
            link.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .collect();
    drivers.sort();
    drivers.dedup();
    if let Some(version) = read_trimmed("/sys/module/nvidia/version") {
        for driver in drivers.iter_mut().filter(|driver| *driver == "nvidia") {
            *driver = format!("nvidia {version}");
        }
    }
    (!drivers.is_empty()).then(|| drivers.join(", "))
}

/// Renderer and version from `glxinfo -B` (mesa-utils), else the installed
/// Mesa package.
fn opengl() -> Option<String> {
    if let Some(output) = output_of("glxinfo", &["-B"]) {
        let field = |key: &str| {
            output
                .lines()
                .find_map(|line| line.trim().strip_prefix(key))
                .map(|value| value.trim().to_string())
        };
        let renderer = field("OpenGL renderer string:");
        let version = field("OpenGL version string:");
        match (renderer, version) {
            (Some(renderer), Some(version)) => return Some(format!("{renderer} · {version}")),
            (renderer, version) => {
                if let Some(value) = renderer.or(version) {
                    return Some(value);
                }
            }
        }
    }
    ["libgl1-mesa-dri", "mesa-dri-drivers", "mesa", "Mesa-dri"]
        .into_iter()
        .find_map(package_version)
        .map(|version| format!("Mesa {version}"))
}

/// Version of an installed distribution package, from whichever package
/// manager is present.
fn package_version(package: &str) -> Option<String> {
    output_of("dpkg-query", &["-W", "-f", "${Version}", package])
        .or_else(|| output_of("rpm", &["-q", "--qf", "%{VERSION}", package]))
        .or_else(|| {
            output_of("pacman", &["-Q", package])
                .and_then(|line| line.split_whitespace().nth(1).map(str::to_owned))
        })
        .map(|version| version.trim().to_string())
        .filter(|version| !version.is_empty() && !version.contains("not installed"))
}

fn first_line_of(program: &str, args: &[&str]) -> Option<String> {
    output_of(program, args)?
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

/// Stdout of a host program that exited successfully.
fn output_of(program: &str, args: &[&str]) -> Option<String> {
    let output = host_env::host_command(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn succeeds(program: &str, args: &[&str], path: &Path) -> bool {
    host_env::host_command(program)
        .args(args)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn read_trimmed(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::os_release_field;

    #[test]
    fn reads_os_release_values() {
        let text = "NAME=\"Linux Mint\"\nPRETTY_NAME=\"Linux Mint 22.3\"\nID=linuxmint\n";
        assert_eq!(
            os_release_field(text, "PRETTY_NAME").as_deref(),
            Some("Linux Mint 22.3")
        );
        assert_eq!(os_release_field(text, "ID").as_deref(), Some("linuxmint"));
        assert_eq!(os_release_field(text, "VERSION_ID"), None);
    }
}
