//! Tauri commands for SecretFolder vault session.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::{AppError, AppResult};
#[cfg(test)]
use crate::policy::{path_is_under, validate_export_path_with};
use crate::policy::{validate_export_path, validate_import_path, UnlockThrottle};
use crate::vault::{guess_mime, ItemDetail, ItemPreview, Vault, VaultStatus, MAX_FILE_BYTES};

pub struct AppState {
    pub vault: Mutex<Vault>,
    pub unlock_throttle: Mutex<UnlockThrottle>,
}

impl AppState {
    pub fn new() -> AppResult<Self> {
        Ok(Self {
            vault: Mutex::new(Vault::open_default()?),
            unlock_throttle: Mutex::new(UnlockThrottle::new()),
        })
    }
}

fn with_vault<F, T>(state: &State<'_, AppState>, f: F) -> AppResult<T>
where
    F: FnOnce(&mut Vault) -> AppResult<T>,
{
    let mut guard = state
        .vault
        .lock()
        .map_err(|_| AppError::Message("vault lock poisoned".into()))?;
    if guard.check_idle_lock() {
        return Err(AppError::Locked);
    }
    f(&mut guard)
}

fn emit_items_changed(app: &AppHandle) {
    let _ = app.emit("items-changed", ());
}

#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> AppResult<VaultStatus> {
    let guard = state
        .vault
        .lock()
        .map_err(|_| AppError::Message("vault lock poisoned".into()))?;
    Ok(guard.status())
}

#[tauri::command]
pub fn setup_vault(state: State<'_, AppState>, password: String) -> AppResult<String> {
    // Do not emit items-changed here: the UI must show the one-time recovery
    // key before any unlock→explorer transition. Caller refreshes after confirm.
    with_vault(&state, |v| v.setup(&password))
}

#[tauri::command]
pub fn unlock_vault(app: AppHandle, state: State<'_, AppState>, password: String) -> AppResult<()> {
    // Rate-limit check
    {
        let mut throttle = state
            .unlock_throttle
            .lock()
            .map_err(|_| AppError::Message("throttle lock poisoned".into()))?;
        throttle.check()?;
    }

    let result = with_vault(&state, |v| v.unlock(&password));
    match &result {
        Ok(_) => {
            // Reset throttle on success
            if let Ok(mut throttle) = state.unlock_throttle.lock() {
                throttle.record_success();
            }
            emit_items_changed(&app);
        }
        Err(_) => {
            // Count all crypto/auth failures toward lockout (not only BadPassword).
            if let Ok(mut throttle) = state.unlock_throttle.lock() {
                throttle.record_failure();
            }
        }
    }
    result
}

#[tauri::command]
pub fn unlock_with_recovery(
    app: AppHandle,
    state: State<'_, AppState>,
    recovery_key: String,
) -> AppResult<()> {
    // Rate-limit check
    {
        let mut throttle = state
            .unlock_throttle
            .lock()
            .map_err(|_| AppError::Message("throttle lock poisoned".into()))?;
        throttle.check()?;
    }

    let result = with_vault(&state, |v| v.unlock_with_recovery(&recovery_key));
    match &result {
        Ok(_) => {
            if let Ok(mut throttle) = state.unlock_throttle.lock() {
                throttle.record_success();
            }
            emit_items_changed(&app);
        }
        Err(_) => {
            // Count all crypto/auth failures toward lockout (not only BadPassword).
            if let Ok(mut throttle) = state.unlock_throttle.lock() {
                throttle.record_failure();
            }
        }
    }
    result
}

#[tauri::command]
pub fn lock_vault(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let mut guard = state
        .vault
        .lock()
        .map_err(|_| AppError::Message("vault lock poisoned".into()))?;
    guard.lock();
    let _ = app.emit("vault-locked", ());
    Ok(())
}

#[tauri::command]
pub fn touch_activity(state: State<'_, AppState>) -> AppResult<()> {
    let mut guard = state
        .vault
        .lock()
        .map_err(|_| AppError::Message("vault lock poisoned".into()))?;
    if guard.check_idle_lock() {
        return Err(AppError::Locked);
    }
    guard.touch();
    Ok(())
}

#[tauri::command]
pub fn set_idle_lock_secs(state: State<'_, AppState>, secs: u64) -> AppResult<()> {
    with_vault(&state, |v| v.set_idle_lock_secs(secs))
}

#[tauri::command]
pub fn change_password(
    state: State<'_, AppState>,
    current_password: String,
    new_password: String,
) -> AppResult<()> {
    with_vault(&state, |v| {
        v.change_password(&current_password, &new_password)
    })
}

#[tauri::command]
pub fn list_items(
    state: State<'_, AppState>,
    parent_id: Option<String>,
) -> AppResult<Vec<ItemPreview>> {
    with_vault(&state, |v| v.list_items_in(parent_id))
}

#[tauri::command]
pub fn folder_path(
    state: State<'_, AppState>,
    folder_id: Option<String>,
) -> AppResult<Vec<ItemPreview>> {
    with_vault(&state, |v| v.folder_path(folder_id.as_deref()))
}

#[tauri::command]
pub fn get_item(state: State<'_, AppState>, id: String) -> AppResult<ItemDetail> {
    with_vault(&state, |v| v.get_item(&id))
}

#[tauri::command]
pub fn create_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    parent_id: Option<String>,
) -> AppResult<ItemPreview> {
    let item = with_vault(&state, |v| v.create_folder(&name, parent_id))?;
    emit_items_changed(&app);
    Ok(item)
}

#[tauri::command]
pub fn move_item(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    parent_id: Option<String>,
) -> AppResult<ItemPreview> {
    let item = with_vault(&state, |v| v.move_item(&id, parent_id))?;
    emit_items_changed(&app);
    Ok(item)
}

#[tauri::command]
pub fn create_text(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    body: String,
    parent_id: Option<String>,
) -> AppResult<ItemPreview> {
    let item = with_vault(&state, |v| v.create_text(&name, &body, parent_id))?;
    emit_items_changed(&app);
    Ok(item)
}

#[tauri::command]
pub fn update_text(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
    body: String,
) -> AppResult<ItemPreview> {
    let item = with_vault(&state, |v| v.update_text(&id, name, body))?;
    emit_items_changed(&app);
    Ok(item)
}

#[tauri::command]
pub fn rename_item(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> AppResult<ItemPreview> {
    let item = with_vault(&state, |v| v.rename_item(&id, &name))?;
    emit_items_changed(&app);
    Ok(item)
}

#[tauri::command]
pub fn folder_content_count(state: State<'_, AppState>, id: String) -> AppResult<usize> {
    with_vault(&state, |v| v.folder_content_count(&id))
}

#[tauri::command]
pub fn delete_item(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    cascade: Option<bool>,
) -> AppResult<()> {
    with_vault(&state, |v| v.delete_item(&id, cascade.unwrap_or(false)))?;
    emit_items_changed(&app);
    Ok(())
}

/// Import a file from an absolute path (must be under allowed user dirs).
#[tauri::command]
pub fn import_path(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    name: Option<String>,
    parent_id: Option<String>,
) -> AppResult<ItemPreview> {
    let p = PathBuf::from(&path);
    if !p.is_file() {
        return Err(AppError::Message("path is not a file".into()));
    }
    validate_import_path(&p)?;
    let meta = fs::metadata(&p)?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(AppError::TooLarge(MAX_FILE_BYTES));
    }
    let data = fs::read(&p)?;
    let display_name = name.unwrap_or_else(|| {
        p.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file")
            .to_string()
    });
    let mime = guess_mime(&display_name);
    let item = with_vault(&state, |v| {
        v.import_bytes(display_name, mime, data, parent_id)
    })?;
    emit_items_changed(&app);
    Ok(item)
}

/// Import raw bytes (base64) — used when the UI reads a local File via webview.
#[tauri::command]
pub fn import_bytes(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    mime: String,
    data_b64: String,
    parent_id: Option<String>,
) -> AppResult<ItemPreview> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    let data = B64
        .decode(data_b64.trim())
        .map_err(|e| AppError::Message(format!("invalid base64: {e}")))?;
    if data.len() as u64 > MAX_FILE_BYTES {
        return Err(AppError::TooLarge(MAX_FILE_BYTES));
    }
    let mime = if mime.is_empty() {
        guess_mime(&name)
    } else {
        mime
    };
    let item = with_vault(&state, |v| v.import_bytes(name, mime, data, parent_id))?;
    emit_items_changed(&app);
    Ok(item)
}

/// Export item bytes to an absolute path chosen by the UI.
#[tauri::command]
pub fn export_path(state: State<'_, AppState>, id: String, path: String) -> AppResult<()> {
    let (_name, data) = with_vault(&state, |v| v.export_bytes(&id))?;
    let p = PathBuf::from(&path);
    validate_export_path(&p)?;
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&p, data)?;
    Ok(())
}

#[tauri::command]
pub fn max_file_bytes() -> u64 {
    MAX_FILE_BYTES
}

/// Show main window (tray).
#[tauri::command]
pub fn show_main_window(app: AppHandle) -> AppResult<()> {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    Ok(())
}

/// Hide main window to tray.
#[tauri::command]
pub fn hide_main_window(app: AppHandle) -> AppResult<()> {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
    Ok(())
}

/// Fully quit the app (locks vault first). Used by UI Quit buttons.
#[tauri::command]
pub fn quit_app(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    {
        let mut guard = state
            .vault
            .lock()
            .map_err(|_| AppError::Message("vault lock poisoned".into()))?;
        guard.lock();
    }
    crate::request_exit();
    app.exit(0);
    Ok(())
}

const ALLOWED_EXTERNAL_URLS: &[&str] = &[
    "https://github.com/AhmiDarrow",
    "https://github.com/AhmiDarrow/SecretFolder",
    "https://github.com/AhmiDarrow/SecretFolder/releases",
    "https://www.patreon.com/cw/AhmiDarrow",
];

/// Open a small allow-listed URL in the system browser (About links).
#[tauri::command]
pub fn open_external_url(url: String) -> AppResult<()> {
    let trimmed = url.trim();
    if !ALLOWED_EXTERNAL_URLS.contains(&trimmed) {
        return Err(AppError::Message("url not allowed".into()));
    }
    open_url_in_browser(trimmed)
}

fn open_url_in_browser(url: &str) -> AppResult<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW — avoid a flashing console when launching the browser.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        // rundll32 FileProtocolHandler is the reliable association path under
        // CREATE_NO_WINDOW (cmd `start` often no-ops when the console is hidden).
        let primary = std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
        if primary.is_ok() {
            return Ok(());
        }

        // Fallbacks for unusual PATH / lockdown environments.
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
        std::process::Command::new("explorer.exe")
            .arg(url)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| AppError::Message(format!("open url: {e}")))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = url;
        Err(AppError::Message(
            "open external url is only supported on Windows in v1".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::Vault;
    use tempfile::tempdir;

    #[test]
    fn max_constant_exposed() {
        assert_eq!(max_file_bytes(), MAX_FILE_BYTES);
    }

    #[test]
    fn vault_state_roundtrip_via_mutex() {
        let dir = tempdir().unwrap();
        let state = AppState {
            vault: Mutex::new(Vault::open_path_for_test(dir.path().to_path_buf()).unwrap()),
            unlock_throttle: Mutex::new(UnlockThrottle::new()),
        };
        {
            let mut g = state.vault.lock().unwrap();
            g.setup("password1234").unwrap();
            g.create_text("a.txt", "hi", None).unwrap();
        }
        {
            let mut g = state.vault.lock().unwrap();
            assert_eq!(g.list_items().unwrap().len(), 1);
        }
    }

    #[test]
    fn unlock_throttle_allows_under_threshold() {
        let mut t = UnlockThrottle::new();
        for _ in 0..4 {
            assert!(t.check().is_ok());
            t.record_failure();
        }
        assert!(t.check().is_ok());
    }

    #[test]
    fn unlock_throttle_blocks_at_threshold() {
        let mut t = UnlockThrottle::new();
        for _ in 0..5 {
            t.record_failure();
        }
        // SF check() applies block when failures >= threshold
        let err = t.check().unwrap_err().to_string();
        assert!(
            err.to_lowercase().contains("too many") || err.to_lowercase().contains("try again"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn unlock_throttle_success_resets() {
        let mut t = UnlockThrottle::new();
        for _ in 0..4 {
            t.record_failure();
        }
        t.record_success();
        assert!(t.check().is_ok());
        for _ in 0..4 {
            t.record_failure();
        }
        assert!(t.check().is_ok());
    }

    #[test]
    fn validate_import_path_allows_temp_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("safe-import.bin");
        fs::write(&file, b"data").unwrap();
        assert!(validate_import_path(&file).is_ok());
    }

    #[test]
    fn validate_import_path_rejects_windows_system() {
        // System32 should never be an allowed import source.
        let p = PathBuf::from(r"C:\Windows\System32\drivers\etc\hosts");
        if p.exists() {
            let err = validate_import_path(&p).unwrap_err().to_string();
            assert!(
                err.to_lowercase().contains("home") || err.to_lowercase().contains("import"),
                "unexpected: {err}"
            );
        }
    }

    #[test]
    fn validate_export_path_allows_temp() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("export-out.bin");
        // parent exists (tempdir); file need not exist yet
        assert!(validate_export_path(&out).is_ok());
    }

    #[test]
    fn validate_export_path_rejects_windows_system() {
        let p = PathBuf::from(r"C:\Windows\System32\evil-export.bin");
        // Parent System32 exists on Windows
        if PathBuf::from(r"C:\Windows\System32").exists() {
            let err = validate_export_path(&p).unwrap_err().to_string();
            assert!(
                err.to_lowercase().contains("home") || err.to_lowercase().contains("export"),
                "unexpected: {err}"
            );
        }
    }

    #[test]
    fn external_url_allowlist_rejects_unknown() {
        for url in [
            "https://evil.example",
            "javascript:alert(1)",
            "file:///C:/Windows/System32",
            "https://github.com/AhmiDarrow/SecretFolder/wiki",
            "",
        ] {
            let err = open_external_url(url.to_string()).unwrap_err().to_string();
            assert!(err.contains("not allowed"), "url={url:?} err={err}");
        }
    }

    #[test]
    fn external_url_allowlist_membership() {
        for url in ALLOWED_EXTERNAL_URLS {
            assert!(ALLOWED_EXTERNAL_URLS.contains(url));
            assert!(url.starts_with("https://"), "{url}");
            assert!(
                url.contains("AhmiDarrow"),
                "stale or unexpected allowlist entry: {url}"
            );
            assert!(!url.contains("AhmiTD"), "stale org: {url}");
        }
        assert!(!ALLOWED_EXTERNAL_URLS.contains(&"https://github.com/AhmiDarrow/"));
    }

    #[test]
    fn validate_import_path_rejects_missing_file() {
        let p = PathBuf::from(r"C:\nonexistent\sf-missing-no-such-file-xyz.bin");
        assert!(validate_import_path(&p).is_err());
    }

    #[test]
    fn unlock_throttle_block_escalates_cooldown() {
        let mut t = UnlockThrottle::new();
        for _ in 0..5 {
            t.record_failure();
        }
        // First block: cooldown step [0] = 10 seconds
        let err1 = t.check().unwrap_err().to_string();
        assert!(
            err1.contains("10") || err1.contains("try again"),
            "msg: {err1}"
        );
        // Simulate waiting past first cooldown
        t.expire_block();
        // Record another batch to re-trigger threshold and escalate cooldown
        for _ in 0..5 {
            t.record_failure();
        }
        let err2 = t.check().unwrap_err().to_string();
        assert!(
            err2.contains("in 30s"),
            "second lockout should escalate: {err2}"
        );

        t.expire_block();
        for _ in 0..5 {
            t.record_failure();
        }
        let err3 = t.check().unwrap_err().to_string();
        assert!(
            err3.contains("in 60s"),
            "third lockout should escalate: {err3}"
        );

        // Success resets escalation.
        t.record_success();
        for _ in 0..5 {
            t.record_failure();
        }
        let err4 = t.check().unwrap_err().to_string();
        assert!(err4.contains("in 10s"), "reset after success: {err4}");
    }

    #[test]
    fn unlock_throttle_cooldown_caps_at_max_tier() {
        let mut t = UnlockThrottle::new();
        let mut last = String::new();
        for _ in 0..10 {
            t.expire_block();
            for _ in 0..5 {
                t.record_failure();
            }
            last = t.check().unwrap_err().to_string();
        }
        assert!(last.contains("in 300s"), "msg: {last}");
    }

    #[test]
    fn validate_export_path_rejects_vault_data_dir() {
        let home = tempdir().unwrap();
        let vault_root = home.path().join("com.ahmi.secretfolder");
        fs::create_dir_all(vault_root.join("blobs")).unwrap();
        fs::write(vault_root.join("vault.json"), b"{}").unwrap();
        for target in [
            vault_root.join("vault.json"),
            vault_root.join("exported.txt"),
            vault_root.join("blobs").join("x.bin"),
        ] {
            let err = validate_export_path_with(&target, &vault_root)
                .unwrap_err()
                .to_string();
            assert!(err.contains("data folder"), "target={target:?} err={err}");
        }
        // A sibling folder next to the vault is still fine.
        let ok = home.path().join("out.txt");
        assert!(validate_export_path_with(&ok, &vault_root).is_ok());
    }

    #[test]
    fn max_file_bytes_computed_correctly() {
        assert_eq!(MAX_FILE_BYTES, 25 * 1024 * 1024);
    }

    #[test]
    fn path_is_under_rejects_prefix_sibling() {
        let base = PathBuf::from(r"C:\Users\alice");
        let child = PathBuf::from(r"C:\Users\alice\Documents\x.txt");
        let sibling = PathBuf::from(r"C:\Users\aliceevil\x.txt");
        assert!(path_is_under(&child, &base));
        assert!(!path_is_under(&sibling, &base));
        assert!(path_is_under(&base, &base));
    }
}

#[cfg(test)]
mod path_boundary_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn path_is_under_respects_component_boundary() {
        let base = PathBuf::from(r"C:\Users\alice");
        let child = PathBuf::from(r"C:\Users\alice\Documents\f.txt");
        let sibling_prefix = PathBuf::from(r"C:\Users\aliceevil\f.txt");
        assert!(path_is_under(&child, &base));
        assert!(!path_is_under(&sibling_prefix, &base));
        assert!(path_is_under(&base, &base));
    }
}
