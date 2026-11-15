//! Plaintext UI prefs beside the vault. Language is needed before unlock,
//! so it must not live inside vault.json.

use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::error::{AppError, AppResult};

const LOCALES: &[&str] = &[
    "en", "es", "fr", "de", "pt-BR", "it", "ru", "pl", "ja", "ko", "zh-CN",
];

pub fn is_ui_locale(value: &str) -> bool {
    LOCALES.contains(&value)
}

pub fn tray_labels(locale: &str) -> (&'static str, &'static str, &'static str) {
    match locale {
        "es" => ("Mostrar", "Bloquear", "Salir"),
        "fr" => ("Afficher", "Verrouiller", "Quitter"),
        "de" => ("Einblenden", "Sperren", "Beenden"),
        "pt-BR" => ("Mostrar", "Bloquear", "Sair"),
        "it" => ("Mostra", "Blocca", "Esci"),
        "ru" => ("Показать", "Заблокировать", "Выход"),
        "pl" => ("Pokaż", "Zablokuj", "Zakończ"),
        "ja" => ("表示", "ロック", "終了"),
        "ko" => ("보이기", "잠금", "종료"),
        "zh-CN" => ("显示", "锁定", "退出"),
        _ => ("Show", "Lock", "Quit"),
    }
}

/// Empty or very long labels fall back so the tray menu stays usable.
pub fn clamp_tray_label(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 40 {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn ci_enabled_from(value: Option<&str>) -> bool {
    matches!(value.map(str::trim), Some("1") | Some("true"))
}

pub fn env_locale_from(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    if is_ui_locale(trimmed) {
        Some(trimmed.to_string())
    } else {
        None
    }
}

pub fn ci_enabled() -> bool {
    ci_enabled_from(std::env::var("SECRETFOLDER_CI").ok().as_deref())
}

pub fn env_locale() -> Option<String> {
    env_locale_from(std::env::var("SECRETFOLDER_LOCALE").ok().as_deref())
}

pub fn read_ui_locale(root: &Path) -> Option<String> {
    let text = fs::read_to_string(root.join("ui.json")).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let locale = value.get("locale")?.as_str()?;
    if is_ui_locale(locale) {
        Some(locale.to_string())
    } else {
        None
    }
}

/// Merge `locale` into an existing ui.json object. Other keys stay.
pub fn write_ui_locale(root: &Path, locale: &str) -> AppResult<()> {
    if !is_ui_locale(locale) {
        return Err(AppError::Message("unsupported locale".into()));
    }
    fs::create_dir_all(root)?;
    let path = root.join("ui.json");
    let mut map = if path.exists() {
        let text = fs::read_to_string(&path).unwrap_or_default();
        match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(map)) => map,
            _ => Map::new(),
        }
    } else {
        Map::new()
    };
    map.insert("locale".into(), Value::String(locale.to_string()));
    let body = serde_json::to_string_pretty(&Value::Object(map))
        .map_err(|e| AppError::Io(e.to_string()))?;
    fs::write(path, body)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_allowlist_is_exact() {
        assert!(is_ui_locale("pt-BR"));
        assert!(is_ui_locale("zh-CN"));
        assert!(!is_ui_locale("pt"));
        assert!(!is_ui_locale("zh-TW"));
        assert!(!is_ui_locale("en-US"));
    }

    #[test]
    fn tray_labels_match_catalogs() {
        assert_eq!(tray_labels("de"), ("Einblenden", "Sperren", "Beenden"));
        assert_eq!(tray_labels("ja"), ("表示", "ロック", "終了"));
        assert_eq!(tray_labels("es"), ("Mostrar", "Bloquear", "Salir"));
        assert_eq!(tray_labels("zh-CN"), ("显示", "锁定", "退出"));
        assert_eq!(tray_labels("en"), ("Show", "Lock", "Quit"));
        assert_eq!(tray_labels("nope"), ("Show", "Lock", "Quit"));
    }

    #[test]
    fn clamp_rejects_empty_and_long() {
        assert_eq!(clamp_tray_label("  ", "Show"), "Show");
        assert_eq!(clamp_tray_label(&"x".repeat(41), "Quit"), "Quit");
        assert_eq!(clamp_tray_label("  Salir  ", "Quit"), "Salir");
    }

    #[test]
    fn parsers_do_not_read_process_env() {
        assert!(ci_enabled_from(Some("1")));
        assert!(ci_enabled_from(Some(" true ")));
        assert!(!ci_enabled_from(Some("0")));
        assert!(!ci_enabled_from(None));
        assert_eq!(env_locale_from(Some(" ja ")).as_deref(), Some("ja"));
        assert_eq!(env_locale_from(Some("zh-TW")), None);
        assert_eq!(env_locale_from(None), None);
    }

    #[test]
    fn ui_json_merges_without_dropping_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ui.json");
        fs::write(&path, r#"{"theme":"dark","locale":"en"}"#).unwrap();
        write_ui_locale(dir.path(), "ja").unwrap();
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["locale"], "ja");
        assert_eq!(value["theme"], "dark");
        assert_eq!(read_ui_locale(dir.path()).as_deref(), Some("ja"));
        assert!(write_ui_locale(dir.path(), "zh-TW").is_err());
        assert_eq!(read_ui_locale(dir.path()).as_deref(), Some("ja"));
    }
}
