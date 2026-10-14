//! Facts about the system that help with bug reports: how the launcher is
//! installed, the distribution, kernel, graphics driver and the optional
//! launch tools. Everything is read from files or from the `--version`
//! output of host programs; nothing is sent anywhere.

use crate::host_env;
use std::{
    io::Read,
    path::Path,
    process::{ExitStatus, Stdio},
    time::{Duration, Instant},
};

/// How long a host program may run. A hung `glxinfo` must not leave the
/// dialog collecting forever.
const PROGRAM_TIMEOUT: Duration = Duration::from_secs(5);

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

/// A package manager that can say which package owns a file.
struct Manager {
    /// What the install is called in the report.
    kind: &'static str,
    /// Program and arguments that succeed when a package owns the file.
    owner_query: &'static str,
    owner_args: &'static [&'static str],
    /// Program whose `--version` names the manager's version.
    name: &'static str,
}

const MANAGERS: [Manager; 3] = [
    Manager {
        kind: "DEB",
        owner_query: "dpkg-query",
        owner_args: &["-S"],
        name: "dpkg",
    },
    Manager {
        kind: "RPM",
        owner_query: "rpm",
        owner_args: &["-qf"],
        name: "rpm",
    },
    Manager {
        kind: "Pacman",
        owner_query: "pacman",
        owner_args: &["-Qo"],
        name: "pacman",
    },
];

fn install_kind() -> Option<String> {
    // Not the `APPIMAGE` variable: every program started from an AppImage
    // application inherits it.
    if host_env::is_active() {
        return Some("AppImage".into());
    }
    if host_env::is_flatpak() {
        return Some("Flatpak".into());
    }
    let exe = std::env::current_exe().ok()?;
    if exe.starts_with("/nix/store") {
        return Some("Nix".into());
    }
    for manager in MANAGERS {
        if succeeds(manager.owner_query, manager.owner_args, &exe) {
            return Some(match first_line_of(manager.name, &["--version"]) {
                Some(version) => format!("{} · {version}", manager.kind),
                None => manager.kind.into(),
            });
        }
    }
    Some(shown_path(
        &exe,
        std::env::var_os("HOME").as_deref().map(Path::new),
    ))
}

/// `path` with the home directory written as `~`: the text ends up in public
/// bug reports and must not carry the user name.
fn shown_path(path: &Path, home: Option<&Path>) -> String {
    match home.filter(|home| !home.as_os_str().is_empty()) {
        Some(home) => match path.strip_prefix(home) {
            Ok(rest) => Path::new("~").join(rest).display().to_string(),
            Err(_) => path.display().to_string(),
        },
        None => path.display().to_string(),
    }
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

/// Renderer and version from `glxinfo -B` (mesa-utils). Without it the
/// installed Mesa package is named, which says what is installed, not what
/// the game renders with.
fn opengl() -> Option<String> {
    if let Some(value) = output_of("glxinfo", &["-B"]).and_then(|output| parse_glxinfo(&output)) {
        return Some(value);
    }
    ["libgl1-mesa-dri", "mesa-dri-drivers", "mesa", "Mesa-dri"]
        .into_iter()
        .find_map(|package| package_version(package).map(|version| format!("{package} {version}")))
}

/// `renderer · version` from the output of `glxinfo -B`.
fn parse_glxinfo(output: &str) -> Option<String> {
    let field = |key: &str| {
        output
            .lines()
            .find_map(|line| line.trim().strip_prefix(key))
            .map(|value| value.trim().to_string())
    };
    let renderer = field("OpenGL renderer string:");
    let version = field("OpenGL version string:");
    match (renderer, version) {
        (Some(renderer), Some(version)) => Some(format!("{renderer} · {version}")),
        (renderer, version) => renderer.or(version),
    }
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
    let (status, stdout) = run(program, args, None)?;
    status
        .success()
        .then(|| String::from_utf8_lossy(&stdout).into_owned())
}

fn succeeds(program: &str, args: &[&str], path: &Path) -> bool {
    run(program, args, Some(path)).is_some_and(|(status, _)| status.success())
}

/// Runs a host program for at most [`PROGRAM_TIMEOUT`] and returns its exit
/// status and stdout; `None` when it cannot be started or runs too long.
fn run(program: &str, args: &[&str], path: Option<&Path>) -> Option<(ExitStatus, Vec<u8>)> {
    let mut command = host_env::host_command(program);
    command.args(args);
    if let Some(path) = path {
        command.arg(path);
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });

    let deadline = Instant::now() + PROGRAM_TIMEOUT;
    let status = loop {
        match child.try_wait().ok()? {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    Some((status, reader.join().ok()?))
}

fn read_trimmed(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{os_release_field, parse_glxinfo, run, shown_path};
    use std::path::Path;

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

    #[test]
    fn reads_the_renderer_and_version_from_glxinfo() {
        let output = "name of display: :0\nOpenGL vendor string: AMD\n\
                      OpenGL renderer string: AMD Radeon 680M (radeonsi)\n\
                      OpenGL core profile version string: 4.6 (Core Profile) Mesa 24.0.9\n\
                      OpenGL version string: 4.6 (Compatibility Profile) Mesa 24.0.9\n";
        assert_eq!(
            parse_glxinfo(output).as_deref(),
            Some("AMD Radeon 680M (radeonsi) · 4.6 (Compatibility Profile) Mesa 24.0.9")
        );
        assert_eq!(
            parse_glxinfo("OpenGL renderer string: llvmpipe\n").as_deref(),
            Some("llvmpipe")
        );
        assert_eq!(parse_glxinfo("Error: unable to open display\n"), None);
    }

    #[test]
    fn the_home_directory_is_not_shown() {
        let home = Path::new("/home/alice");
        assert_eq!(
            shown_path(Path::new("/home/alice/build/launcher"), Some(home)),
            "~/build/launcher"
        );
        assert_eq!(
            shown_path(Path::new("/opt/launcher/bin/launcher"), Some(home)),
            "/opt/launcher/bin/launcher"
        );
        assert_eq!(
            shown_path(Path::new("/home/alice/launcher"), None),
            "/home/alice/launcher"
        );
    }

    #[test]
    fn a_program_that_runs_too_long_is_given_up_on() {
        let (status, output) = run("echo", &["hello"], None).unwrap();
        assert!(status.success());
        assert_eq!(String::from_utf8_lossy(&output).trim(), "hello");
        assert!(run("no-such-program-for-sysinfo", &[], None).is_none());
    }
}
