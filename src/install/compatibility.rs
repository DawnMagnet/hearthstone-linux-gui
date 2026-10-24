use crate::{game_options, util, Locale, Region};
use anyhow::Result;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

const MACOS_COREFOUNDATION_IMPORT: &[u8] =
    b"/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation";
const LINUX_COREFOUNDATION_IMPORT: &[u8] = b"CoreFoundation.so";

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

/// Writes `client.config` from the template and applies the user's
/// `overrides` (`key=value` lines) on top of it.
pub fn write_client_config(
    game_dir: &Path,
    region: Region,
    locale: Locale,
    overrides: &str,
) -> Result<()> {
    let config = include_str!("../../assets/client.config.in")
        .replace("{{aurora_env}}", region.aurora_env())
        .replace("{{locale}}", locale.as_str());
    std::fs::write(
        game_dir.join("client.config"),
        game_options::apply_overrides(&config, overrides),
    )?;
    Ok(())
}

pub fn install_compatibility_files(
    game_dir: &Path,
    region: Region,
    locale: Locale,
    client_config_overrides: &str,
) -> Result<()> {
    info!(
        game_dir = %game_dir.display(),
        region = %region,
        locale = %locale,
        "installing compatibility files"
    );
    write_client_config(game_dir, region, locale, client_config_overrides)?;

    for stub in stub_files()? {
        for target in stub.spec.targets {
            copy_required(&stub.source, &game_dir.join(target))?;
        }
    }
    patch_corefoundation_imports(game_dir)?;
    install_window_icon(game_dir);
    Ok(())
}

/// Gives the game window its icon. The Linux Unity player takes it from
/// `Resources/UnityPlayer.png`, which the macOS game files lack, so the
/// window shows a generic X11 icon. The macOS `PlayerIcon.icns` already
/// holds PNG images; the 256 px one is copied out. Best effort: a missing
/// or odd icon must never prevent installation or launch.
pub fn install_window_icon(game_dir: &Path) {
    let resources = game_dir.join("Bin/Hearthstone_Data/Resources");
    let icon = resources.join("UnityPlayer.png");
    // Keep the game's own icon (or one installed here) but replace an empty
    // file left by an interrupted earlier write.
    if std::fs::metadata(&icon).is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0) {
        return;
    }

    let icns_path = resources.join("PlayerIcon.icns");
    let icns = match std::fs::read(&icns_path) {
        Ok(icns) => icns,
        Err(error) => {
            debug!(
                path = %icns_path.display(),
                %error,
                "no source for the game window icon"
            );
            return;
        }
    };
    let Some(png) = icns_png(&icns) else {
        debug!(
            path = %icns_path.display(),
            "no PNG image in the source icon"
        );
        return;
    };
    match std::fs::write(&icon, png) {
        Ok(()) => info!(path = %icon.display(), "installed game window icon"),
        Err(error) => debug!(
            path = %icon.display(),
            %error,
            "could not write game window icon"
        ),
    }
}

/// The PNG image of an `.icns` file, preferring 256 px (`ic08`), which is
/// plenty for a window icon and keeps the X11 icon property small.
///
/// A malformed or truncated file never panics: parsing stops at the image
/// container's declared end, and a broken trailing entry keeps the images
/// already found.
fn icns_png(icns: &[u8]) -> Option<&[u8]> {
    if !icns.starts_with(b"icns") {
        return None;
    }
    let declared = icns
        .get(4..8)
        .and_then(|length| length.try_into().ok())
        .map(u32::from_be_bytes)? as usize;
    // Writers normally set the total length, but tolerate a zero or stale
    // value instead of discarding an otherwise readable file.
    let end = if declared >= 8 {
        declared.min(icns.len())
    } else {
        icns.len()
    };

    let mut pngs = Vec::new();
    let mut offset = 8;
    while offset + 8 <= end {
        let header = &icns[offset..offset + 8];
        let len = u32::from_be_bytes(header[4..8].try_into().unwrap()) as usize;
        if len < 8 {
            break;
        }
        let Some(next) = offset.checked_add(len).filter(|next| *next <= end) else {
            break;
        };
        let data = &icns[offset + 8..next];
        if data.starts_with(b"\x89PNG") {
            pngs.push((&header[..4], data));
        }
        offset = next;
    }
    ["ic08", "ic09", "ic07", "ic10"]
        .iter()
        .find_map(|kind| pngs.iter().find(|(tag, _)| *tag == kind.as_bytes()))
        .or(pngs.first())
        .map(|(_, data)| *data)
}

/// Rewrite the game's absolute macOS P/Invoke name to a normal Linux library
/// name.  This keeps launch independent of Nix, FHS wrappers, and mount
/// namespace tools.  Unity's managed string heap is NUL-terminated, so the
/// shorter replacement can safely occupy the original byte span.
pub fn patch_corefoundation_imports(game_dir: &Path) -> Result<usize> {
    let managed_dir = game_dir.join("Bin/Hearthstone_Data/Managed");
    if !managed_dir.is_dir() {
        return Ok(0);
    }

    let mut patched = 0;
    for entry in std::fs::read_dir(&managed_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("dll") {
            continue;
        }
        let mut data = std::fs::read(&path)?;
        let count = patch_corefoundation_import_bytes(&mut data);
        if count == 0 {
            continue;
        }
        util::make_user_writable(&path)?;
        std::fs::write(&path, data)?;
        patched += count;
        debug!(path = %path.display(), count, "patched macOS CoreFoundation imports");
    }
    if patched > 0 {
        info!(
            count = patched,
            "patched managed CoreFoundation imports for Linux"
        );
    }
    Ok(patched)
}

fn patch_corefoundation_import_bytes(data: &mut [u8]) -> usize {
    let mut count = 0;
    let mut offset = 0;
    while let Some(relative) = data[offset..]
        .windows(MACOS_COREFOUNDATION_IMPORT.len())
        .position(|window| window == MACOS_COREFOUNDATION_IMPORT)
    {
        let start = offset + relative;
        data[start..start + MACOS_COREFOUNDATION_IMPORT.len()].fill(0);
        data[start..start + LINUX_COREFOUNDATION_IMPORT.len()]
            .copy_from_slice(LINUX_COREFOUNDATION_IMPORT);
        count += 1;
        offset = start + LINUX_COREFOUNDATION_IMPORT.len();
    }
    count
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
    use super::{copy_required, icns_png, install_window_icon, patch_corefoundation_import_bytes};

    fn icns(entries: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        let body: Vec<u8> = entries
            .iter()
            .flat_map(|(tag, data)| {
                let len = (data.len() as u32 + 8).to_be_bytes();
                [tag.as_slice(), &len, data].concat()
            })
            .collect();
        [
            b"icns".as_slice(),
            &(body.len() as u32 + 8).to_be_bytes(),
            &body,
        ]
        .concat()
    }

    #[test]
    fn picks_the_256_px_png_from_an_icns() {
        let file = icns(&[
            (b"is32", b"rle data"),
            (b"ic07", b"\x89PNG 128"),
            (b"ic08", b"\x89PNG 256"),
            (b"ic10", b"\x89PNG 1024"),
        ]);
        assert_eq!(icns_png(&file), Some(b"\x89PNG 256".as_slice()));
        assert_eq!(
            icns_png(&icns(&[(b"ic10", b"\x89PNG 1024")])),
            Some(b"\x89PNG 1024".as_slice())
        );
        assert_eq!(icns_png(&icns(&[(b"is32", b"rle data")])), None);
        assert_eq!(icns_png(b"not an icns"), None);
        // A length running past the end must not panic.
        assert_eq!(icns_png(b"icns\0\0\0\x20ic08\0\0\x01\0\x89PNG"), None);
    }

    #[test]
    fn keeps_valid_images_before_a_truncated_entry() {
        let mut file = icns(&[(b"ic08", b"\x89PNG 256")]);
        // The header claims another entry, but the file ends inside it.
        let declared = (file.len() + 16) as u32;
        file[4..8].copy_from_slice(&declared.to_be_bytes());
        file.extend_from_slice(b"ic09\0\0\0\x10");
        assert_eq!(icns_png(&file), Some(b"\x89PNG 256".as_slice()));
    }

    #[test]
    fn ignores_entries_after_the_declared_container_length() {
        let mut file = icns(&[(b"ic08", b"\x89PNG 256")]);
        let declared = file.len() as u32;
        file[4..8].copy_from_slice(&declared.to_be_bytes());
        let trailing = icns(&[(b"ic09", b"\x89PNG 512")]);
        file.extend_from_slice(&trailing[8..]);
        assert_eq!(icns_png(&file), Some(b"\x89PNG 256".as_slice()));
    }

    #[test]
    fn falls_back_to_the_file_length_when_the_total_is_unset() {
        let mut file = icns(&[(b"ic10", b"\x89PNG 1024")]);
        file[4..8].copy_from_slice(&0u32.to_be_bytes());
        assert_eq!(icns_png(&file), Some(b"\x89PNG 1024".as_slice()));
    }

    #[test]
    fn installs_the_window_icon_once() {
        let temp = tempfile::tempdir().unwrap();
        let resources = temp.path().join("Bin/Hearthstone_Data/Resources");
        std::fs::create_dir_all(&resources).unwrap();
        install_window_icon(temp.path()); // no icns: nothing happens
        assert!(!resources.join("UnityPlayer.png").exists());

        std::fs::write(
            resources.join("PlayerIcon.icns"),
            icns(&[(b"ic08", b"\x89PNG 256")]),
        )
        .unwrap();
        install_window_icon(temp.path());
        assert_eq!(
            std::fs::read(resources.join("UnityPlayer.png")).unwrap(),
            b"\x89PNG 256"
        );

        // An empty file from an interrupted earlier write is replaced.
        std::fs::write(resources.join("UnityPlayer.png"), b"").unwrap();
        install_window_icon(temp.path());
        assert_eq!(
            std::fs::read(resources.join("UnityPlayer.png")).unwrap(),
            b"\x89PNG 256"
        );

        std::fs::write(resources.join("UnityPlayer.png"), b"custom").unwrap();
        install_window_icon(temp.path());
        assert_eq!(
            std::fs::read(resources.join("UnityPlayer.png")).unwrap(),
            b"custom"
        );
    }

    #[test]
    fn patches_absolute_corefoundation_import_to_linux_name() {
        let mut data =
            b"prefix\0/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation\0suffix"
                .to_vec();

        assert_eq!(patch_corefoundation_import_bytes(&mut data), 1);
        assert!(data
            .windows(b"CoreFoundation.so".len())
            .any(|window| { window == b"CoreFoundation.so" }));
        assert!(!data
            .windows(b"/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation".len())
            .any(|window| {
                window == b"/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation"
            }));
    }

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
