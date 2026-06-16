use crate::{util, Locale, Region};
use anyhow::Result;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

struct StubSpec {
    installed_name: &'static str,
    dev_name: &'static str,
    targets: &'static [&'static str],
}

const STUBS: &[StubSpec] = &[
    StubSpec {
        installed_name: "CoreFoundation.so",
        dev_name: "libCoreFoundation.so",
        targets: &[
            "Bin/Hearthstone_Data/Plugins/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation.so",
        ],
    },
    StubSpec {
        installed_name: "libOSXWindowManagement.so",
        dev_name: "libOSXWindowManagement.so",
        targets: &["Bin/Hearthstone_Data/Plugins/libOSXWindowManagement.so"],
    },
    StubSpec {
        installed_name: "libblz_commerce_sdk_plugin.so",
        dev_name: "libblz_commerce_sdk_plugin.so",
        targets: &["Bin/Hearthstone_Data/Plugins/libblz_commerce_sdk_plugin.so"],
    },
    StubSpec {
        installed_name: "libNativeApiMac.so",
        dev_name: "libNativeApiMac.so",
        targets: &[
            "Bin/Hearthstone_Data/Plugins/libNativeApiMac.so",
            "Bin/Hearthstone_Data/MonoBleedingEdge/x86_64/libNativeApiMac.so",
        ],
    },
    StubSpec {
        installed_name: "libcommerce_http_client.so",
        dev_name: "libcommerce_http_client.so",
        targets: &[
            "Bin/Hearthstone_Data/Plugins/libcommerce_http_client.so",
            "Bin/Hearthstone_Data/MonoBleedingEdge/x86_64/libcommerce_http_client.so",
        ],
    },
];

pub fn install_compatibility_files(game_dir: &Path, region: Region, locale: Locale) -> Result<()> {
    info!(
        game_dir = %game_dir.display(),
        region = %region,
        locale = %locale,
        "installing compatibility files"
    );
    let config = include_str!("../../assets/client.config.in")
        .replace("{{aurora_env}}", region.aurora_env())
        .replace("{{locale}}", locale.as_str());
    std::fs::write(game_dir.join("client.config"), config)?;

    for stub in stub_files()? {
        for target in stub.spec.targets {
            copy_required(&stub.source, &game_dir.join(target))?;
        }
    }
    Ok(())
}

struct StubFile {
    spec: &'static StubSpec,
    source: PathBuf,
}

#[derive(Clone, Copy)]
enum StubLayout {
    Installed,
    Dev,
}

impl StubSpec {
    fn source_name(&self, layout: StubLayout) -> &'static str {
        match layout {
            StubLayout::Installed => self.installed_name,
            StubLayout::Dev => self.dev_name,
        }
    }
}

fn copy_required(from: &Path, to: &Path) -> Result<()> {
    debug!(from = %from.display(), to = %to.display(), "copying compatibility file");
    util::copy_file_replace(from, to)?;
    util::make_user_writable(to)?;
    Ok(())
}

fn resource_dir() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("HEARTHSTONE_LINUX_RESOURCES") {
        debug!(path = %path, "using resources from environment");
        return Ok(PathBuf::from(path));
    }

    let exe = std::env::current_exe()?;
    if let Some(prefix) = exe.parent().and_then(|bin| bin.parent()) {
        let share = prefix.join("share/hearthstone-linux-gui");
        if share.exists() {
            debug!(path = %share.display(), "using installed resources");
            return Ok(share);
        }
    }

    anyhow::bail!("could not locate hearthstone-linux-gui resources")
}

fn stub_files() -> Result<Vec<StubFile>> {
    if let Ok(path) = std::env::var("HEARTHSTONE_LINUX_STUBS") {
        debug!(path = %path, "using stubs from environment");
        return stub_files_in(PathBuf::from(path), StubLayout::Installed);
    }

    if let Ok(resources) = resource_dir() {
        let stubs = resources.join("stubs");
        if stubs.exists() {
            debug!(path = %stubs.display(), "using installed stubs");
            return stub_files_in(stubs, StubLayout::Installed);
        }
    }

    if let Some(stubs) = dev_stub_files()? {
        debug!("using development stub libraries");
        return Ok(stubs);
    }

    anyhow::bail!("could not locate hearthstone-linux-gui stub libraries")
}

fn stub_files_in(dir: PathBuf, layout: StubLayout) -> Result<Vec<StubFile>> {
    let mut files = Vec::with_capacity(STUBS.len());
    let mut missing = Vec::new();

    for spec in STUBS {
        let source = dir.join(spec.source_name(layout));
        if source.exists() {
            files.push(StubFile { spec, source });
        } else {
            missing.push(spec.source_name(layout));
        }
    }

    if missing.is_empty() {
        Ok(files)
    } else {
        anyhow::bail!(
            "stub libraries are missing from {}: {}",
            dir.display(),
            missing.join(", ")
        )
    }
}

fn dev_stub_files() -> Result<Option<Vec<StubFile>>> {
    let exe = std::env::current_exe()?;
    let Some(profile_dir) = exe.parent() else {
        return Ok(None);
    };

    for dir in [profile_dir.to_path_buf(), profile_dir.join("deps")] {
        if let Ok(files) = stub_files_in(dir, StubLayout::Dev) {
            return Ok(Some(files));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::copy_required;

    #[cfg(unix)]
    #[test]
    fn replaces_read_only_stub_and_keeps_destination_writable() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let first_source = temp.path().join("first.so");
        let second_source = temp.path().join("second.so");
        let destination = temp.path().join("game/Bin/Plugins/libstub.so");

        std::fs::write(&first_source, b"first").unwrap();
        std::fs::write(&second_source, b"second").unwrap();
        std::fs::set_permissions(&first_source, std::fs::Permissions::from_mode(0o444)).unwrap();
        std::fs::set_permissions(&second_source, std::fs::Permissions::from_mode(0o444)).unwrap();

        copy_required(&first_source, &destination).unwrap();
        std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o444)).unwrap();

        copy_required(&second_source, &destination).unwrap();

        assert_eq!(std::fs::read(&destination).unwrap(), b"second");
        assert_ne!(
            std::fs::metadata(&destination)
                .unwrap()
                .permissions()
                .mode()
                & 0o200,
            0
        );
    }
}
