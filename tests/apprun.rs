//! Checks the environment `packaging/appimage/AppRun` hands to the launcher.
//!
//! The AppImage never bundles glibc or the dynamic loader and must never
//! export `LD_LIBRARY_PATH` (issue #22): every host program the launcher
//! starts would inherit it. Every variable AppRun changes must be recorded so
//! `host_env` can restore it for child processes.

use std::{collections::HashMap, fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

use hearthstone_linux::host_env::OVERRIDDEN_VARS;

const APPRUN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/packaging/appimage/AppRun");

fn write_script(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// Builds a fake AppDir whose "launcher" dumps its environment.
fn fake_appdir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::copy(APPRUN, root.join("AppRun")).unwrap();
    fs::set_permissions(root.join("AppRun"), fs::Permissions::from_mode(0o755)).unwrap();
    write_script(
        &root.join("usr/bin/hearthstone-linux-gui"),
        "#!/bin/sh\nenv > \"$ENV_DUMP\"\n",
    );
    let loaders = root.join("usr/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache.in");
    fs::create_dir_all(loaders.parent().unwrap()).unwrap();
    fs::write(
        &loaders,
        "\"@APPDIR@/usr/lib/gdk-pixbuf-2.0/2.10.0/loaders/libpixbufloader-svg.so\"\n",
    )
    .unwrap();
    dir
}

fn run(appdir: &Path, env: &[(&str, &str)]) -> HashMap<String, String> {
    let env_dump = appdir.join("env.txt");
    let runtime = appdir.join("runtime");
    fs::create_dir_all(&runtime).unwrap();
    let mut command = Command::new(appdir.join("AppRun"));
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("APPDIR", appdir)
        .env("ENV_DUMP", &env_dump)
        .env("XDG_RUNTIME_DIR", &runtime)
        .envs(env.iter().copied());
    let status = command.status().unwrap();
    assert!(status.success(), "AppRun failed: {status}");
    fs::read_to_string(env_dump)
        .unwrap()
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[test]
fn library_path_is_left_alone() {
    let appdir = fake_appdir();
    let vars = run(appdir.path(), &[("LD_LIBRARY_PATH", "/host/lib")]);
    assert_eq!(
        vars.get("LD_LIBRARY_PATH").map(String::as_str),
        Some("/host/lib")
    );
    assert_eq!(
        vars.get("HEARTHSTONE_LINUX_HOST_LD_LIBRARY_PATH")
            .map(String::as_str),
        Some("/host/lib")
    );
    assert_eq!(vars.get("PATH").map(String::as_str), Some("/usr/bin:/bin"));
    assert_eq!(
        vars.get("HEARTHSTONE_LINUX_APPIMAGE_ENV")
            .map(String::as_str),
        Some("1")
    );
    // No bundled loader, no interpreter rewriting of the game.
    assert!(!vars.contains_key("HEARTHSTONE_LINUX_ELF_INTERPRETER"));
    assert!(!vars.contains_key("HEARTHSTONE_LINUX_RUNTIME_DIR"));
}

#[test]
fn unset_library_path_stays_unset() {
    let appdir = fake_appdir();
    let vars = run(appdir.path(), &[]);
    assert!(!vars.contains_key("LD_LIBRARY_PATH"));
    assert_eq!(
        vars.get("HEARTHSTONE_LINUX_HOST_UNSET_LD_LIBRARY_PATH")
            .map(String::as_str),
        Some("1")
    );
}

#[test]
fn every_overridden_variable_is_recorded() {
    let appdir = fake_appdir();
    let vars = run(appdir.path(), &[("XDG_DATA_DIRS", "/host/share")]);
    for name in OVERRIDDEN_VARS {
        let recorded = vars.contains_key(&format!("HEARTHSTONE_LINUX_HOST_{name}"))
            || vars.contains_key(&format!("HEARTHSTONE_LINUX_HOST_UNSET_{name}"));
        assert!(recorded, "{name} is not recorded by AppRun");
    }
    assert_eq!(
        vars.get("HEARTHSTONE_LINUX_HOST_XDG_DATA_DIRS")
            .map(String::as_str),
        Some("/host/share")
    );

    // Everything AppRun points into the AppDir must be restorable.
    let root = appdir.path().display().to_string();
    for (name, value) in &vars {
        if name.starts_with("HEARTHSTONE_LINUX_")
            || ["APPDIR", "ENV_DUMP", "XDG_RUNTIME_DIR"].contains(&name.as_str())
        {
            continue;
        }
        if value.contains(&root) {
            assert!(
                OVERRIDDEN_VARS.contains(&name.as_str()),
                "{name} points into the AppDir but is not in host_env::OVERRIDDEN_VARS"
            );
        }
    }
}

#[test]
fn gtk_points_at_bundled_modules() {
    let appdir = fake_appdir();
    let vars = run(appdir.path(), &[("XDG_DATA_DIRS", "/host/share")]);
    let root = appdir.path().display().to_string();
    assert_eq!(
        vars.get("XDG_DATA_DIRS").unwrap(),
        &format!("{root}/usr/share:/host/share")
    );
    assert_eq!(
        vars.get("GIO_MODULE_DIR").unwrap(),
        &format!("{root}/usr/lib/gio/modules")
    );
    assert_eq!(vars.get("GTK_EXE_PREFIX").unwrap(), &format!("{root}/usr"));

    // The loader cache is generated with absolute paths into this AppDir.
    let cache = vars.get("GDK_PIXBUF_MODULE_FILE").expect("loader cache");
    let cache = fs::read_to_string(cache).unwrap();
    assert!(cache.contains(&format!(
        "\"{root}/usr/lib/gdk-pixbuf-2.0/2.10.0/loaders/libpixbufloader-svg.so\""
    )));
    assert!(!cache.contains("@APPDIR@"));
}
