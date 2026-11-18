//! Rules shared by every SecretFolder front end (the Tauri app and the RemedyUI app):
//! unlock throttling, and where files may be imported from and exported to.

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::error::{AppError, AppResult};

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

    /// Ends a lockout early (tests stand in for the clock).
    #[cfg(test)]
    pub fn expire_block(&mut self) {
        self.blocked_until = None;
    }

    pub fn record_success(&mut self) {
        self.failures.clear();
        self.blocked_until = None;
        self.strikes = 0;
    }
}

impl Default for UnlockThrottle {
    fn default() -> Self {
        Self::new()
    }
}

/// True if `path` is `base` or a strict child of `base` (component boundary).
/// Avoids prefix traps like `C:\Users\aliceevil` matching `C:\Users\alice`.
pub fn path_is_under(path: &Path, base: &Path) -> bool {
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
pub fn validate_import_path(path: &Path) -> AppResult<()> {
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
pub fn validate_export_path(path: &Path) -> AppResult<()> {
    let vault_root = crate::vault::default_vault_root()?;
    validate_export_path_with(path, &vault_root)
}

pub fn validate_export_path_with(path: &Path, vault_root: &Path) -> AppResult<()> {
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
