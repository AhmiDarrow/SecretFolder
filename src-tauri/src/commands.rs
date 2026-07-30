//! Tauri commands for SecretFolder vault session.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::{AppError, AppResult};
use crate::vault::{guess_mime, ItemDetail, ItemPreview, Vault, VaultStatus, MAX_FILE_BYTES};

pub struct AppState {
    pub vault: Mutex<Vault>,
}

impl AppState {
    pub fn new() -> AppResult<Self> {
        Ok(Self {
            vault: Mutex::new(Vault::open_default()?),
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
    with_vault(&state, |v| v.unlock(&password))?;
    emit_items_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn unlock_with_recovery(
    app: AppHandle,
    state: State<'_, AppState>,
    recovery_key: String,
) -> AppResult<()> {
    with_vault(&state, |v| v.unlock_with_recovery(&recovery_key))?;
    emit_items_changed(&app);
    Ok(())
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

/// Import a file from an absolute path on disk into the vault.
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
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&p, data)?;
    Ok(())
}

/// Return decrypted bytes as base64 for client-side download/save.
#[tauri::command]
pub fn export_bytes_b64(state: State<'_, AppState>, id: String) -> AppResult<ExportPayload> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    let (name, data) = with_vault(&state, |v| v.export_bytes(&id))?;
    Ok(ExportPayload {
        name,
        data_b64: B64.encode(data),
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPayload {
    pub name: String,
    pub data_b64: String,
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
    "https://github.com/AhmiTD/SecretFolder",
    "https://github.com/AhmiTD/SecretFolder/releases",
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
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| AppError::Message(format!("open url: {e}")))?;
        return Ok(());
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
        };
        {
            let mut g = state.vault.lock().unwrap();
            g.setup("password123").unwrap();
            g.create_text("a.txt", "hi", None).unwrap();
        }
        {
            let mut g = state.vault.lock().unwrap();
            assert_eq!(g.list_items().unwrap().len(), 1);
        }
    }
}
