//! Dynamic symbol tables of the two commerce libraries.
//!
//! `cargo test` does not emit the cdylibs, so this builds them into a private
//! target directory and reads `nm -D --defined-only`. That is the count of
//! 212 symbols on `libblz_commerce_sdk_plugin.so` and 6 on
//! `libcommerce_http_client.so`.

use hearthstone_linux_gui_commerce::REFUSED_EXPORTS;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn dynamic_export_table_keeps_the_refused_purchase_symbols() {
    let debug = build_commerce_cdylibs();
    let plugin = defined_symbols(&debug.join("libblz_commerce_sdk_plugin.so"));
    assert_eq!(
        plugin.len(),
        212,
        "libblz_commerce_sdk_plugin.so defined dynamic symbols"
    );
    for export in REFUSED_EXPORTS {
        assert!(
            plugin.iter().any(|name| name == export),
            "missing {export} from libblz_commerce_sdk_plugin.so"
        );
    }

    let http = defined_symbols(&debug.join("libcommerce_http_client.so"));
    assert_eq!(
        http.iter().map(String::as_str).collect::<Vec<_>>(),
        [
            "AddHttpObserver",
            "CreateHttpClient",
            "CreateHttpClientWithRootCAs",
            "DestroyHttpClient",
            "RemoveHttpObserver",
            "SetHttpOptions",
        ]
    );
}

fn build_commerce_cdylibs() -> PathBuf {
    let workspace = workspace_root();
    let target = std::env::temp_dir().join("hearthstone-linux-gui-export-check");
    std::fs::create_dir_all(&target).expect("export-check target dir");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = Command::new(cargo)
        .current_dir(&workspace)
        .args([
            "build",
            "--locked",
            "--offline",
            "-p",
            "hearthstone-linux-gui-blz-commerce-sdk-plugin-stub",
            "-p",
            "hearthstone-linux-gui-commerce-http-client-stub",
        ])
        .env("CARGO_TARGET_DIR", &target)
        .env_remove("CARGO_MAKEFLAGS")
        .env_remove("MAKEFLAGS")
        .env_remove("MFLAGS")
        .output()
        .expect("spawn cargo build for commerce cdylibs");
    assert!(
        output.status.success(),
        "cargo build of commerce cdylibs failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    target.join("debug")
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|path| path.join("Cargo.lock").is_file())
        .expect("workspace root")
        .to_path_buf()
}

fn defined_symbols(library: &Path) -> Vec<String> {
    assert!(library.is_file(), "missing {}", library.display());
    let output = Command::new("nm")
        .args(["-D", "--defined-only"])
        .arg(library)
        .output()
        .expect("spawn nm");
    assert!(
        output.status.success(),
        "nm -D --defined-only {} failed: {}",
        library.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("nm output is utf-8");
    let mut names = Vec::new();
    for line in text.lines().filter(|line| !line.is_empty()) {
        let mut parts = line.split_whitespace();
        let _addr = parts.next().unwrap_or_else(|| panic!("address in {line}"));
        let _kind = parts.next().unwrap_or_else(|| panic!("kind in {line}"));
        let name = parts.next().unwrap_or_else(|| panic!("name in {line}"));
        assert!(parts.next().is_none(), "extra nm column in {line}");
        names.push(name.to_string());
    }
    names.sort();
    names
}
