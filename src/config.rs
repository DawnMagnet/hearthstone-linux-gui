use crate::{i18n::Phrase, phrase};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use strum::{Display, EnumString};

#[derive(Clone, Copy, Debug, Deserialize, Display, EnumString, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum Region {
    Eu,
    Us,
    Kr,
    Cn,
}

impl Region {
    pub const ALL: [Region; 4] = [Region::Us, Region::Eu, Region::Kr, Region::Cn];

    pub fn as_str(self) -> &'static str {
        match self {
            Region::Eu => "eu",
            Region::Us => "us",
            Region::Kr => "kr",
            Region::Cn => "cn",
        }
    }

    /// Catalog key of the name shown in the settings dropdown.
    /// Config files and the game keep using [`Self::as_str`].
    pub fn display_key(self) -> &'static str {
        match self {
            Region::Us => "region.us",
            Region::Eu => "region.eu",
            Region::Kr => "region.kr",
            Region::Cn => "region.cn",
        }
    }

    /// Name shown in the settings dropdown, in the current UI language.
    /// Call it again after the language changes. Config files and the game
    /// keep using [`Self::as_str`].
    pub fn display_name(self) -> Phrase {
        match self {
            Region::Us => phrase!("region.us"),
            Region::Eu => phrase!("region.eu"),
            Region::Kr => phrase!("region.kr"),
            Region::Cn => phrase!("region.cn"),
        }
    }

    pub fn remote_url(self) -> String {
        format!("http://{}.patch.battle.net:1119/hsb", self.as_str())
    }

    pub fn default_cdn(self) -> &'static str {
        match self {
            Region::Cn => "https://blzdist-hs.necdn.leihuo.netease.com/tpr/hs",
            _ => "http://level3.blizzard.com/tpr/hs",
        }
    }

    pub fn aurora_env(self) -> &'static str {
        match self {
            Region::Cn => "cn.actual.battlenet.com.cn",
            Region::Eu => "eu.actual.battle.net",
            Region::Us => "us.actual.battle.net",
            Region::Kr => "kr.actual.battle.net",
        }
    }

    /// The Hearthstone login page (`app=wtcg`) is made for the game's built-in
    /// browser, so it looks unfinished in a regular one. Other `app=` values
    /// look normal but do not work: `app=bnet` switches to `app=bas` after
    /// the e-mail step and ends in the account web page, so no token is
    /// handed back to the `blizzard-hearthstone://` callback.
    pub fn login_url(self) -> &'static str {
        match self {
            Region::Cn => "https://account.battlenet.com.cn/login/zh/?ref=blizzard-hearthstone://localhost:0/&app=wtcg-and&showCredentials=true",
            _ => "https://battle.net/login/?ref=blizzard-hearthstone://localhost:0/&app=wtcg",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Display, EnumString, Eq, PartialEq, Serialize)]
pub enum Locale {
    #[serde(rename = "deDE")]
    #[strum(serialize = "deDE")]
    DeDe,
    #[serde(rename = "enGB")]
    #[strum(serialize = "enGB")]
    EnGb,
    #[serde(rename = "enUS")]
    #[strum(serialize = "enUS")]
    EnUs,
    #[serde(rename = "esES")]
    #[strum(serialize = "esES")]
    EsEs,
    #[serde(rename = "esMX")]
    #[strum(serialize = "esMX")]
    EsMx,
    #[serde(rename = "frFR")]
    #[strum(serialize = "frFR")]
    FrFr,
    #[serde(rename = "itIT")]
    #[strum(serialize = "itIT")]
    ItIt,
    #[serde(rename = "jaJP")]
    #[strum(serialize = "jaJP")]
    JaJp,
    #[serde(rename = "koKR")]
    #[strum(serialize = "koKR")]
    KoKr,
    #[serde(rename = "plPL")]
    #[strum(serialize = "plPL")]
    PlPl,
    #[serde(rename = "ptBR")]
    #[strum(serialize = "ptBR")]
    PtBr,
    #[serde(rename = "ruRU")]
    #[strum(serialize = "ruRU")]
    RuRu,
    #[serde(rename = "thTH")]
    #[strum(serialize = "thTH")]
    ThTh,
    #[serde(rename = "zhCN")]
    #[strum(serialize = "zhCN")]
    ZhCn,
    #[serde(rename = "zhTW")]
    #[strum(serialize = "zhTW")]
    ZhTw,
}

impl Locale {
    pub const ALL: [Locale; 15] = [
        Locale::DeDe,
        Locale::EnGb,
        Locale::EnUs,
        Locale::EsEs,
        Locale::EsMx,
        Locale::FrFr,
        Locale::ItIt,
        Locale::JaJp,
        Locale::KoKr,
        Locale::PlPl,
        Locale::PtBr,
        Locale::RuRu,
        Locale::ThTh,
        Locale::ZhCn,
        Locale::ZhTw,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Locale::DeDe => "deDE",
            Locale::EnGb => "enGB",
            Locale::EnUs => "enUS",
            Locale::EsEs => "esES",
            Locale::EsMx => "esMX",
            Locale::FrFr => "frFR",
            Locale::ItIt => "itIT",
            Locale::JaJp => "jaJP",
            Locale::KoKr => "koKR",
            Locale::PlPl => "plPL",
            Locale::PtBr => "ptBR",
            Locale::RuRu => "ruRU",
            Locale::ThTh => "thTH",
            Locale::ZhCn => "zhCN",
            Locale::ZhTw => "zhTW",
        }
    }

    /// Catalog key of the name shown in the settings dropdown.
    /// Config files and the game keep using [`Self::as_str`].
    pub fn display_key(self) -> &'static str {
        match self {
            Locale::DeDe => "game_locale.deDE",
            Locale::EnGb => "game_locale.enGB",
            Locale::EnUs => "game_locale.enUS",
            Locale::EsEs => "game_locale.esES",
            Locale::EsMx => "game_locale.esMX",
            Locale::FrFr => "game_locale.frFR",
            Locale::ItIt => "game_locale.itIT",
            Locale::JaJp => "game_locale.jaJP",
            Locale::KoKr => "game_locale.koKR",
            Locale::PlPl => "game_locale.plPL",
            Locale::PtBr => "game_locale.ptBR",
            Locale::RuRu => "game_locale.ruRU",
            Locale::ThTh => "game_locale.thTH",
            Locale::ZhCn => "game_locale.zhCN",
            Locale::ZhTw => "game_locale.zhTW",
        }
    }

    /// Name shown in the settings dropdown, in the current UI language.
    /// Call it again after the language changes. Config files and the game
    /// keep using [`Self::as_str`].
    ///
    /// Game languages use their own name (Deutsch, 简体中文, …) in every
    /// catalog, the same way launcher languages use `meta.name`.
    pub fn display_name(self) -> Phrase {
        match self {
            Locale::DeDe => phrase!("game_locale.deDE"),
            Locale::EnGb => phrase!("game_locale.enGB"),
            Locale::EnUs => phrase!("game_locale.enUS"),
            Locale::EsEs => phrase!("game_locale.esES"),
            Locale::EsMx => phrase!("game_locale.esMX"),
            Locale::FrFr => phrase!("game_locale.frFR"),
            Locale::ItIt => phrase!("game_locale.itIT"),
            Locale::JaJp => phrase!("game_locale.jaJP"),
            Locale::KoKr => phrase!("game_locale.koKR"),
            Locale::PlPl => phrase!("game_locale.plPL"),
            Locale::PtBr => phrase!("game_locale.ptBR"),
            Locale::RuRu => phrase!("game_locale.ruRU"),
            Locale::ThTh => phrase!("game_locale.thTH"),
            Locale::ZhCn => phrase!("game_locale.zhCN"),
            Locale::ZhTw => phrase!("game_locale.zhTW"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AppConfig {
    pub region: Region,
    pub locale: Locale,
    pub game_dir: Option<PathBuf>,
    pub installed_version: Option<String>,
    pub unity_version: Option<String>,
    #[serde(default)]
    pub logged_in: bool,
    #[serde(default)]
    pub last_login_at: Option<String>,
    #[serde(default)]
    pub use_discrete_gpu: bool,
    /// Start the game through Feral GameMode (`gamemoderun`).
    #[serde(default)]
    pub use_gamemode: bool,
    /// Start the game with the MangoHud overlay (`mangohud`).
    #[serde(default)]
    pub use_mangohud: bool,
    /// Look for a newer launcher release at startup and point to its page.
    /// Off by default: it contacts GitHub.
    #[serde(default)]
    pub check_for_updates: bool,
    /// Upload the card collection to HSReplay.net after each launch.
    /// Off by default: it reads the game's memory and contacts HSReplay.
    #[serde(default)]
    pub hsreplay_sync: bool,
    /// Launcher UI language; `None` follows the system locale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_language: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            region: Region::Us,
            locale: Locale::EnUs,
            game_dir: None,
            installed_version: None,
            unity_version: None,
            logged_in: false,
            last_login_at: None,
            use_discrete_gpu: false,
            use_gamemode: false,
            use_mangohud: false,
            check_for_updates: false,
            hsreplay_sync: false,
            ui_language: None,
        }
    }
}

impl AppConfig {
    pub fn load_or_default(path: &std::path::Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let data = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str(&data).with_context(|| format!("failed to parse {}", path.display()))
    }

    /// Defaults with region and game language guessed from `LC_ALL`,
    /// `LC_MESSAGES` or `LANG`, whichever is set first. Meant for the first
    /// start of the GUI, before anything is saved.
    pub fn for_system_locale() -> Self {
        let system_locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .filter_map(|name| std::env::var(name).ok())
            .find(|value| !value.is_empty())
            .unwrap_or_default();
        let (region, locale) = detect_region_and_locale(&system_locale);
        Self {
            region,
            locale,
            ..Self::default()
        }
    }

    pub fn save(&self, path: &std::path::Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = toml::to_string_pretty(self)?;
        std::fs::write(path, data).with_context(|| format!("failed to write {}", path.display()))
    }
}

/// Maps a POSIX locale such as `de_DE.UTF-8` to the Battle.net region and
/// game language a player there most likely wants. Unknown locales give the
/// regular defaults.
fn detect_region_and_locale(system_locale: &str) -> (Region, Locale) {
    let tag = system_locale
        .split(['.', '@'])
        .next()
        .unwrap_or_default()
        .replace('-', "_");
    let (language, country) = match tag.split_once('_') {
        Some((language, country)) => (language.to_ascii_lowercase(), country.to_ascii_uppercase()),
        None => (tag.to_ascii_lowercase(), String::new()),
    };
    let country = country.as_str();

    let locale = match (language.as_str(), country) {
        ("de", _) => Locale::DeDe,
        ("en", "GB" | "IE" | "AU" | "NZ" | "ZA" | "IN") => Locale::EnGb,
        ("es", "" | "ES") => Locale::EsEs,
        ("es", _) => Locale::EsMx,
        ("fr", _) => Locale::FrFr,
        ("it", _) => Locale::ItIt,
        ("ja", _) => Locale::JaJp,
        ("ko", _) => Locale::KoKr,
        ("pl", _) => Locale::PlPl,
        ("pt", _) => Locale::PtBr,
        ("ru", _) => Locale::RuRu,
        ("th", _) => Locale::ThTh,
        ("zh", "TW" | "HK" | "MO") => Locale::ZhTw,
        ("zh", _) => Locale::ZhCn,
        _ => Locale::EnUs,
    };

    // Battle.net regions: Americas also covers Oceania and Southeast Asia,
    // Asia covers Korea, Taiwan and Japan, Europe covers the rest of the
    // countries known here.
    let region = match country {
        "CN" => Region::Cn,
        "KR" | "TW" | "HK" | "MO" | "JP" => Region::Kr,
        "US" | "CA" | "MX" | "BR" | "AR" | "CL" | "CO" | "PE" | "VE" | "UY" | "EC" | "BO"
        | "PY" | "CR" | "PA" | "DO" | "GT" | "AU" | "NZ" | "SG" | "TH" | "MY" | "PH" | "ID"
        | "VN" => Region::Us,
        "" => match locale {
            Locale::ZhCn => Region::Cn,
            Locale::JaJp | Locale::KoKr | Locale::ZhTw => Region::Kr,
            Locale::DeDe
            | Locale::EnGb
            | Locale::EsEs
            | Locale::FrFr
            | Locale::ItIt
            | Locale::PlPl
            | Locale::RuRu => Region::Eu,
            _ => Region::Us,
        },
        _ => Region::Eu,
    };
    (region, locale)
}

#[cfg(test)]
mod tests {
    use super::{detect_region_and_locale, AppConfig, Locale, Region};

    #[test]
    fn detects_region_and_game_language_from_system_locale() {
        let cases = [
            ("de_DE.UTF-8", Region::Eu, Locale::DeDe),
            ("de_AT@euro", Region::Eu, Locale::DeDe),
            ("en_US.UTF-8", Region::Us, Locale::EnUs),
            ("en_GB.UTF-8", Region::Eu, Locale::EnGb),
            ("en_AU.UTF-8", Region::Us, Locale::EnGb),
            ("es_ES", Region::Eu, Locale::EsEs),
            ("es_MX", Region::Us, Locale::EsMx),
            ("pt_BR.UTF-8", Region::Us, Locale::PtBr),
            ("pt_PT.UTF-8", Region::Eu, Locale::PtBr),
            ("zh_CN.UTF-8", Region::Cn, Locale::ZhCn),
            ("zh_TW.UTF-8", Region::Kr, Locale::ZhTw),
            ("ja_JP.UTF-8", Region::Kr, Locale::JaJp),
            ("ko_KR.UTF-8", Region::Kr, Locale::KoKr),
            ("nl_NL.UTF-8", Region::Eu, Locale::EnUs),
            ("fr", Region::Eu, Locale::FrFr),
            ("C.UTF-8", Region::Us, Locale::EnUs),
            ("", Region::Us, Locale::EnUs),
        ];
        for (system_locale, region, locale) in cases {
            assert_eq!(
                detect_region_and_locale(system_locale),
                (region, locale),
                "{system_locale}"
            );
        }
    }

    #[test]
    fn parses_and_displays_regions() {
        assert_eq!("us".parse::<Region>().unwrap(), Region::Us);
        assert_eq!(Region::Cn.to_string(), "cn");
        assert!("tw".parse::<Region>().is_err());
    }

    #[test]
    fn parses_and_displays_locales() {
        assert_eq!("enUS".parse::<Locale>().unwrap(), Locale::EnUs);
        assert_eq!(Locale::ZhTw.to_string(), "zhTW");
        assert!("en-us".parse::<Locale>().is_err());
    }

    #[test]
    fn config_file_keeps_region_and_locale_codes() {
        let config = AppConfig {
            region: Region::Kr,
            locale: Locale::JaJp,
            ..AppConfig::default()
        };
        let text = toml::to_string(&config).unwrap();
        assert!(text.contains("region = \"kr\""), "{text}");
        assert!(text.contains("locale = \"jaJP\""), "{text}");
        assert!(!text.contains("Asia"), "{text}");
        assert!(!text.contains("日本語"), "{text}");
        let parsed: AppConfig = toml::from_str(&text).unwrap();
        assert_eq!((parsed.region, parsed.locale), (Region::Kr, Locale::JaJp));
        assert_eq!(parsed.region.as_str(), "kr");
        assert_eq!(parsed.locale.as_str(), "jaJP");
    }

    /// Every shipped catalog has a display name for every variant, and
    /// [`Region::display_name`] / [`Locale::display_name`] read that entry.
    #[test]
    fn every_catalog_names_every_region_and_locale() {
        use crate::i18n::LanguageHold;
        use std::collections::HashMap;

        fn flatten(prefix: &str, table: &toml::Table, into: &mut HashMap<String, String>) {
            for (name, value) in table {
                let key = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}.{name}")
                };
                match value {
                    toml::Value::String(text) => {
                        into.insert(key, text.clone());
                    }
                    toml::Value::Table(nested) => flatten(&key, nested, into),
                    _ => {}
                }
            }
        }

        fn catalog(code: &str) -> HashMap<String, String> {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("locales/{code}.toml"));
            let table: toml::Table = toml::from_str(
                &std::fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display())),
            )
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let mut parsed = HashMap::new();
            flatten("", &table, &mut parsed);
            parsed
        }

        let english = catalog("en");
        let hold = LanguageHold::acquire();
        for code in ["en", "de", "fr", "zh-CN"] {
            let parsed = catalog(code);
            hold.select(Some(code));
            for region in Region::ALL {
                let key = region.display_key();
                let value = parsed
                    .get(key)
                    .unwrap_or_else(|| panic!("locales/{code}.toml misses {key}"));
                assert!(!value.is_empty(), "{key}");
                assert_ne!(value, region.as_str(), "{key}");
                assert_eq!(region.display_name().render(), *value, "{code} {key}");
            }
            for locale in Locale::ALL {
                let key = locale.display_key();
                let value = parsed
                    .get(key)
                    .unwrap_or_else(|| panic!("locales/{code}.toml misses {key}"));
                assert!(!value.is_empty(), "{key}");
                assert_ne!(value, locale.as_str(), "{key}");
                assert_eq!(locale.display_name().render(), *value, "{code} {key}");
                // The game language is named in that language in every catalog.
                assert_eq!(value, &english[key], "{code} {key}");
            }
        }
    }
}
