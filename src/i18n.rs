//! Embedded locale catalogs for every user-visible string.
//!
//! Catalogs live in `locales/<code>.toml` and are compiled into the binary, so
//! no package format has to install translation files. English is the base
//! catalog and the fallback for keys a translation does not provide.
//!
//! One-shot strings use the [`tr!`](crate::tr) macro. Text that stays on
//! screen (status lines, progress) is a [`Phrase`]: it keeps the catalog key
//! and its arguments and is rendered again when the language changes.
//! [`select`] notifies [`subscribe`] listeners on that change, which is how
//! the window updates in place. Keys must be string literals; a test checks
//! that every key used in the sources exists and that every catalog key is
//! used.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    fmt::Display,
    rc::Rc,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex, MutexGuard, OnceLock,
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
static CATALOGS: [Entry; 4] = [
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
        "fr",
        include_str!("../locales/fr.toml"),
        include_bytes!("../locales/flags/fr.png"),
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

/// Serializes [`select`] with [`translate`] so a test can switch language
/// without another test observing the temporary catalog.
static LANG_LOCK: Mutex<()> = Mutex::new(());

type Listener = Rc<dyn Fn()>;

thread_local! {
    /// Non-zero while this thread already holds [`LANG_LOCK`], so a render
    /// started under the lock can translate without deadlocking.
    static LOCK_DEPTH: Cell<u32> = const { Cell::new(0) };
    static LISTENERS: RefCell<Vec<(u64, Listener)>> = const { RefCell::new(Vec::new()) };
    static NEXT_LISTENER: Cell<u64> = const { Cell::new(1) };
}

fn with_lang_lock<T>(body: impl FnOnce() -> T) -> T {
    LOCK_DEPTH.with(|depth| {
        if depth.get() > 0 {
            return body();
        }
        let _guard = LANG_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        depth.set(1);
        let _reset = LockDepthGuard;
        body()
    })
}

struct LockDepthGuard;

impl Drop for LockDepthGuard {
    fn drop(&mut self) {
        LOCK_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

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

/// Selects the UI language. Returns whether the catalog actually changed.
///
/// Listeners registered with [`subscribe`] run on this thread afterwards, and
/// only when the catalog changed. Call it from the GTK main thread once the
/// UI is up: listeners update widgets. A listener must not call [`select`].
pub fn select(preferred: Option<&str>) -> bool {
    let changed = with_lang_lock(|| select_unlocked(preferred));
    if changed {
        notify();
    }
    changed
}

fn select_unlocked(preferred: Option<&str>) -> bool {
    let code = pick(preferred);
    let index = CATALOGS
        .iter()
        .position(|entry| entry.code == code)
        .unwrap_or(0);
    CURRENT.swap(index, Ordering::Relaxed) != index
}

/// Keeps a [`subscribe`] listener registered. Dropping it unregisters the
/// listener; drop it on the thread that subscribed.
pub struct Subscription {
    id: u64,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let id = self.id;
        LISTENERS.with(|listeners| {
            listeners
                .borrow_mut()
                .retain(|(candidate, _)| *candidate != id)
        });
    }
}

/// Runs `listener` after every [`select`] that changes the catalog, on the
/// thread that calls `select`. The listener is not called immediately.
pub fn subscribe(listener: impl Fn() + 'static) -> Subscription {
    let id = NEXT_LISTENER.with(|next| {
        let id = next.get();
        next.set(id.wrapping_add(1));
        id
    });
    LISTENERS.with(|listeners| {
        listeners
            .borrow_mut()
            .push((id, Rc::new(listener) as Rc<dyn Fn()>));
    });
    Subscription { id }
}

fn notify() {
    let current: Vec<Listener> = LISTENERS.with(|listeners| {
        listeners
            .borrow()
            .iter()
            .map(|(_, listener)| Rc::clone(listener))
            .collect()
    });
    for listener in current {
        listener();
    }
}

/// Holds the language lock for a test and restores the previous catalog on
/// drop, so other tests keep seeing English (the default).
#[doc(hidden)]
pub struct LanguageHold {
    _guard: MutexGuard<'static, ()>,
    previous: usize,
}

impl LanguageHold {
    pub fn acquire() -> Self {
        let guard = LANG_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        LOCK_DEPTH.with(|depth| depth.set(depth.get() + 1));
        Self {
            _guard: guard,
            previous: CURRENT.load(Ordering::Relaxed),
        }
    }

    /// Selects `preferred` without taking the lock again. Notifies listeners.
    pub fn select(&self, preferred: Option<&str>) -> bool {
        let changed = select_unlocked(preferred);
        if changed {
            notify();
        }
        changed
    }
}

impl Drop for LanguageHold {
    fn drop(&mut self) {
        CURRENT.store(self.previous, Ordering::Relaxed);
        LOCK_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
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
    with_lang_lock(|| translate_unlocked(key))
}

fn translate_unlocked(key: &str) -> String {
    current_entry()
        .catalog()
        .get(key)
        .or_else(|| CATALOGS[0].catalog().get(key))
        .map_or(key, String::as_str)
        .to_string()
}

/// Looks up `key` and replaces each `{name}` placeholder.
pub fn translate_with(key: &str, args: &[(&str, &dyn Display)]) -> String {
    with_lang_lock(|| translate_with_unlocked(key, args))
}

fn translate_with_unlocked(key: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut text = translate_unlocked(key);
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), &value.to_string());
    }
    text
}

/// A catalog string that can be rendered again after the language changes.
///
/// Built with the [`phrase!`](crate::phrase) macro. Arguments that are
/// themselves phrases are rendered with the language in use at render time,
/// so a progress line updates as a whole. Literals (numbers, paths, errors)
/// stay as they are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Phrase {
    kind: PhraseKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PhraseKind {
    Literal(String),
    Key {
        key: &'static str,
        args: Vec<(&'static str, Phrase)>,
    },
    Join {
        separator: Box<Phrase>,
        parts: Vec<Phrase>,
    },
}

impl Phrase {
    /// A fixed string, not looked up in the catalog.
    pub fn literal(text: impl Into<String>) -> Self {
        Self {
            kind: PhraseKind::Literal(text.into()),
        }
    }

    /// A catalog key, rendered with [`translate`].
    pub fn key(key: &'static str) -> Self {
        Self {
            kind: PhraseKind::Key {
                key,
                args: Vec::new(),
            },
        }
    }

    /// Adds a `{name}` argument. `value` is rendered when the phrase is.
    pub fn with(mut self, name: &'static str, value: Phrase) -> Self {
        match &mut self.kind {
            PhraseKind::Key { args, .. } => args.push((name, value)),
            PhraseKind::Literal(_) | PhraseKind::Join { .. } => {
                debug_assert!(false, "only a catalog phrase takes arguments");
            }
        }
        self
    }

    /// Joins `parts` with `separator` between them. The separator is rendered
    /// at the same time as the parts, so it follows the language too.
    pub fn join(separator: Phrase, parts: Vec<Phrase>) -> Self {
        Self {
            kind: PhraseKind::Join {
                separator: Box::new(separator),
                parts,
            },
        }
    }

    /// Renders this phrase in the language currently selected.
    pub fn render(&self) -> String {
        with_lang_lock(|| self.render_unlocked())
    }

    fn render_unlocked(&self) -> String {
        match &self.kind {
            PhraseKind::Literal(text) => text.clone(),
            PhraseKind::Key { key, args } => {
                let rendered: Vec<(String, String)> = args
                    .iter()
                    .map(|(name, value)| ((*name).to_string(), value.render_unlocked()))
                    .collect();
                let borrowed: Vec<(&str, &dyn Display)> = rendered
                    .iter()
                    .map(|(name, value)| (name.as_str(), value as &dyn Display))
                    .collect();
                translate_with_unlocked(key, &borrowed)
            }
            PhraseKind::Join { separator, parts } => {
                let separator = separator.render_unlocked();
                parts
                    .iter()
                    .map(Phrase::render_unlocked)
                    .collect::<Vec<_>>()
                    .join(&separator)
            }
        }
    }
}

impl Display for Phrase {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.render())
    }
}

/// Turns a [`phrase!`](crate::phrase) argument into a [`Phrase`].
///
/// Phrases are kept and rendered later. Everything else is formatted now and
/// stored as a literal, which is what numbers, paths and error text need.
pub trait ArgValue {
    fn to_phrase(&self) -> Phrase;
}

impl ArgValue for Phrase {
    fn to_phrase(&self) -> Phrase {
        self.clone()
    }
}

impl ArgValue for String {
    fn to_phrase(&self) -> Phrase {
        Phrase::literal(self.clone())
    }
}

impl ArgValue for str {
    fn to_phrase(&self) -> Phrase {
        Phrase::literal(self)
    }
}

impl<T: ArgValue + ?Sized> ArgValue for &T {
    fn to_phrase(&self) -> Phrase {
        (*self).to_phrase()
    }
}

macro_rules! arg_value_display {
    ($($type:ty),+ $(,)?) => {$(
        impl ArgValue for $type {
            fn to_phrase(&self) -> Phrase {
                Phrase::literal(self.to_string())
            }
        }
    )+};
}

arg_value_display!(u8, u16, u32, u64, usize, i8, i16, i32, i64, f32, f64, bool);

/// Used by [`phrase!`](crate::phrase). Not a general conversion.
#[doc(hidden)]
pub fn arg(value: &impl ArgValue) -> Phrase {
    value.to_phrase()
}

/// Translates a catalog key, optionally filling `{name}` placeholders:
/// `tr!("status.ready")`, `tr!("install.failed", error = message)`.
///
/// The result is a `String` in the language selected right now. Text that
/// stays visible across a language change should be a [`phrase!`] instead.
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

/// A [`Phrase`](i18n::Phrase) for text that is shown again after the language
/// changes: `phrase!("status.ready")`, `phrase!("install.failed", error = message)`.
///
/// An argument that is itself a [`Phrase`](i18n::Phrase) is rendered with the
/// current language. Other arguments are formatted immediately.
#[macro_export]
macro_rules! phrase {
    ($key:literal) => {
        $crate::i18n::Phrase::key($key)
    };
    ($key:literal, $($name:ident = $value:expr),+ $(,)?) => {{
        let mut phrase = $crate::i18n::Phrase::key($key);
        $(
            phrase = phrase.with(stringify!($name), $crate::i18n::arg(&$value));
        )+
        phrase
    }};
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
    use std::{cell::Cell, collections::BTreeSet, rc::Rc};

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

    /// Keys used through `tr!("...")` or `phrase!("...")` anywhere under `src/`.
    fn keys_used_in_sources() -> BTreeSet<String> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let sources = [root.join("src")];
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
            for macro_name in ["phrase!(", "tr!("] {
                let mut rest = text.as_str();
                while let Some(start) = rest.find(macro_name) {
                    // Skip other macros ending in `tr!`, such as `include_str!`.
                    let other_macro = rest[..start]
                        .chars()
                        .next_back()
                        .is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
                    rest = rest[start + macro_name.len()..].trim_start();
                    if other_macro {
                        continue;
                    }
                    let Some(literal) = rest.strip_prefix('"') else {
                        panic!(
                            "{}: {macro_name} needs a string literal key",
                            path.display()
                        );
                    };
                    let end = literal.find('"').unwrap();
                    keys.insert(literal[..end].to_string());
                }
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
        assert_eq!(match_code("fr_FR.UTF-8"), Some("fr"));
        assert_eq!(match_code("fr_CA"), Some("fr"));
        assert_eq!(match_code("ja_JP.UTF-8"), None);
        assert_eq!(match_code(""), None);
    }

    #[test]
    fn fills_placeholders_and_falls_back_to_the_key() {
        let _hold = super::LanguageHold::acquire();
        assert_eq!(
            translate_with("install.failed", &[("error", &"disk full")]),
            "Failed: disk full"
        );
        assert_eq!(super::translate("no.such.key"), "no.such.key");
    }

    #[test]
    fn switching_language_rerenders_a_phrase_without_changing_it() {
        let hold = super::LanguageHold::acquire();
        // What the window keeps while a task runs: the phrase and the task
        // state. Changing the language renders new text and leaves both.
        #[derive(Clone, Debug, PartialEq)]
        struct TaskView {
            headline: super::Phrase,
            fraction: Option<f64>,
            files_done: usize,
            login_open: bool,
        }
        let view = TaskView {
            headline: crate::phrase!(
                "progress.with_total",
                action = crate::phrase!("progress.unity_downloading"),
                done = "1.0 MiB",
                total = "4.0 MiB",
                speed = crate::phrase!("progress.speed", speed = "512 KiB"),
            ),
            fraction: Some(0.25),
            files_done: 3,
            login_open: true,
        };
        let before = view.clone();
        hold.select(Some("en"));
        let english = view.headline.render();
        assert_eq!(english, "Downloading Unity:\n1.0 MiB/4.0 MiB\n512 KiB/s");
        assert!(hold.select(Some("de")));
        let german = view.headline.render();
        assert!(hold.select(Some("fr")));
        let french = view.headline.render();
        assert_eq!(view, before);
        assert_ne!(english, german);
        assert_ne!(english, french);
        assert_ne!(german, french);
        // The same catalog again does not notify.
        let hits = Rc::new(Cell::new(0));
        let hits_for_listener = Rc::clone(&hits);
        let _subscription =
            super::subscribe(move || hits_for_listener.set(hits_for_listener.get() + 1));
        assert!(!hold.select(Some("fr")));
        assert_eq!(hits.get(), 0);
        assert!(hold.select(Some("en")));
        assert_eq!(hits.get(), 1);
        assert_eq!(view.headline.render(), english);
    }

    #[test]
    fn dropping_a_subscription_stops_notifications() {
        let hold = super::LanguageHold::acquire();
        hold.select(Some("en"));
        let hits = Rc::new(Cell::new(0));
        let hits_for_listener = Rc::clone(&hits);
        let subscription =
            super::subscribe(move || hits_for_listener.set(hits_for_listener.get() + 1));
        hold.select(Some("de"));
        drop(subscription);
        hold.select(Some("en"));
        assert_eq!(hits.get(), 1);
    }
}
