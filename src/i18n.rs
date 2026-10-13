//! Embedded locale catalogs for every user-visible string.
//!
//! Catalogs live in `locales/<code>.toml` and are compiled into the binary, so
//! no package format has to install translation files. English is the base
//! catalog and the fallback for keys a translation does not provide.
//!
//! Strings are looked up with the [`tr!`](crate::tr) macro. Keys must be
//! string literals; a test checks that every key used in the sources exists
//! and that every catalog key is used.

use std::{collections::HashMap, fmt::Display, sync::OnceLock};

const BASE_CODE: &str = "en";

/// `(language code, catalog source, flag as PNG)`; the base catalog comes
/// first. The flags are rendered from the SVG files next to them.
const CATALOGS: &[(&str, &str, &[u8])] = &[
    (
        BASE_CODE,
        include_str!("../locales/en.toml"),
        include_bytes!("../locales/flags/en.png"),
    ),
    (
        "de",
        include_str!("../locales/de.toml"),
        include_bytes!("../locales/flags/de.png"),
    ),
    (
        "zh-CN",
        include_str!("../locales/zh-CN.toml"),
        include_bytes!("../locales/flags/zh-CN.png"),
    ),
];

type Catalog = HashMap<String, String>;

struct Active {
    code: &'static str,
    selected: Option<Catalog>,
    base: Catalog,
}

static ACTIVE: OnceLock<Active> = OnceLock::new();

impl Active {
    fn for_code(code: &'static str) -> Self {
        Self {
            code,
            selected: (code != BASE_CODE).then(|| load(code)),
            base: load(BASE_CODE),
        }
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.selected
            .as_ref()
            .and_then(|catalog| catalog.get(key))
            .or_else(|| self.base.get(key))
            .map(String::as_str)
    }
}

fn active() -> &'static Active {
    // Without `init` (tests, library use) everything stays in English.
    ACTIVE.get_or_init(|| Active::for_code(BASE_CODE))
}

/// Selects the UI language once at startup. `preferred` is the language set
/// in the launcher settings; without it the system locale decides. Later
/// calls have no effect.
pub fn init(preferred: Option<&str>) {
    let code = preferred
        .and_then(match_code)
        .or_else(system_language)
        .unwrap_or(BASE_CODE);
    let _ = ACTIVE.set(Active::for_code(code));
}

/// Code of the catalog in use, e.g. `en` or `de`.
pub fn current() -> &'static str {
    active().code
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
        .map(|(code, _, flag)| Language {
            code,
            name: load(code)
                .remove("meta.name")
                .unwrap_or_else(|| (*code).to_string()),
            flag,
        })
        .collect()
}

/// Looks up `key`. Prefer the [`tr!`](crate::tr) macro.
pub fn translate(key: &str) -> String {
    active().get(key).unwrap_or(key).to_string()
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

fn load(code: &str) -> Catalog {
    let source = CATALOGS
        .iter()
        .find(|(candidate, ..)| *candidate == code)
        .map(|(_, source, _)| *source)
        .unwrap_or_default();
    parse(source).unwrap_or_default()
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

    let codes = || CATALOGS.iter().map(|(code, ..)| *code);
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
        let source = CATALOGS.iter().find(|(c, ..)| *c == code).unwrap().1;
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
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut keys = BTreeSet::new();
        for entry in walkdir::WalkDir::new(&src) {
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
        for (code, ..) in CATALOGS.iter().skip(1) {
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
        let defined: BTreeSet<String> = catalog(BASE_CODE)
            .into_keys()
            .filter(|key| !key.starts_with("meta."))
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
