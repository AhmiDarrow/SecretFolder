//! The interface language, as SecretFolder 0.2.0 has it: the shipped catalogs
//! (`src/i18n/locales/*.json`, compiled in unchanged), the choice kept in `ui.json` beside the
//! vault by the shipped `prefs.rs` (never inside `vault.json`), and every label following a
//! switch at once. Strings the port shows that 0.2.0 had no key for live in
//! `port-strings.json`, in the same eleven languages.

use std::collections::HashMap;
use std::sync::OnceLock;

use remedy_ui::reactive::{Signal, untrack};

/// The languages SecretFolder ships, with their own names (`LOCALE_NATIVE_NAMES`).
pub const LOCALES: [(&str, &str); 11] = [
    ("en", "English"),
    ("es", "Español"),
    ("fr", "Français"),
    ("de", "Deutsch"),
    ("pt-BR", "Português (Brasil)"),
    ("it", "Italiano"),
    ("ru", "Русский"),
    ("pl", "Polski"),
    ("ja", "日本語"),
    ("ko", "한국어"),
    ("zh-CN", "简体中文"),
];

const SOURCES: [(&str, &str); 11] = [
    ("en", include_str!("../../src/i18n/locales/en.json")),
    ("es", include_str!("../../src/i18n/locales/es.json")),
    ("fr", include_str!("../../src/i18n/locales/fr.json")),
    ("de", include_str!("../../src/i18n/locales/de.json")),
    ("pt-BR", include_str!("../../src/i18n/locales/pt-BR.json")),
    ("it", include_str!("../../src/i18n/locales/it.json")),
    ("ru", include_str!("../../src/i18n/locales/ru.json")),
    ("pl", include_str!("../../src/i18n/locales/pl.json")),
    ("ja", include_str!("../../src/i18n/locales/ja.json")),
    ("ko", include_str!("../../src/i18n/locales/ko.json")),
    ("zh-CN", include_str!("../../src/i18n/locales/zh-CN.json")),
];

const PORT: &str = include_str!("port-strings.json");

type Table = HashMap<String, String>;

fn catalogs() -> &'static HashMap<&'static str, Table> {
    static C: OnceLock<HashMap<&'static str, Table>> = OnceLock::new();
    C.get_or_init(|| {
        SOURCES
            .iter()
            .map(|(id, src)| (*id, serde_json::from_str::<Table>(src.trim_start_matches('\u{feff}')).unwrap_or_default()))
            .collect()
    })
}

/// `port.*` key → language → text.
fn port() -> &'static HashMap<String, Table> {
    static P: OnceLock<HashMap<String, Table>> = OnceLock::new();
    P.get_or_init(|| serde_json::from_str(PORT).unwrap_or_default())
}

fn lookup(id: &str, key: &str) -> Option<String> {
    let found = if key.starts_with("port.") {
        port().get(key).and_then(|m| m.get(id)).cloned()
    } else {
        catalogs().get(id).and_then(|c| c.get(key)).cloned()
    };
    found.filter(|s| !s.is_empty())
}

/// `key` in `id`, then English, then the key itself (as the shipped `translate`).
pub fn tr(id: &str, key: &str) -> String {
    lookup(id, key).or_else(|| lookup("en", key)).unwrap_or_else(|| key.to_owned())
}

/// `text` with each `{name}` in `vars` filled; unknown placeholders stay as written.
pub fn fill(text: &str, vars: &[(&str, &str)]) -> String {
    let mut out = text.to_owned();
    for (name, value) in vars {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

/// [`tr`] with placeholders filled.
pub fn trv(id: &str, key: &str, vars: &[(&str, &str)]) -> String {
    fill(&tr(id, key), vars)
}

/// The CLDR plural category of a whole number in a shipped locale (what `Intl.PluralRules`
/// answers for these eleven).
pub fn plural(id: &str, n: u64) -> &'static str {
    let (m10, m100) = (n % 10, n % 100);
    let few = (2..=4).contains(&m10) && !(12..=14).contains(&m100);
    match id {
        "ja" | "ko" | "zh-CN" => "other",
        "fr" | "pt-BR" if n <= 1 => "one",
        "fr" | "pt-BR" | "es" | "it" if n != 0 && n % 1_000_000 == 0 => "many",
        "fr" | "pt-BR" => "other",
        "ru" if m10 == 1 && m100 != 11 => "one",
        "pl" if n == 1 => "one",
        "ru" | "pl" if few => "few",
        "ru" | "pl" => "many",
        _ if n == 1 => "one",
        _ => "other",
    }
}

/// A count in the locale's plural form (`<base>.one`, `.few`, `.many` or `.other`, with
/// `{count}`), as the shipped `tPlural`.
pub fn count_in(id: &str, base: &str, n: u64) -> String {
    let key = format!("{base}.{}", plural(id, n));
    let text = lookup(id, &key)
        .or_else(|| lookup(id, &format!("{base}.other")))
        .or_else(|| lookup("en", &key))
        .or_else(|| lookup("en", &format!("{base}.other")))
        .unwrap_or_else(|| format!("{base}.other"));
    fill(&text, &[("count", &number(id, n))])
}

/// The decimal sign the locale writes.
fn decimal(id: &str) -> char {
    match id {
        "en" | "ja" | "ko" | "zh-CN" => '.',
        _ => ',',
    }
}

/// A whole number grouped the locale's way (`Intl.NumberFormat`): Spanish and Polish group
/// only from five digits.
pub fn number(id: &str, n: u64) -> String {
    let digits = n.to_string();
    let sep = match id {
        "en" | "ja" | "ko" | "zh-CN" => ",",
        "de" | "es" | "it" | "pt-BR" => ".",
        "fr" => "\u{202f}",
        _ => "\u{a0}",
    };
    let min = if matches!(id, "es" | "pl") { 5 } else { 4 };
    if digits.len() < min {
        return digits;
    }
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push_str(sep);
        }
        out.push(c);
    }
    out
}

/// A size as the shipped `formatBytes`: bytes whole, KB and MB to one place.
pub fn bytes(id: &str, n: u64) -> String {
    let (value, unit) = if n >= 1024 * 1024 {
        (n as f64 / (1024.0 * 1024.0), "MB")
    } else if n >= 1024 {
        (n as f64 / 1024.0, "KB")
    } else {
        return format!("{} B", number(id, n));
    };
    let s = format!("{value:.1}");
    let (whole, frac) = s.split_once('.').unwrap_or((&s, "0"));
    let whole = number(id, whole.parse().unwrap_or(0));
    format!("{whole}{}{frac} {unit}", decimal(id))
}

/// The first letter in upper case (catalog words such as "text" or "dismiss" used as labels).
pub fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Catalog text without its `**strong**` markers (the shipped `RichText`), for places that
/// show plain text.
pub fn plain(s: &str) -> String {
    s.replace("**", "")
}

/// A vault or system error (English, from the shipped code) in the interface language: the
/// shipped `translateBackendError`. Text it does not know is shown as it is.
pub fn err_in(id: &str, raw: &str) -> String {
    const EXACT: [(&str, &str); 37] = [
        ("Something went wrong.", "err.generic"),
        ("vault is not initialized", "err.notInitialized"),
        ("vault is already initialized", "err.alreadyInitialized"),
        ("vault is already unlocked", "err.alreadyUnlocked"),
        ("vault is locked", "err.locked"),
        ("incorrect password or recovery key", "err.badPassword"),
        ("item not found", "err.notFound"),
        ("invalid name", "err.invalidName"),
        ("vault lock poisoned", "err.vaultLockPoisoned"),
        ("throttle lock poisoned", "err.throttleLockPoisoned"),
        ("path does not exist or cannot be resolved", "err.pathMissing"),
        ("import path must be under your home directory (Desktop, Documents, or Downloads)", "err.importPath"),
        ("invalid export path", "err.invalidExportPath"),
        ("export path cannot be resolved", "err.exportUnresolved"),
        ("cannot export into the SecretFolder data folder — choose another location", "err.exportIntoVault"),
        ("export path must be under your home directory (Desktop, Documents, or Downloads)", "err.exportPath"),
        ("path is not a file", "err.notAFile"),
        ("url not allowed", "err.urlNotAllowed"),
        ("open external url is only supported on Windows in v1", "err.openUrlUnsupported"),
        ("no recovery key configured", "err.noRecovery"),
        ("parent is not a folder", "err.parentNotFolder"),
        ("folder path too deep", "err.folderTooDeep"),
        ("path entry is not a folder", "err.pathNotFolder"),
        ("cannot move a folder into itself", "err.moveIntoSelf"),
        ("cannot move a folder into its own subfolder", "err.moveIntoChild"),
        ("use create_folder for folders", "err.useCreateFolder"),
        ("item is not a text file", "err.notText"),
        ("cannot export a folder as bytes", "err.cannotExportFolder"),
        ("folder hierarchy too deep", "err.hierarchyTooDeep"),
        ("no data dir", "err.noDataDir"),
        ("no parent", "err.noParent"),
        ("failed to read file", "err.readFailed"),
        ("read failed", "err.readFailedShort"),
        ("unsupported locale", "err.unsupportedLocale"),
        ("copy failed", "unlock.copyFailed"),
        ("Passwords do not match.", "unlock.pwMismatch"),
        ("Password must be at least 12 characters.", "unlock.pwShort"),
    ];
    if let Some((_, key)) = EXACT.iter().find(|(m, _)| *m == raw) {
        return tr(id, key);
    }
    let digits = |s: &str| s.chars().take_while(char::is_ascii_digit).collect::<String>();
    let num = |s: &str| number(id, s.parse().unwrap_or(0));
    let prefixes = [
        ("crypto error: ", "err.crypto"),
        ("io error: ", "err.io"),
        ("invalid base64: ", "err.badBase64"),
        ("open url: ", "err.openUrl"),
        ("atomic vault replace failed (saved files left untouched): ", "err.atomicReplace"),
        ("vault.json parse: ", "err.vaultParse"),
    ];
    for (prefix, key) in prefixes {
        if let Some(detail) = raw.strip_prefix(prefix) {
            return trv(id, key, &[("detail", detail)]);
        }
    }
    if let Some(rest) = raw.strip_prefix("file too large (max ") {
        let n = digits(rest);
        if !n.is_empty() && rest[n.len()..] == *" bytes)" {
            return trv(id, "err.tooLarge", &[("n", &num(&n))]);
        }
    }
    for dash in ["—", "-"] {
        if let Some(rest) = raw.strip_prefix(&format!("too many unlock attempts {dash} try again in ")) {
            let n = digits(rest);
            if !n.is_empty() && rest[n.len()..] == *"s" {
                return trv(id, "err.throttle", &[("n", &num(&n))]);
            }
        }
    }
    if let Some(rest) = raw.strip_prefix("Folder is not empty (") {
        let n = digits(rest);
        let tail = &rest[n.len()..];
        let end = "). Empty it first, or confirm deleting everything inside.";
        if !n.is_empty() && tail == format!(" item{end}") {
            return trv(id, "err.folderNotEmptyOne", &[("n", &num(&n))]);
        }
        if !n.is_empty() && tail == format!(" items{end}") {
            return trv(id, "err.folderNotEmptyMany", &[("n", &num(&n))]);
        }
    }
    if let Some(rest) = raw.strip_prefix("vault format version ") {
        let v = digits(rest);
        let tail = &rest[v.len()..];
        if !v.is_empty() && tail == " is invalid" {
            return trv(id, "err.versionInvalid", &[("v", &v)]);
        }
        if let Some(after) = tail.strip_prefix(" is newer than this app supports (") {
            let s = digits(after);
            if !s.is_empty()
                && after[s.len()..] == *") — upgrade SecretFolder; refusing to open so saved files are not rewritten"
            {
                return trv(id, "err.versionNewer", &[("v", &v), ("supported", &s)]);
            }
        }
    }
    raw.to_owned()
}

/// The language every part of the app shows (a shipped locale id). Reading it through
/// [`Lang::id`] or a [`Lang::k`] label tracks it, so labels redraw on a switch.
#[derive(Clone, Copy)]
pub struct Lang(pub Signal<String>);

impl Lang {
    pub fn new(id: &str) -> Lang {
        Lang(Signal::new(id.to_owned()))
    }

    /// The current id, tracked.
    pub fn id(&self) -> String {
        self.0.get()
    }

    /// The current id, not tracked (for one-off text: dialogs, messages).
    pub fn now(&self) -> String {
        untrack(|| self.0.get())
    }

    /// `key` in the current language, now.
    pub fn t(&self, key: &str) -> String {
        tr(&self.now(), key)
    }

    /// [`Lang::t`] with placeholders filled.
    pub fn tv(&self, key: &str, vars: &[(&str, &str)]) -> String {
        trv(&self.now(), key, vars)
    }

    /// A count in the current language's plural form.
    pub fn count(&self, base: &str, n: u64) -> String {
        count_in(&self.now(), base, n)
    }

    /// A whole number in the current language's way.
    pub fn num(&self, n: u64) -> String {
        number(&self.now(), n)
    }

    /// An error from the shipped code in the current language.
    pub fn err(&self, raw: &str) -> String {
        err_in(&self.now(), raw)
    }

    /// `key` as a live label: widgets that take a closure redraw it on a switch.
    pub fn k(self, key: &'static str) -> impl Fn() -> String + Clone + 'static {
        move || tr(&self.id(), key)
    }

    /// Names a text field (for assistive technology, and its placeholder when asked) in the
    /// current language, again after every switch.
    pub fn name_field(self, field: &remedy_ui::TextField, key: &'static str, placeholder: bool) {
        let f = field.clone();
        let _ = field.el().bind(move || {
            let text = tr(&self.id(), key);
            let f = f.clone().label(text.clone());
            if placeholder {
                let _ = f.placeholder(text);
            }
        });
    }
}

fn canonical(tag: &str) -> String {
    tag.trim().replace('_', "-")
}

/// The first supported language among `tags` (most preferred first), as the shipped
/// `detectLocale`: Portuguese is Brazilian, Simplified Chinese only (Traditional is skipped
/// and the next tag tried), English when nothing matches.
pub fn detect<S: AsRef<str>>(tags: &[S]) -> &'static str {
    for raw in tags {
        let tag = canonical(raw.as_ref());
        if tag.is_empty() {
            continue;
        }
        let lower = tag.to_lowercase();
        if let Some((id, _)) = LOCALES.iter().find(|(id, _)| id.to_lowercase() == lower) {
            return id;
        }
        let primary = lower.split('-').next().unwrap_or("");
        if primary == "pt" {
            return "pt-BR";
        }
        if primary == "zh" {
            let traditional =
                matches!(lower.as_str(), "zh-tw" | "zh-hk" | "zh-mo" | "zh-hant") || lower.contains("hant");
            if traditional {
                continue;
            }
            if lower == "zh" || lower.starts_with("zh-cn") || lower.starts_with("zh-hans") || lower.starts_with("zh-sg") {
                return "zh-CN";
            }
            continue;
        }
        if let Some((id, _)) = LOCALES.iter().find(|(id, _)| *id == primary) {
            return id;
        }
    }
    "en"
}

/// The person's display languages, most preferred first (what the webview gave 0.2.0 as
/// `navigator.languages`).
#[cfg(all(windows, not(test)))]
pub fn system_tags() -> Vec<String> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetUserPreferredUILanguages(flags: u32, num: *mut u32, buf: *mut u16, len: *mut u32) -> i32;
        fn GetUserDefaultLocaleName(buf: *mut u16, len: i32) -> i32;
    }
    const MUI_LANGUAGE_NAME: u32 = 0x8;
    let mut out = Vec::new();
    let (mut num, mut len) = (0u32, 0u32);
    // SAFETY: the documented size query (null buffer), then a buffer of the size it gave.
    let sized = unsafe { GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut num, std::ptr::null_mut(), &mut len) };
    if sized != 0 && len > 0 {
        let mut buf = vec![0u16; len as usize];
        // SAFETY: `buf` holds `len` UTF-16 units, as the call asked for.
        if unsafe { GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut num, buf.as_mut_ptr(), &mut len) } != 0 {
            out.extend(buf.split(|c| *c == 0).filter(|s| !s.is_empty()).map(String::from_utf16_lossy));
        }
    }
    let mut buf = [0u16; 85];
    // SAFETY: `buf` is 85 units (LOCALE_NAME_MAX_LENGTH), as passed.
    let n = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
    if n > 1 {
        out.push(String::from_utf16_lossy(&buf[..(n as usize) - 1]));
    }
    out
}

/// The person's languages from the environment (`LANGUAGE`, then `LC_ALL`, `LC_MESSAGES`,
/// `LANG`), without encodings or modifiers; "C" and "POSIX" say nothing.
#[cfg(all(not(windows), not(test)))]
pub fn system_tags() -> Vec<String> {
    let mut out = Vec::new();
    for var in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
        let Ok(v) = std::env::var(var) else { continue };
        for part in v.split(':') {
            let tag = part.split(['.', '@']).next().unwrap_or("").trim();
            if !tag.is_empty() && tag != "C" && tag != "POSIX" {
                out.push(tag.to_owned());
            }
        }
    }
    out
}

/// Tests see an English system whatever the machine says.
#[cfg(test)]
pub fn system_tags() -> Vec<String> {
    vec!["en-US".to_owned()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalog_parses_and_has_every_english_key() {
        let en = catalogs().get("en").expect("en");
        assert!(en.len() > 200, "{}", en.len());
        for (id, _) in LOCALES {
            let c = catalogs().get(id).unwrap_or_else(|| panic!("{id}"));
            let missing: Vec<_> = en.keys().filter(|k| !c.contains_key(*k)).collect();
            assert!(missing.is_empty(), "{id} lacks {missing:?}");
        }
        for (key, by_lang) in port() {
            for (id, _) in LOCALES {
                assert!(by_lang.get(id).is_some_and(|s| !s.is_empty()), "{key} in {id}");
            }
        }
    }

    #[test]
    fn lookups_fall_back_to_english_then_the_key() {
        assert_eq!(tr("de", "nav.settings"), "Einstellungen");
        assert_eq!(tr("de", "no.such.key"), "no.such.key");
        assert_eq!(tr("ja", "port.colSize"), "サイズ");
        assert_eq!(trv("en", "settings.idleSet", &[("count", "30")]), "Idle lock set to 30 min.");
    }

    #[test]
    fn plural_forms_follow_cldr() {
        for (l, n, want) in [
            ("en", 1, "one"),
            ("en", 2, "other"),
            ("en", 0, "other"),
            ("fr", 0, "one"),
            ("fr", 1, "one"),
            ("fr", 2, "other"),
            ("pt-BR", 0, "one"),
            ("ru", 1, "one"),
            ("ru", 21, "one"),
            ("ru", 11, "many"),
            ("ru", 3, "few"),
            ("ru", 12, "many"),
            ("pl", 1, "one"),
            ("pl", 21, "many"),
            ("pl", 22, "few"),
            ("pl", 5, "many"),
            ("ja", 1, "other"),
            ("es", 1_000_000, "many"),
        ] {
            assert_eq!(plural(l, n), want, "{l} {n}");
        }
        assert_eq!(count_in("en", "items", 1), "1 item");
        assert_eq!(count_in("en", "items", 3), "3 items");
        assert_eq!(count_in("ru", "items", 3), trv("ru", "items.few", &[("count", "3")]));
    }

    #[test]
    fn numbers_and_sizes_read_as_intl_wrote_them() {
        assert_eq!(number("en", 52_428_800), "52,428,800");
        assert_eq!(number("de", 1234), "1.234");
        assert_eq!(number("es", 1234), "1234");
        assert_eq!(number("es", 12_345), "12.345");
        assert_eq!(bytes("en", 512), "512 B");
        assert_eq!(bytes("en", 1536), "1.5 KB");
        assert_eq!(bytes("de", 1536), "1,5 KB");
        assert_eq!(bytes("en", 5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn errors_are_localized_as_shipped() {
        assert_eq!(err_in("en", "incorrect password or recovery key"), "incorrect password or recovery key");
        assert_ne!(err_in("de", "incorrect password or recovery key"), "incorrect password or recovery key");
        assert_eq!(err_in("de", "vault is locked"), tr("de", "err.locked"));
        assert_eq!(
            err_in("en", "too many unlock attempts — try again in 30s"),
            "too many unlock attempts — try again in 30s"
        );
        assert_eq!(
            err_in("de", "too many unlock attempts — try again in 30s"),
            trv("de", "err.throttle", &[("n", "30")])
        );
        assert_eq!(err_in("fr", "io error: disk full"), trv("fr", "err.io", &[("detail", "disk full")]));
        assert_eq!(
            err_in("en", "file too large (max 52428800 bytes)"),
            "file too large (max 52,428,800 bytes)"
        );
        assert_eq!(
            err_in("ja", "vault format version 9 is newer than this app supports (1) — upgrade SecretFolder; refusing to open so saved files are not rewritten"),
            trv("ja", "err.versionNewer", &[("v", "9"), ("supported", "1")])
        );
        assert_eq!(err_in("de", "something new"), "something new");
    }

    #[test]
    fn the_system_language_is_matched_as_shipped() {
        assert_eq!(detect(&["de-DE"]), "de");
        assert_eq!(detect(&["pt-PT"]), "pt-BR");
        assert_eq!(detect(&["zh-TW", "ja-JP"]), "ja");
        assert_eq!(detect(&["zh-Hans-CN"]), "zh-CN");
        assert_eq!(detect(&["zh_CN"]), "zh-CN");
        assert_eq!(detect(&["nl-NL", "fr-CA"]), "fr");
        assert_eq!(detect(&["nl-NL"]), "en");
        assert_eq!(detect::<&str>(&[]), "en");
    }
}
