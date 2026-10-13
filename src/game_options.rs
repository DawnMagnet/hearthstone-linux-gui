//! Reads and edits the game's own settings file, `options.txt`.
//!
//! Hearthstone keeps its graphics and gameplay options as `key=value` lines.
//! The Linux Unity player uses the macOS location below the home directory
//! unchanged. Unknown keys and the order of lines are preserved, because the
//! game owns this file and rewrites it on exit.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

const FRAME_RATE: &str = "targetframerate";
const VSYNC: &str = "vsync";
const QUALITY: &str = "graphicsquality";

/// `~/Library/Preferences/Blizzard/Hearthstone/options.txt`.
pub fn options_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").filter(|home| !home.is_empty())?;
    Some(PathBuf::from(home).join("Library/Preferences/Blizzard/Hearthstone/options.txt"))
}

/// Graphics quality levels as the game stores them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Quality {
    Low,
    Medium,
    High,
}

impl Quality {
    pub const ALL: [Quality; 3] = [Quality::Low, Quality::Medium, Quality::High];

    fn value(self) -> &'static str {
        match self {
            Quality::Low => "0",
            Quality::Medium => "1",
            Quality::High => "2",
        }
    }
}

/// The lines of an `options.txt`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GameOptions {
    lines: Vec<String>,
}

impl GameOptions {
    pub fn parse(text: &str) -> Self {
        Self {
            lines: text.lines().map(str::to_owned).collect(),
        }
    }

    /// Loads the file; a missing file gives empty options.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        Ok(Self::parse(&text))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        std::fs::write(path, self.to_text())
            .with_context(|| format!("failed to write {}", path.display()))
    }

    pub fn to_text(&self) -> String {
        let mut text = self.lines.join("\n");
        if !text.is_empty() {
            text.push('\n');
        }
        text
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.lines
            .iter()
            .find_map(|line| split_line(line).filter(|(name, _)| *name == key))
            .map(|(_, value)| value)
    }

    /// Replaces the value of `key` in place, or appends the key.
    pub fn set(&mut self, key: &str, value: &str) {
        let line = format!("{key}={value}");
        match self.position(key) {
            Some(index) => self.lines[index] = line,
            None => self.lines.push(line),
        }
    }

    pub fn remove(&mut self, key: &str) {
        self.lines
            .retain(|line| split_line(line).is_none_or(|(name, _)| name != key));
    }

    fn position(&self, key: &str) -> Option<usize> {
        self.lines
            .iter()
            .position(|line| split_line(line).is_some_and(|(name, _)| name == key))
    }

    /// Frame-rate limit in frames per second, if one is set.
    pub fn frame_rate(&self) -> Option<u32> {
        self.get(FRAME_RATE)?.parse().ok()
    }

    pub fn set_frame_rate(&mut self, fps: u32) {
        self.set(FRAME_RATE, &fps.to_string());
    }

    /// VSync is on unless the file says `vsync=0`.
    pub fn vsync(&self) -> bool {
        !matches!(self.get(VSYNC), Some("0" | "false" | "False"))
    }

    /// Switching VSync off adds `vsync=0`; switching it on removes the key
    /// again, which is the game's default.
    pub fn set_vsync(&mut self, enabled: bool) {
        if enabled {
            self.remove(VSYNC);
        } else {
            self.set(VSYNC, "0");
        }
    }

    pub fn quality(&self) -> Option<Quality> {
        let value = self.get(QUALITY)?;
        Quality::ALL
            .into_iter()
            .find(|quality| quality.value() == value)
    }

    pub fn set_quality(&mut self, quality: Quality) {
        self.set(QUALITY, quality.value());
    }

    /// Removes the keys this launcher edits, so the game's defaults apply.
    pub fn reset_graphics(&mut self) {
        for key in [FRAME_RATE, VSYNC, QUALITY] {
            self.remove(key);
        }
    }
}

fn split_line(line: &str) -> Option<(&str, &str)> {
    let (name, value) = line.split_once('=')?;
    Some((name.trim(), value.trim()))
}

/// Applies user entries (`key=value` lines) on top of a rendered
/// `client.config`: an entry replaces the line with the same key or is
/// appended. Blank lines and `#` comments in `overrides` are ignored.
pub fn apply_overrides(config: &str, overrides: &str) -> String {
    let mut merged = GameOptions::parse(config);
    for line in overrides.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = split_line(line) {
            merged.set(key, value);
        }
    }
    merged.to_text()
}

#[cfg(test)]
mod tests {
    use super::{apply_overrides, GameOptions, Quality};

    #[test]
    fn keeps_unknown_keys_and_order() {
        let mut options = GameOptions::parse("music=0.5\ntargetframerate=60\nsound=1\n");
        options.set_frame_rate(144);
        options.set_vsync(false);
        assert_eq!(
            options.to_text(),
            "music=0.5\ntargetframerate=144\nsound=1\nvsync=0\n"
        );
    }

    #[test]
    fn vsync_is_on_unless_set_to_zero() {
        let mut options = GameOptions::parse("graphicsquality=2\n");
        assert!(options.vsync());
        assert_eq!(options.quality(), Some(Quality::High));

        options.set_vsync(false);
        assert_eq!(options.get("vsync"), Some("0"));
        assert!(!options.vsync());

        options.set_vsync(true);
        assert_eq!(options.get("vsync"), None);
    }

    #[test]
    fn reset_removes_only_graphics_keys() {
        let mut options =
            GameOptions::parse("music=0.5\ntargetframerate=30\nvsync=0\ngraphicsquality=0\n");
        options.reset_graphics();
        assert_eq!(options.to_text(), "music=0.5\n");
        assert_eq!(options.frame_rate(), None);
    }

    #[test]
    fn missing_file_loads_as_empty_and_saves_with_parents() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("a/b/options.txt");
        let mut options = GameOptions::load(&path).unwrap();
        assert_eq!(options.to_text(), "");
        options.set_quality(Quality::Low);
        options.save(&path).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "graphicsquality=0\n"
        );
    }

    #[test]
    fn overrides_replace_or_append_client_config_entries() {
        let config = "Aurora.ClientCheck=false\nLocalization.Locale=enUS\n";
        let overrides = "# my settings\n\nLocalization.Locale=deDE\nAurora.Version=1\n";
        assert_eq!(
            apply_overrides(config, overrides),
            "Aurora.ClientCheck=false\nLocalization.Locale=deDE\nAurora.Version=1\n"
        );
        assert_eq!(apply_overrides(config, ""), config);
    }
}
