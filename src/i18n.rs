//! Embedded locale catalogs for every user-visible string.
//!
//! Catalogs live in `locales/<code>.toml` and are compiled into the binary, so
//! no package format has to install translation files. English is the base
//! catalog and the fallback for keys a translation does not provide.
//!
//! Strings are looked up with the [`tr!`](crate::tr) macro. Keys must be
//! string literals; a test checks that every key used in the sources exists
//! and that every catalog key is used.

use std::{
    collections::HashMap,
    fmt::Display,
    sync::{
        atomic::{AtomicUsize, Ordering},
        OnceLock,
    },
};

const BASE_CODE: &str = "en";

/// One shipped language. The catalog is parsed on first use and then kept, so
/// each embedded source is parsed at most once per run.
struct Entry {
    code: &'static str,
    source: &'static str,
    /// Flag as PNG data, rendered from the SVG file next to it.
    flag: &'static [u8],
    parsed: OnceLock<Catalog>,
}

impl Entry {
    const fn new(code: &'static str, source: &'static str, flag: &'static [u8]) -> Self {
        Self {
            code,
            source,
            flag,
            parsed: OnceLock::new(),
        }
    }

    fn catalog(&self) -> &Catalog {
        self.parsed
            .get_or_init(|| parse(self.source).unwrap_or_default())
    }
}

/// All shipped languages; the base catalog comes first.
static CATALOGS: [Entry; 3] = [
    Entry::new(
        BASE_CODE,
        include_str!("../locales/en.toml"),
        include_bytes!("../locales/flags/en.png"),
    ),
    Entry::new(
        "de",
        include_str!("../locales/de.toml"),
        include_bytes!("../locales/flags/de.png"),
    ),
    Entry::new(
        "zh-CN",
        include_str!("../locales/zh-CN.toml"),
        include_bytes!("../locales/flags/zh-CN.png"),
    ),
];

type Catalog = HashMap<String, String>;

/// Index into [`CATALOGS`] of the language in use.
static CURRENT: AtomicUsize = AtomicUsize::new(0);

fn current_entry() -> &'static Entry {
    &CATALOGS[CURRENT.load(Ordering::Relaxed)]
}

/// The shipped language for `preferred` (the language set in the launcher
/// settings); without a match the system locale decides, then English.
fn pick(preferred: Option<&str>) -> &'static str {
    preferred
        .and_then(match_code)
        .or_else(system_language)
        .unwrap_or(BASE_CODE)
}

/// Selects the UI language. Every later lookup uses it; texts that were
/// looked up earlier keep their old language until they are built again.
pub fn select(preferred: Option<&str>) {
    let code = pick(preferred);
    let index = CATALOGS
        .iter()
        .position(|entry| entry.code == code)
        .unwrap_or(0);
    CURRENT.store(index, Ordering::Relaxed);
}

/// Code of the catalog in use, e.g. `en` or `de`.
pub fn current() -> &'static str {
    current_entry().code
}

/// A shipped UI language, described in its own words.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Language {
    pub code: &'static str,
    /// Name of the language in that language, e.g. `Deutsch`.
    pub name: String,
    /// Flag shown next to the name, as PNG data.
    pub flag: &'static [u8],
}

/// All shipped languages, base language first.
pub fn available() -> Vec<Language> {
    CATALOGS
        .iter()
        .map(|entry| Language {
            code: entry.code,
            name: entry
                .catalog()
                .get("meta.name")
                .cloned()
                .unwrap_or_else(|| entry.code.to_string()),
            flag: entry.flag,
        })
        .collect()
}

/// Looks up `key`. Prefer the [`tr!`](crate::tr) macro.
pub fn translate(key: &str) -> String {
    current_entry()
        .catalog()
        .get(key)
        .or_else(|| CATALOGS[0].catalog().get(key))
        .map_or(key, String::as_str)
        .to_string()
}

/// Looks up `key` and replaces each `{name}` placeholder.
pub fn translate_with(key: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut text = translate(key);
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), &value.to_string());
    }
    text
}

/// Translates a catalog key, optionally filling `{name}` placeholders:
/// `tr!("status.ready")`, `tr!("install.failed", error = message)`.
#[macro_export]
macro_rules! tr {
    ($key:literal) => {
        $crate::i18n::translate($key)
    };
    ($key:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::translate_with(
            $key,
            &[$((stringify!($name), &$value as &dyn ::std::fmt::Display)),+],
        )
    };
}

fn parse(source: &str) -> Result<Catalog, toml::de::Error> {
    let table: toml::Table = toml::from_str(source)?;
    let mut catalog = Catalog::new();
    flatten("", &table, &mut catalog);
    Ok(catalog)
}

fn flatten(prefix: &str, table: &toml::Table, catalog: &mut Catalog) {
    for (name, value) in table {
        let key = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}.{name}")
        };
        match value {
            toml::Value::String(text) => {
                catalog.insert(key, text.clone());
            }
            toml::Value::Table(nested) => flatten(&key, nested, catalog),
            _ => {}
        }
    }
}

/// First shipped language among the POSIX locale variables, in their usual
/// order of precedence.
fn system_language() -> Option<&'static str> {
    let language = std::env::var("LANGUAGE").unwrap_or_default();
    language
        .split(':')
        .map(str::to_owned)
        .chain(
            ["LC_ALL", "LC_MESSAGES", "LANG"]
                .into_iter()
                .filter_map(|name| std::env::var(name).ok())
                .filter(|value| !value.is_empty())
                .take(1),
        )
        .find_map(|locale| match_code(&locale))
}

/// Maps a locale such as `de_DE.UTF-8` or `zh-TW` to a shipped catalog: an
/// exact match first, then any catalog of the same language.
fn match_code(locale: &str) -> Option<&'static str> {
    let locale = locale
        .split(['.', '@'])
        .next()
        .unwrap_or_default()
        .replace('_', "-")
        .to_ascii_lowercase();
    if locale.is_empty() {
        return None;
    }
    if locale == "c" || locale == "posix" {
        return Some(BASE_CODE);
    }

    let codes = || CATALOGS.iter().map(|entry| entry.code);
    let language = |code: &str| code.split('-').next().unwrap_or_default().to_owned();
    codes()
        .find(|code| code.to_ascii_lowercase() == locale)
        .or_else(|| codes().find(|code| language(&code.to_ascii_lowercase()) == language(&locale)))
}

#[cfg(test)]
mod tests {
    use super::{match_code, parse, translate_with, Catalog, BASE_CODE, CATALOGS};
    use std::collections::BTreeSet;

    fn catalog(code: &str) -> Catalog {
        let source = CATALOGS
            .iter()
            .find(|entry| entry.code == code)
            .unwrap()
            .source;
        parse(source).unwrap_or_else(|error| panic!("locales/{code}.toml: {error}"))
    }

    fn placeholders(text: &str) -> BTreeSet<String> {
        let mut found = BTreeSet::new();
        let mut rest = text;
        while let Some(start) = rest.find('{') {
            let Some(len) = rest[start..].find('}') else {
                break;
            };
            found.insert(rest[start + 1..start + len].to_string());
            rest = &rest[start + len + 1..];
        }
        found
    }

    /// Keys used through `tr!("...")` anywhere under `src/`.
    fn keys_used_in_sources() -> BTreeSet<String> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        // The optional login window (`login-helper/`) uses the same catalogs.
        let sources = [root.join("src"), root.join("login-helper/src")];
        let mut keys = BTreeSet::new();
        for entry in sources
            .iter()
            .filter(|dir| dir.is_dir())
            .flat_map(walkdir::WalkDir::new)
        {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("rs")
                || path.ends_with("i18n.rs")
            {
                continue;
            }
            let text = std::fs::read_to_string(path).unwrap();
            let mut rest = text.as_str();
            while let Some(start) = rest.find("tr!(") {
                // Skip other macros ending in `tr!`, such as `include_str!`.
                let other_macro = rest[..start]
                    .chars()
                    .next_back()
                    .is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
                rest = rest[start + 4..].trim_start();
                if other_macro {
                    continue;
                }
                let Some(literal) = rest.strip_prefix('"') else {
                    panic!("{}: tr! needs a string literal key", path.display());
                };
                let end = literal.find('"').unwrap();
                keys.insert(literal[..end].to_string());
            }
        }
        keys
    }

    #[test]
    fn catalogs_have_the_same_keys_and_placeholders() {
        let base = catalog(BASE_CODE);
        for code in CATALOGS.iter().skip(1).map(|entry| entry.code) {
            let translated = catalog(code);
            let base_keys: BTreeSet<_> = base.keys().collect();
            let keys: BTreeSet<_> = translated.keys().collect();
            let missing: Vec<_> = base_keys.difference(&keys).collect();
            let unknown: Vec<_> = keys.difference(&base_keys).collect();
            assert!(missing.is_empty(), "locales/{code}.toml misses {missing:?}");
            assert!(
                unknown.is_empty(),
                "locales/{code}.toml has unknown keys {unknown:?}"
            );
            for (key, text) in &base {
                assert_eq!(
                    placeholders(text),
                    placeholders(&translated[key]),
                    "locales/{code}.toml: placeholders of `{key}` differ"
                );
            }
        }
    }

    #[test]
    fn source_keys_match_the_base_catalog() {
        let used = keys_used_in_sources();
        // The login window's keys are only checked where its sources are
        // present; package builds copy just the launcher.
        let helper_present = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("login-helper/src")
            .is_dir();
        let defined: BTreeSet<String> = catalog(BASE_CODE)
            .into_keys()
            .filter(|key| !key.starts_with("meta."))
            .filter(|key| helper_present || !key.starts_with("login_window."))
            .collect();
        let missing: Vec<_> = used.difference(&defined).collect();
        let unused: Vec<_> = defined.difference(&used).collect();
        assert!(
            missing.is_empty(),
            "keys missing in locales/en.toml: {missing:?}"
        );
        assert!(
            unused.is_empty(),
            "unused keys in locales/en.toml: {unused:?}"
        );
    }

    #[test]
    fn picks_the_preferred_language() {
        assert_eq!(super::pick(Some("de")), "de");
        assert_eq!(super::pick(Some("zh-cn")), "zh-CN");
    }

    #[test]
    fn matches_system_locales_to_catalogs() {
        assert_eq!(match_code("de_DE.UTF-8"), Some("de"));
        assert_eq!(match_code("de_AT@euro"), Some("de"));
        assert_eq!(match_code("zh_CN.UTF-8"), Some("zh-CN"));
        assert_eq!(match_code("zh-cn"), Some("zh-CN"));
        assert_eq!(match_code("en_GB"), Some("en"));
        assert_eq!(match_code("C.UTF-8"), Some("en"));
        assert_eq!(match_code("fr_FR.UTF-8"), None);
        assert_eq!(match_code(""), None);
    }

    #[test]
    fn fills_placeholders_and_falls_back_to_the_key() {
        assert_eq!(
            translate_with("install.failed", &[("error", &"disk full")]),
            "Failed: disk full"
        );
        assert_eq!(super::translate("no.such.key"), "no.such.key");
    }
}
