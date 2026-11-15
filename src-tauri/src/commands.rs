//! Tauri commands for SecretFolder vault session.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::{AppError, AppResult};
use crate::vault::{guess_mime, ItemDetail, ItemPreview, Vault, VaultStatus, MAX_FILE_BYTES};

/// Brute-force protection: sliding-window throttle on unlock attempts.
/// After FAIL_THRESHOLD failures within WINDOW_SECS, the caller is blocked
/// for escalating cooldown periods.
pub struct UnlockThrottle {
    failures: Vec<Instant>,
    blocked_until: Option<Instant>,
    /// Lockouts imposed since the last successful unlock (drives escalation).
    strikes: usize,
}

impl UnlockThrottle {
    const WINDOW_SECS: u64 = 60;
    const FAIL_THRESHOLD: usize = 5;
    const COOLDOWN_STEPS: &'static [u64] = &[10, 30, 60, 300]; // seconds

    pub fn new() -> Self {
        Self {
            failures: Vec::new(),
            blocked_until: None,
            strikes: 0,
        }
    }

    /// Returns Ok if allowed, Err with wait-seconds message if blocked.
    pub fn check(&mut self) -> AppResult<()> {
        // Prune expired entries from the window. `checked_sub` can be None
        // shortly after boot (monotonic clock origin) — keep everything then
        // instead of panicking and poisoning the throttle mutex.
        let window = std::time::Duration::from_secs(Self::WINDOW_SECS);
        if let Some(cutoff) = Instant::now().checked_sub(window) {
            self.failures.retain(|t| *t > cutoff);
        }

        // Check if currently blocked.
        if let Some(until) = self.blocked_until {
            if Instant::now() < until {
                let remain = until.saturating_duration_since(Instant::now()).as_secs();
                return Err(AppError::Message(format!(
                    "too many unlock attempts — try again in {}s",
                    remain
                )));
            }
            self.blocked_until = None;
        }

        if self.failures.len() >= Self::FAIL_THRESHOLD {
            // Escalate: each lockout since the last success moves up a tier
            // (failures are cleared per lockout, so they cannot drive this).
            let tier = self.strikes.min(Self::COOLDOWN_STEPS.len() - 1);
            self.strikes = self.strikes.saturating_add(1);
            let cooldown = Self::COOLDOWN_STEPS[tier];
            self.blocked_until =
                Instant::now().checked_add(std::time::Duration::from_secs(cooldown));
            self.failures.clear();
            return Err(AppError::Message(format!(
                "too many unlock attempts — try again in {}s",
                cooldown
            )));
        }
        Ok(())
    }

    pub fn record_failure(&mut self) {
        self.failures.push(Instant::now());
    }

    pub fn record_success(&mut self) {
        self.failures.clear();
        self.blocked_until = None;
        self.strikes = 0;
    }
}

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

/// True if `path` is `base` or a strict child of `base` (component boundary).
/// Avoids prefix traps like `C:\Users\aliceevil` matching `C:\Users\alice`.
fn path_is_under(path: &Path, base: &Path) -> bool {
    let mut path_iter = path.components();
    for base_comp in base.components() {
        match path_iter.next() {
            Some(c) if paths_component_eq(&c, &base_comp) => {}
            _ => return false,
        }
    }
    true
}

fn paths_component_eq(a: &std::path::Component<'_>, b: &std::path::Component<'_>) -> bool {
    use std::path::Component;
    match (a, b) {
        (Component::Prefix(ap), Component::Prefix(bp)) => {
            ap.as_os_str().eq_ignore_ascii_case(bp.as_os_str())
        }
        (Component::RootDir, Component::RootDir) => true,
        (Component::Normal(an), Component::Normal(bn)) => an.eq_ignore_ascii_case(bn),
        (Component::CurDir, Component::CurDir) => true,
        (Component::ParentDir, Component::ParentDir) => true,
        _ => false,
    }
}

fn path_allowed_under_any(path: &Path, allowed: &[PathBuf]) -> bool {
    for base in allowed {
        if let Ok(base_canonical) = base.canonicalize() {
            if path_is_under(path, &base_canonical) {
                return true;
            }
        } else if path_is_under(path, base) {
            return true;
        }
    }
    false
}

fn allowed_user_bases() -> Vec<PathBuf> {
    let mut allowed = Vec::new();
    if let Some(home) = dirs::home_dir() {
        for sub in &["Desktop", "Documents", "Downloads"] {
            allowed.push(home.join(sub));
        }
        allowed.push(home);
    }
    allowed.push(std::env::temp_dir());
    if let Ok(vault_root) = crate::vault::default_vault_root() {
        allowed.push(vault_root);
    }
    allowed
}

/// Validate that an import path is within the user's home directory
/// or standard data directories (Desktop, Documents, Downloads)
/// to prevent exfiltration of system files via the vault.
fn validate_import_path(path: &Path) -> AppResult<()> {
    let canonical = path
        .canonicalize()
        .map_err(|_| AppError::Message("path does not exist or cannot be resolved".into()))?;

    if path_allowed_under_any(&canonical, &allowed_user_bases()) {
        return Ok(());
    }

    Err(AppError::Message(
        "import path must be under your home directory (Desktop, Documents, or Downloads)".into(),
    ))
}

/// Validate that an export path is within user home/data directories
/// to prevent writing vault content to system locations.
fn validate_export_path(path: &Path) -> AppResult<()> {
    let vault_root = crate::vault::default_vault_root()?;
    validate_export_path_with(path, &vault_root)
}

fn validate_export_path_with(path: &Path, vault_root: &Path) -> AppResult<()> {
    // If the path doesn't exist yet, resolve its parent
    let target = if path.exists() {
        path.to_path_buf()
    } else if let Some(parent) = path.parent() {
        parent.to_path_buf()
    } else {
        return Err(AppError::Message("invalid export path".into()));
    };

    let canonical = target
        .canonicalize()
        .map_err(|_| AppError::Message("export path cannot be resolved".into()))?;

    // Never write decrypted content into the vault's own data directory:
    // it would sit there in plaintext and could overwrite vault.json / blobs.
    let in_vault = match vault_root.canonicalize() {
        Ok(root) => path_is_under(&canonical, &root),
        Err(_) => path_is_under(&canonical, vault_root),
    };
    if in_vault {
        return Err(AppError::Message(
            "cannot export into the SecretFolder data folder — choose another location".into(),
        ));
    }

    if path_allowed_under_any(&canonical, &allowed_user_bases()) {
        return Ok(());
    }

    Err(AppError::Message(
        "export path must be under your home directory (Desktop, Documents, or Downloads)".into(),
    ))
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

/// Show main window (tray). Hidden CI runs never focus the desktop.
#[tauri::command]
pub fn show_main_window(app: AppHandle) -> AppResult<()> {
    if crate::prefs::ci_enabled() {
        return Ok(());
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootInfo {
    pub saved_locale: Option<String>,
    pub env_locale: Option<String>,
    pub ci: bool,
}

#[tauri::command]
pub fn boot_info(state: State<'_, AppState>) -> BootInfo {
    let saved_locale = state
        .vault
        .lock()
        .ok()
        .and_then(|vault| crate::prefs::read_ui_locale(vault.data_root()));
    BootInfo {
        saved_locale,
        env_locale: crate::prefs::env_locale(),
        ci: crate::prefs::ci_enabled(),
    }
}

#[tauri::command]
pub fn set_ui_locale(state: State<'_, AppState>, locale: String) -> AppResult<()> {
    let vault = state
        .vault
        .lock()
        .map_err(|_| AppError::Message("vault lock poisoned".into()))?;
    crate::prefs::write_ui_locale(vault.data_root(), &locale)
}

#[tauri::command]
pub fn set_tray_labels(app: AppHandle, show: String, lock: String, quit: String) -> AppResult<()> {
    if crate::prefs::ci_enabled() {
        return Ok(());
    }
    crate::install_tray(&app, &show, &lock, &quit).map_err(|e| AppError::Message(e.to_string()))
}

/// Headless smoke: write the UI text, then exit. No-op outside CI.
#[tauri::command]
pub fn ci_report(app: AppHandle, text: String) -> AppResult<()> {
    if !crate::prefs::ci_enabled() {
        return Ok(());
    }
    if let Ok(path) = std::env::var("SECRETFOLDER_CI_OUT") {
        let path = path.trim();
        if !path.is_empty() {
            let _ = std::fs::write(path, text);
        }
    }
    crate::request_exit();
    app.exit(0);
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
        t.blocked_until = None;
        // Record another batch to re-trigger threshold and escalate cooldown
        for _ in 0..5 {
            t.record_failure();
        }
        let err2 = t.check().unwrap_err().to_string();
        assert!(
            err2.contains("in 30s"),
            "second lockout should escalate: {err2}"
        );

        t.blocked_until = None;
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
            t.blocked_until = None;
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
