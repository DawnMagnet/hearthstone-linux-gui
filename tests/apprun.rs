//! Checks the environment `packaging/appimage/AppRun` hands to the launcher.
//!
//! The bundled glibc must never end up in an exported `LD_LIBRARY_PATH`
//! (issue #22): every host program the launcher starts would inherit it.

use std::{collections::HashMap, fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

const APPRUN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/packaging/appimage/AppRun");

fn write_script(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// Builds a fake AppDir whose "launcher" dumps its environment.
fn fake_appdir(with_interpreter: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::copy(APPRUN, root.join("AppRun")).unwrap();
    fs::set_permissions(root.join("AppRun"), fs::Permissions::from_mode(0o755)).unwrap();
    write_script(
        &root.join("usr/bin/hearthstone-linux-gui"),
        "#!/bin/sh\nenv > \"$ENV_DUMP\"\n",
    );
    if with_interpreter {
        // Stands in for ld-linux: records --library-path and runs the program.
        write_script(
            &root.join("usr/lib/hearthstone-linux-gui-runtime/ld-linux-x86-64.so.2"),
            "#!/bin/sh\n[ \"$1\" = --library-path ] || exit 99\nprintf '%s' \"$2\" > \"$LIBPATH_DUMP\"\nshift 2\nexec \"$@\"\n",
        );
    }
    dir
}

fn run(appdir: &Path, env: &[(&str, &str)]) -> (HashMap<String, String>, Option<String>) {
    let env_dump = appdir.join("env.txt");
    let libpath_dump = appdir.join("libpath.txt");
    let mut command = Command::new(appdir.join("AppRun"));
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("APPDIR", appdir)
        .env("ENV_DUMP", &env_dump)
        .env("LIBPATH_DUMP", &libpath_dump)
        .envs(env.iter().copied());
    let status = command.status().unwrap();
    assert!(status.success(), "AppRun failed: {status}");
    let vars = fs::read_to_string(env_dump)
        .unwrap()
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    (vars, fs::read_to_string(libpath_dump).ok())
}

#[test]
fn bundled_libraries_are_not_exported() {
    let appdir = fake_appdir(true);
    let (vars, libpath) = run(appdir.path(), &[("LD_LIBRARY_PATH", "/host/lib")]);
    let bundled = appdir.path().join("usr/lib").display().to_string();

    // The launcher gets the bundled libraries through --library-path only.
    let libpath = libpath.expect("interpreter was used");
    assert!(libpath.starts_with(&bundled), "{libpath}");
    assert!(libpath.contains("/host/lib"), "{libpath}");

    // Children see the host value, untouched.
    assert_eq!(
        vars.get("LD_LIBRARY_PATH").map(String::as_str),
        Some("/host/lib")
    );
    assert_eq!(
        vars.get("HEARTHSTONE_LINUX_HOST_LD_LIBRARY_PATH")
            .map(String::as_str),
        Some("/host/lib")
    );
    assert_eq!(
        vars.get("HEARTHSTONE_LINUX_APPIMAGE_ENV")
            .map(String::as_str),
        Some("1")
    );
}

#[test]
fn unset_host_variables_are_recorded() {
    let appdir = fake_appdir(true);
    let (vars, _) = run(appdir.path(), &[]);
    assert!(!vars.contains_key("LD_LIBRARY_PATH"));
    assert_eq!(
        vars.get("HEARTHSTONE_LINUX_HOST_UNSET_LD_LIBRARY_PATH")
            .map(String::as_str),
        Some("1")
    );
    assert_eq!(
        vars.get("HEARTHSTONE_LINUX_HOST_PATH").map(String::as_str),
        Some("/usr/bin:/bin")
    );
    let path = vars.get("PATH").unwrap();
    assert!(path.starts_with(&appdir.path().join("usr/bin").display().to_string()));
}

#[test]
fn fallback_without_bundled_loader_still_records_host_env() {
    let appdir = fake_appdir(false);
    let (vars, libpath) = run(appdir.path(), &[("LD_LIBRARY_PATH", "/host/lib")]);
    assert!(libpath.is_none());
    let exported = vars.get("LD_LIBRARY_PATH").unwrap();
    assert!(exported.starts_with(&appdir.path().join("usr/lib").display().to_string()));
    assert_eq!(
        vars.get("HEARTHSTONE_LINUX_HOST_LD_LIBRARY_PATH")
            .map(String::as_str),
        Some("/host/lib")
    );
}
