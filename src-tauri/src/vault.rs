//! On-disk encrypted file vault.
//!
//! Layout (app data dir, never shared with SecretSticky):
//!   vault.json          — header + encrypted item index (names encrypted)
//!   vault.json.bak      — last-known-good sibling before each replace
//!   blobs/<uuid>.bin    — per-file ciphertext (XChaCha20-Poly1305)
//!
//! AAD namespace: secretfolder-*
//!
//! # Durability (non-negotiable)
//!
//! - Installers and auto-update replace the **app binary only**. They must never
//!   touch `%APPDATA%\com.ahmi.secretfolder\`.
//! - Saves use temp + OS replace-in-place (Windows: `MoveFileExW` REPLACE_EXISTING
//!   | WRITE_THROUGH). **Never** delete the live `vault.json` before the new bytes
//!   are durable at the destination.
//! - A last-known-good `vault.json.bak` is snapshotted before each replace; boot
//!   restores from `.bak` if the live file is missing or unreadable.
//! - Failed decrypt / locked vault / ACL deny must error clearly — never replace
//!   the vault with an empty one “to fix” load errors.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use zeroize::Zeroize;

use crate::crypto::{
    self, derive_key, generate_recovery_key, normalize_recovery_key, MasterKey, ARGON2_M_KIB,
    ARGON2_P, ARGON2_T, SALT_LEN,
};
use crate::error::{AppError, AppResult};

const VAULT_VERSION: u32 = 1;
const DEFAULT_IDLE_LOCK_SECS: u64 = 15 * 60;
/// v1 hard cap — load-all-in-memory encrypt; no full video vault promise.
pub const MAX_FILE_BYTES: u64 = 25 * 1024 * 1024; // 25 MiB
/// Images larger than this are not rendered inline as base64 data URLs.
/// User must export/open externally instead.
const MAX_PREVIEW_SIZE: u64 = 5 * 1024 * 1024; // 5 MiB

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    pub initialized: bool,
    pub unlocked: bool,
    pub item_count: usize,
    pub idle_lock_secs: u64,
    pub has_recovery_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemPreview {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub kind: ItemKind,
    /// Parent folder id; None = vault root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    Text,
    Image,
    Binary,
    Folder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDetail {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub kind: ItemKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// UTF-8 text when kind == Text; otherwise null.
    pub text: Option<String>,
    /// data URL (base64) when kind == Image; otherwise null.
    pub data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VaultHeader {
    version: u32,
    salt_b64: String,
    verifier_b64: String,
    recovery_verifier_b64: Option<String>,
    wrapped_master_b64: Option<String>,
    password_wrapped_key_b64: Option<String>,
    argon2_m_kib: u32,
    argon2_t: u32,
    argon2_p: u32,
    idle_lock_secs: u64,
}

/// Encrypted index entry — ciphertext holds ItemPlain JSON (includes real name).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EncIndexEntry {
    id: String,
    ciphertext_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VaultFile {
    header: VaultHeader,
    items: Vec<EncIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Zeroize)]
#[zeroize(drop)]
struct ItemPlain {
    id: String,
    name: String,
    mime: String,
    size: u64,
    #[zeroize(skip)]
    kind: ItemKind,
    /// Parent folder id; None = vault root. Missing in older vaults = root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent_id: Option<String>,
    created_at: String,
    updated_at: String,
}

struct Session {
    key: MasterKey,
    items: HashMap<String, ItemPlain>,
    idle_lock_secs: u64,
    last_activity: Instant,
    has_recovery_key: bool,
}

impl Drop for Session {
    fn drop(&mut self) {
        // Best-effort wipe of plaintext item cache on lock/drop.
        for item in self.items.values_mut() {
            item.zeroize();
        }
        self.items.clear();
    }
}

pub struct Vault {
    #[allow(dead_code)]
    root: PathBuf,
    index_path: PathBuf,
    blobs_dir: PathBuf,
    file: Option<VaultFile>,
    session: Option<Session>,
}

impl Vault {
    pub fn open_default() -> AppResult<Self> {
        let root = default_vault_root()?;
        Self::open_root(root)
    }

    pub fn open_root(root: PathBuf) -> AppResult<Self> {
        fs::create_dir_all(&root)?;
        let blobs_dir = root.join("blobs");
        fs::create_dir_all(&blobs_dir)?;
        let index_path = root.join("vault.json");
        // Prefer the live vault. If it is missing (crash mid-replace on older
        // builds) but a sibling `.bak` exists, restore it — never treat that as
        // "uninitialized" and allow setup to mint an empty vault over secrets.
        let bak = backup_path_for(&index_path);
        if !index_path.exists() && bak.exists() {
            restore_backup(&bak, &index_path)?;
        }

        let file = if index_path.exists() {
            match load_vault_file(&index_path) {
                Ok(f) => Some(f),
                Err(primary_err) => {
                    // Corrupt/unreadable primary: try last-known-good backup.
                    // Never delete the primary here — operator can recover offline.
                    if bak.exists() {
                        match load_vault_file(&bak) {
                            Ok(f) => {
                                let _ = restore_backup(&bak, &index_path);
                                Some(f)
                            }
                            Err(_) => return Err(primary_err),
                        }
                    } else {
                        return Err(primary_err);
                    }
                }
            }
        } else {
            None
        };
        Ok(Self {
            root,
            index_path,
            blobs_dir,
            file,
            session: None,
        })
    }

    #[cfg(test)]
    pub fn open_path_for_test(root: PathBuf) -> AppResult<Self> {
        Self::open_root(root)
    }

    pub fn status(&self) -> VaultStatus {
        let (unlocked, item_count, idle, has_rec) = if let Some(s) = &self.session {
            (true, s.items.len(), s.idle_lock_secs, s.has_recovery_key)
        } else if let Some(f) = &self.file {
            (
                false,
                f.items.len(),
                f.header.idle_lock_secs,
                f.header.wrapped_master_b64.is_some(),
            )
        } else {
            (false, 0, DEFAULT_IDLE_LOCK_SECS, false)
        };
        VaultStatus {
            initialized: self.file.is_some(),
            unlocked,
            item_count,
            idle_lock_secs: idle,
            has_recovery_key: has_rec,
        }
    }

    pub fn touch(&mut self) {
        if let Some(s) = &mut self.session {
            s.last_activity = Instant::now();
        }
    }

    /// Returns true if session was locked due to idle timeout.
    pub fn check_idle_lock(&mut self) -> bool {
        let should = if let Some(s) = &self.session {
            s.idle_lock_secs > 0 && s.last_activity.elapsed().as_secs() >= s.idle_lock_secs
        } else {
            false
        };
        if should {
            self.lock();
            true
        } else {
            false
        }
    }

    pub fn set_idle_lock_secs(&mut self, secs: u64) -> AppResult<()> {
        self.require_unlocked()?;
        if let Some(f) = &mut self.file {
            f.header.idle_lock_secs = secs;
        }
        if let Some(s) = &mut self.session {
            s.idle_lock_secs = secs;
            s.last_activity = Instant::now();
        }
        self.persist()?;
        Ok(())
    }

    pub fn setup(&mut self, password: &str) -> AppResult<String> {
        if self.file.is_some() {
            return Err(AppError::AlreadyInitialized);
        }
        if password.chars().count() < 12 {
            return Err(AppError::Message(
                "password must be at least 12 characters".into(),
            ));
        }

        let salt = crypto::random_array::<SALT_LEN>();
        let content_key = MasterKey::from_bytes(crypto::random_array::<{ crypto::KEY_LEN }>());
        let pw_key = derive_key(password, &salt, ARGON2_M_KIB, ARGON2_T, ARGON2_P)?;
        let verifier = crypto::encrypt(&pw_key, b"secretfolder-ok", b"secretfolder-verifier")?;
        let password_wrapped =
            crypto::encrypt(&pw_key, content_key.as_bytes(), b"secretfolder-pw-wrap")?;

        let recovery = generate_recovery_key();
        let recovery_norm = normalize_recovery_key(&recovery);
        let recovery_key = derive_key(&recovery_norm, &salt, ARGON2_M_KIB, ARGON2_T, ARGON2_P)?;
        let recovery_verifier = crypto::encrypt(
            &recovery_key,
            b"secretfolder-recovery-ok",
            b"secretfolder-recovery",
        )?;
        let wrapped_master =
            crypto::encrypt(&recovery_key, content_key.as_bytes(), b"secretfolder-wrap")?;

        let header = VaultHeader {
            version: VAULT_VERSION,
            salt_b64: B64.encode(salt),
            verifier_b64: B64.encode(verifier),
            recovery_verifier_b64: Some(B64.encode(recovery_verifier)),
            wrapped_master_b64: Some(B64.encode(wrapped_master)),
            password_wrapped_key_b64: Some(B64.encode(password_wrapped)),
            argon2_m_kib: ARGON2_M_KIB,
            argon2_t: ARGON2_T,
            argon2_p: ARGON2_P,
            idle_lock_secs: DEFAULT_IDLE_LOCK_SECS,
        };

        self.file = Some(VaultFile {
            header,
            items: vec![],
        });
        self.session = Some(Session {
            key: content_key,
            items: HashMap::new(),
            idle_lock_secs: DEFAULT_IDLE_LOCK_SECS,
            last_activity: Instant::now(),
            has_recovery_key: true,
        });
        self.persist()?;
        Ok(recovery)
    }

    pub fn unlock(&mut self, password: &str) -> AppResult<()> {
        if self.session.is_some() {
            return Err(AppError::AlreadyUnlocked);
        }
        let file = self.file.as_ref().ok_or(AppError::NotInitialized)?;
        let salt = B64
            .decode(&file.header.salt_b64)
            .map_err(|e| AppError::Crypto(format!("salt: {e}")))?;
        let pw_key = derive_key(
            password,
            &salt,
            file.header.argon2_m_kib,
            file.header.argon2_t,
            file.header.argon2_p,
        )?;
        let verifier = B64
            .decode(&file.header.verifier_b64)
            .map_err(|e| AppError::Crypto(format!("verifier: {e}")))?;
        let _ = crypto::decrypt(&pw_key, &verifier, b"secretfolder-verifier")
            .map_err(|_| AppError::BadPassword)?;

        let wrap_b64 = file
            .header
            .password_wrapped_key_b64
            .as_ref()
            .ok_or_else(|| AppError::Crypto("missing password wrap".into()))?;
        let wrapped = B64
            .decode(wrap_b64)
            .map_err(|e| AppError::Crypto(format!("pw wrap: {e}")))?;
        let bytes = crypto::decrypt(&pw_key, &wrapped, b"secretfolder-pw-wrap")
            .map_err(|_| AppError::BadPassword)?;
        let content_key = MasterKey::from_slice(&bytes)?;
        self.load_session(content_key)?;
        Ok(())
    }

    pub fn unlock_with_recovery(&mut self, recovery_key: &str) -> AppResult<()> {
        if self.session.is_some() {
            return Err(AppError::AlreadyUnlocked);
        }
        let file = self.file.clone().ok_or(AppError::NotInitialized)?;
        let wrapped_b64 = file
            .header
            .wrapped_master_b64
            .as_ref()
            .ok_or_else(|| AppError::Message("no recovery key configured".into()))?;

        let salt = B64
            .decode(&file.header.salt_b64)
            .map_err(|e| AppError::Crypto(format!("salt: {e}")))?;
        let norm = normalize_recovery_key(recovery_key);
        if norm.len() != 64 {
            return Err(AppError::BadPassword);
        }
        let rkey = derive_key(
            &norm,
            &salt,
            file.header.argon2_m_kib,
            file.header.argon2_t,
            file.header.argon2_p,
        )?;
        if let Some(ver_b64) = &file.header.recovery_verifier_b64 {
            let verifier = B64
                .decode(ver_b64)
                .map_err(|e| AppError::Crypto(format!("recovery verifier: {e}")))?;
            let _ = crypto::decrypt(&rkey, &verifier, b"secretfolder-recovery")
                .map_err(|_| AppError::BadPassword)?;
        }
        let wrapped = B64
            .decode(wrapped_b64)
            .map_err(|e| AppError::Crypto(format!("wrapped master: {e}")))?;
        let master_bytes = crypto::decrypt(&rkey, &wrapped, b"secretfolder-wrap")
            .map_err(|_| AppError::BadPassword)?;
        if master_bytes.len() != crypto::KEY_LEN {
            return Err(AppError::Crypto("bad wrapped master length".into()));
        }
        self.load_session(MasterKey::from_slice(&master_bytes)?)?;
        Ok(())
    }

    fn load_session(&mut self, key: MasterKey) -> AppResult<()> {
        let file = self.file.as_ref().ok_or(AppError::NotInitialized)?;
        let mut items = HashMap::new();
        for enc in &file.items {
            let blob = B64
                .decode(&enc.ciphertext_b64)
                .map_err(|e| AppError::Crypto(format!("item b64: {e}")))?;
            let aad = index_aad(&enc.id);
            let pt = crypto::decrypt(&key, &blob, &aad)?;
            let plain: ItemPlain = serde_json::from_slice(&pt)
                .map_err(|e| AppError::Crypto(format!("item json: {e}")))?;
            items.insert(plain.id.clone(), plain);
        }
        let idle = file.header.idle_lock_secs;
        let has_recovery_key = file.header.wrapped_master_b64.is_some();
        self.session = Some(Session {
            key,
            items,
            idle_lock_secs: idle,
            last_activity: Instant::now(),
            has_recovery_key,
        });
        Ok(())
    }

    pub fn lock(&mut self) {
        // Drop session explicitly: clear plaintext item map first, then MasterKey
        // zeroizes on drop via ZeroizeOnDrop.
        if let Some(mut session) = self.session.take() {
            session.items.clear();
            drop(session);
        }
    }

    pub fn change_password(&mut self, current: &str, new_password: &str) -> AppResult<()> {
        self.require_unlocked()?;
        if new_password.chars().count() < 12 {
            return Err(AppError::Message(
                "password must be at least 12 characters".into(),
            ));
        }
        // Verify current password against verifier without dropping session.
        {
            let file = self.file.as_ref().ok_or(AppError::NotInitialized)?;
            let salt = B64
                .decode(&file.header.salt_b64)
                .map_err(|e| AppError::Crypto(format!("salt: {e}")))?;
            let pw_key = derive_key(
                current,
                &salt,
                file.header.argon2_m_kib,
                file.header.argon2_t,
                file.header.argon2_p,
            )?;
            let verifier = B64
                .decode(&file.header.verifier_b64)
                .map_err(|e| AppError::Crypto(format!("verifier: {e}")))?;
            let _ = crypto::decrypt(&pw_key, &verifier, b"secretfolder-verifier")
                .map_err(|_| AppError::BadPassword)?;
        }

        let mut content_key_bytes = {
            let s = self.session.as_ref().ok_or(AppError::Locked)?;
            *s.key.as_bytes()
        };

        let result = (|| {
            let file = self.file.as_mut().ok_or(AppError::NotInitialized)?;
            let salt = B64
                .decode(&file.header.salt_b64)
                .map_err(|e| AppError::Crypto(format!("salt: {e}")))?;
            let new_pw_key = derive_key(
                new_password,
                &salt,
                file.header.argon2_m_kib,
                file.header.argon2_t,
                file.header.argon2_p,
            )?;
            let verifier =
                crypto::encrypt(&new_pw_key, b"secretfolder-ok", b"secretfolder-verifier")?;
            let password_wrapped =
                crypto::encrypt(&new_pw_key, &content_key_bytes, b"secretfolder-pw-wrap")?;
            file.header.verifier_b64 = B64.encode(verifier);
            file.header.password_wrapped_key_b64 = Some(B64.encode(password_wrapped));
            // Recovery wrap stays the same (content key unchanged).
            self.persist()?;
            self.touch();
            Ok(())
        })();
        content_key_bytes.zeroize();
        result
    }

    #[allow(dead_code)]
    pub fn list_items(&mut self) -> AppResult<Vec<ItemPreview>> {
        self.list_items_in(None)
    }

    /// List items directly under `parent_id` (None = vault root).
    pub fn list_items_in(&mut self, parent_id: Option<String>) -> AppResult<Vec<ItemPreview>> {
        self.require_unlocked()?;
        self.touch();
        let s = self.session.as_ref().unwrap();
        let parent = parent_id.filter(|p| !p.is_empty());
        if let Some(ref pid) = parent {
            let folder = s.items.get(pid).ok_or(AppError::NotFound)?;
            if folder.kind != ItemKind::Folder {
                return Err(AppError::Message("parent is not a folder".into()));
            }
        }
        let mut list: Vec<ItemPreview> = s
            .items
            .values()
            .filter(|i| i.parent_id == parent)
            .map(item_to_preview)
            .collect();
        list.sort_by(|a, b| {
            let af = matches!(a.kind, ItemKind::Folder);
            let bf = matches!(b.kind, ItemKind::Folder);
            bf.cmp(&af)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(list)
    }

    /// Breadcrumb from root → folder (empty if root).
    pub fn folder_path(&mut self, folder_id: Option<&str>) -> AppResult<Vec<ItemPreview>> {
        self.require_unlocked()?;
        self.touch();
        let Some(start) = folder_id.filter(|s| !s.is_empty()) else {
            return Ok(vec![]);
        };
        let s = self.session.as_ref().unwrap();
        let mut chain = Vec::new();
        let mut cur = Some(start.to_string());
        let mut guard = 0usize;
        while let Some(id) = cur {
            guard += 1;
            if guard > 64 {
                return Err(AppError::Message("folder path too deep".into()));
            }
            let item = s.items.get(&id).ok_or(AppError::NotFound)?;
            if item.kind != ItemKind::Folder {
                return Err(AppError::Message("path entry is not a folder".into()));
            }
            chain.push(item_to_preview(item));
            cur = item.parent_id.clone();
        }
        chain.reverse();
        Ok(chain)
    }

    pub fn get_item(&mut self, id: &str) -> AppResult<ItemDetail> {
        self.require_unlocked()?;
        self.touch();
        let s = self.session.as_ref().ok_or(AppError::Locked)?;
        let meta = s.items.get(id).ok_or(AppError::NotFound)?.clone();
        if meta.kind == ItemKind::Folder {
            return Ok(ItemDetail {
                id: meta.id.clone(),
                name: meta.name.clone(),
                mime: meta.mime.clone(),
                size: meta.size,
                kind: meta.kind,
                parent_id: meta.parent_id.clone(),
                created_at: meta.created_at.clone(),
                updated_at: meta.updated_at.clone(),
                text: None,
                data_url: None,
            });
        }
        let key = &s.key;
        let raw = self.read_blob(id, key)?;

        let (text, data_url) = match meta.kind {
            ItemKind::Text => {
                let t = String::from_utf8_lossy(&raw).into_owned();
                (Some(t), None)
            }
            ItemKind::Image => {
                if meta.size > MAX_PREVIEW_SIZE {
                    // Too large for inline rendering — user must export
                    (None, None)
                } else {
                    let b64 = B64.encode(&raw);
                    let url = format!("data:{};base64,{}", meta.mime, b64);
                    (None, Some(url))
                }
            }
            ItemKind::Binary | ItemKind::Folder => (None, None),
        };

        Ok(ItemDetail {
            id: meta.id.clone(),
            name: meta.name.clone(),
            mime: meta.mime.clone(),
            size: meta.size,
            kind: meta.kind,
            parent_id: meta.parent_id.clone(),
            created_at: meta.created_at.clone(),
            updated_at: meta.updated_at.clone(),
            text,
            data_url,
        })
    }

    /// Create a folder (group) under optional parent.
    pub fn create_folder(
        &mut self,
        name: &str,
        parent_id: Option<String>,
    ) -> AppResult<ItemPreview> {
        self.require_unlocked()?;
        self.touch();
        let name = sanitize_name(name)?;
        let parent = self.validate_parent(parent_id)?;
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let plain = ItemPlain {
            id: id.clone(),
            name,
            mime: "inode/directory".into(),
            size: 0,
            kind: ItemKind::Folder,
            parent_id: parent,
            created_at: now.clone(),
            updated_at: now,
        };
        {
            let s = self.session.as_mut().ok_or(AppError::Locked)?;
            s.items.insert(id, plain.clone());
        }
        self.persist()?;
        Ok(item_to_preview(&plain))
    }

    /// Move an item into a folder (or root when parent_id is None).
    pub fn move_item(&mut self, id: &str, parent_id: Option<String>) -> AppResult<ItemPreview> {
        self.require_unlocked()?;
        self.touch();
        let parent = self.validate_parent(parent_id)?;
        if let Some(ref p) = parent {
            if p == id {
                return Err(AppError::Message("cannot move a folder into itself".into()));
            }
            // Prevent cycles: parent must not be a descendant of id.
            if self.is_descendant(p, id)? {
                return Err(AppError::Message(
                    "cannot move a folder into its own subfolder".into(),
                ));
            }
        }
        let preview = {
            let s = self.session.as_mut().ok_or(AppError::Locked)?;
            let item = s.items.get_mut(id).ok_or(AppError::NotFound)?;
            item.parent_id = parent;
            item.updated_at = Utc::now().to_rfc3339();
            item_to_preview(item)
        };
        self.persist()?;
        Ok(preview)
    }

    /// Create a new text item (empty or with initial body).
    pub fn create_text(
        &mut self,
        name: &str,
        body: &str,
        parent_id: Option<String>,
    ) -> AppResult<ItemPreview> {
        let name = sanitize_name(name)?;
        let data = body.as_bytes().to_vec();
        self.import_bytes(name, "text/plain".into(), data, parent_id)
    }

    /// Import raw bytes under a display name.
    pub fn import_bytes(
        &mut self,
        name: String,
        mime: String,
        data: Vec<u8>,
        parent_id: Option<String>,
    ) -> AppResult<ItemPreview> {
        self.require_unlocked()?;
        self.touch();
        if data.len() as u64 > MAX_FILE_BYTES {
            return Err(AppError::TooLarge(MAX_FILE_BYTES));
        }
        let name = sanitize_name(&name)?;
        let kind = classify_kind(&mime, &name);
        if kind == ItemKind::Folder {
            return Err(AppError::Message("use create_folder for folders".into()));
        }
        let parent = self.validate_parent(parent_id)?;
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let size = data.len() as u64;

        {
            let s = self.session.as_ref().ok_or(AppError::Locked)?;
            self.write_blob(&id, &s.key, &data)?;
        }

        let plain = ItemPlain {
            id: id.clone(),
            name: name.clone(),
            mime: mime.clone(),
            size,
            kind,
            parent_id: parent,
            created_at: now.clone(),
            updated_at: now,
        };

        {
            let s = self.session.as_mut().ok_or(AppError::Locked)?;
            s.items.insert(id.clone(), plain.clone());
        }
        self.persist()?;

        Ok(item_to_preview(&plain))
    }

    pub fn update_text(
        &mut self,
        id: &str,
        name: Option<String>,
        body: String,
    ) -> AppResult<ItemPreview> {
        self.require_unlocked()?;
        self.touch();
        if body.len() as u64 > MAX_FILE_BYTES {
            return Err(AppError::TooLarge(MAX_FILE_BYTES));
        }

        let data = body.into_bytes();
        {
            let s = self.session.as_ref().ok_or(AppError::Locked)?;
            if !s.items.contains_key(id) {
                return Err(AppError::NotFound);
            }
            self.write_blob(id, &s.key, &data)?;
        }

        let preview = {
            let s = self.session.as_mut().ok_or(AppError::Locked)?;
            let item = s.items.get_mut(id).ok_or(AppError::NotFound)?;
            if item.kind != ItemKind::Text {
                return Err(AppError::Message("item is not a text file".into()));
            }
            if let Some(n) = name {
                item.name = sanitize_name(&n)?;
            }
            item.size = data.len() as u64;
            item.mime = "text/plain".into();
            item.updated_at = Utc::now().to_rfc3339();
            item_to_preview(item)
        };
        self.persist()?;
        Ok(preview)
    }

    pub fn rename_item(&mut self, id: &str, name: &str) -> AppResult<ItemPreview> {
        self.require_unlocked()?;
        self.touch();
        let name = sanitize_name(name)?;
        let preview = {
            let s = self.session.as_mut().ok_or(AppError::Locked)?;
            let item = s.items.get_mut(id).ok_or(AppError::NotFound)?;
            item.name = name;
            item.updated_at = Utc::now().to_rfc3339();
            item_to_preview(item)
        };
        self.persist()?;
        Ok(preview)
    }

    /// Number of items nested under a folder (0 for files / empty folders).
    pub fn folder_content_count(&mut self, id: &str) -> AppResult<usize> {
        self.require_unlocked()?;
        self.touch();
        let s = self.session.as_ref().ok_or(AppError::Locked)?;
        let item = s.items.get(id).ok_or(AppError::NotFound)?;
        if item.kind != ItemKind::Folder {
            return Ok(0);
        }
        Ok(collect_subtree_ids(&s.items, id).len().saturating_sub(1))
    }

    /// Delete an item. Folders must be empty unless `cascade` is true
    /// (then the folder and all nested contents are removed).
    pub fn delete_item(&mut self, id: &str, cascade: bool) -> AppResult<()> {
        self.require_unlocked()?;
        self.touch();
        // Collect id + all descendants (folders recurse).
        let to_delete = {
            let s = self.session.as_ref().ok_or(AppError::Locked)?;
            let item = s.items.get(id).ok_or(AppError::NotFound)?;
            let subtree = collect_subtree_ids(&s.items, id);
            if item.kind == ItemKind::Folder {
                let nested = subtree.len().saturating_sub(1);
                if nested > 0 && !cascade {
                    return Err(AppError::Message(format!(
                        "Folder is not empty ({nested} item{}). Empty it first, or confirm deleting everything inside.",
                        if nested == 1 { "" } else { "s" }
                    )));
                }
            }
            subtree
        };
        {
            let s = self.session.as_mut().ok_or(AppError::Locked)?;
            for del_id in &to_delete {
                s.items.remove(del_id);
            }
        }
        for del_id in &to_delete {
            let blob_path = self.blob_path(del_id);
            let _ = fs::remove_file(blob_path);
        }
        self.persist()?;
        Ok(())
    }

    /// Export decrypted bytes for an item (caller writes to disk via dialog path).
    pub fn export_bytes(&mut self, id: &str) -> AppResult<(String, Vec<u8>)> {
        self.require_unlocked()?;
        self.touch();
        let s = self.session.as_ref().ok_or(AppError::Locked)?;
        let meta = s.items.get(id).ok_or(AppError::NotFound)?.clone();
        if meta.kind == ItemKind::Folder {
            return Err(AppError::Message("cannot export a folder as bytes".into()));
        }
        let raw = self.read_blob(id, &s.key)?;
        Ok((meta.name.clone(), raw))
    }

    fn validate_parent(&self, parent_id: Option<String>) -> AppResult<Option<String>> {
        let parent = parent_id.filter(|p| !p.is_empty());
        if let Some(ref pid) = parent {
            let s = self.session.as_ref().ok_or(AppError::Locked)?;
            let folder = s.items.get(pid).ok_or(AppError::NotFound)?;
            if folder.kind != ItemKind::Folder {
                return Err(AppError::Message("parent is not a folder".into()));
            }
        }
        Ok(parent)
    }

    /// True if `maybe_desc` is id or nested under `ancestor_id`.
    fn is_descendant(&self, maybe_desc: &str, ancestor_id: &str) -> AppResult<bool> {
        let s = self.session.as_ref().ok_or(AppError::Locked)?;
        let mut cur = Some(maybe_desc.to_string());
        let mut guard = 0usize;
        while let Some(id) = cur {
            if id == ancestor_id {
                return Ok(true);
            }
            guard += 1;
            if guard > 64 {
                return Err(AppError::Message("folder hierarchy too deep".into()));
            }
            cur = s.items.get(&id).and_then(|i| i.parent_id.clone());
        }
        Ok(false)
    }

    fn require_unlocked(&self) -> AppResult<()> {
        if self.session.is_none() {
            Err(AppError::Locked)
        } else {
            Ok(())
        }
    }

    fn blob_path(&self, id: &str) -> PathBuf {
        self.blobs_dir.join(format!("{id}.bin"))
    }

    fn write_blob(&self, id: &str, key: &MasterKey, data: &[u8]) -> AppResult<()> {
        let aad = blob_aad(id);
        let ct = crypto::encrypt(key, data, &aad)?;
        let path = self.blob_path(id);
        atomic_write(&path, &ct)?;
        Ok(())
    }

    fn read_blob(&self, id: &str, key: &MasterKey) -> AppResult<Vec<u8>> {
        let path = self.blob_path(id);
        let mut f = File::open(&path).map_err(|_| AppError::NotFound)?;
        let mut blob = Vec::new();
        f.read_to_end(&mut blob)?;
        let aad = blob_aad(id);
        crypto::decrypt(key, &blob, &aad)
    }

    fn persist(&mut self) -> AppResult<()> {
        let session = self.session.as_ref().ok_or(AppError::Locked)?;
        let file = self.file.as_mut().ok_or(AppError::NotInitialized)?;

        let mut items = Vec::with_capacity(session.items.len());
        for (id, plain) in &session.items {
            let json = serde_json::to_vec(plain)
                .map_err(|e| AppError::Crypto(format!("serialize item: {e}")))?;
            let aad = index_aad(id);
            let ct = crypto::encrypt(&session.key, &json, &aad)?;
            items.push(EncIndexEntry {
                id: id.clone(),
                ciphertext_b64: B64.encode(ct),
            });
        }
        items.sort_by(|a, b| a.id.cmp(&b.id));
        file.items = items;

        let json = serde_json::to_vec_pretty(file)
            .map_err(|e| AppError::Io(format!("serialize vault: {e}")))?;
        // Index is the source of truth for unlock — use bak + atomic replace.
        atomic_write_with_backup(&self.index_path, &json)?;
        Ok(())
    }
}

fn index_aad(id: &str) -> Vec<u8> {
    format!("secretfolder-index:{id}").into_bytes()
}

fn blob_aad(id: &str) -> Vec<u8> {
    format!("secretfolder-blob:{id}").into_bytes()
}

fn item_to_preview(item: &ItemPlain) -> ItemPreview {
    ItemPreview {
        id: item.id.clone(),
        name: item.name.clone(),
        mime: item.mime.clone(),
        size: item.size,
        kind: item.kind,
        parent_id: item.parent_id.clone(),
        created_at: item.created_at.clone(),
        updated_at: item.updated_at.clone(),
    }
}

/// `root_id` plus all nested children (folders recurse).
fn collect_subtree_ids(items: &HashMap<String, ItemPlain>, root_id: &str) -> Vec<String> {
    let mut out = vec![root_id.to_string()];
    let mut stack = vec![root_id.to_string()];
    while let Some(cur) = stack.pop() {
        for (id, item) in items {
            if item.parent_id.as_deref() == Some(cur.as_str()) {
                out.push(id.clone());
                if item.kind == ItemKind::Folder {
                    stack.push(id.clone());
                }
            }
        }
    }
    out
}

pub fn default_vault_root() -> AppResult<PathBuf> {
    let base = dirs::data_dir().ok_or_else(|| AppError::Io("no data dir".into()))?;
    // Separate from SecretSticky — never share vault/keys.
    Ok(base.join("com.ahmi.secretfolder"))
}

fn sanitize_name(name: &str) -> AppResult<String> {
    let t = name.trim();
    if t.is_empty() || t.len() > 255 {
        return Err(AppError::InvalidName);
    }
    if t.contains('/') || t.contains('\\') || t.contains('\0') {
        return Err(AppError::InvalidName);
    }
    if t == "." || t == ".." {
        return Err(AppError::InvalidName);
    }
    Ok(t.to_string())
}

fn classify_kind(mime: &str, name: &str) -> ItemKind {
    let m = mime.to_ascii_lowercase();
    if m.starts_with("text/") || m == "application/json" || m == "application/xml" {
        return ItemKind::Text;
    }
    if m.starts_with("image/") {
        return ItemKind::Image;
    }
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".txt")
        || lower.ends_with(".md")
        || lower.ends_with(".json")
        || lower.ends_with(".csv")
        || lower.ends_with(".log")
        || lower.ends_with(".xml")
        || lower.ends_with(".yml")
        || lower.ends_with(".yaml")
        || lower.ends_with(".toml")
        || lower.ends_with(".rs")
        || lower.ends_with(".ts")
        || lower.ends_with(".tsx")
        || lower.ends_with(".js")
        || lower.ends_with(".py")
        || lower.ends_with(".env")
    {
        return ItemKind::Text;
    }
    if lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".gif")
        || lower.ends_with(".webp")
        || lower.ends_with(".bmp")
        || lower.ends_with(".svg")
    {
        return ItemKind::Image;
    }
    ItemKind::Binary
}

/// Guess mime from filename.
pub fn guess_mime(name: &str) -> String {
    mime_guess::from_path(name)
        .first_or_octet_stream()
        .essence_str()
        .to_string()
}

fn backup_path_for(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

fn load_vault_file(path: &Path) -> AppResult<VaultFile> {
    let raw = fs::read_to_string(path)?;
    let file: VaultFile =
        serde_json::from_str(&raw).map_err(|e| AppError::Io(format!("vault.json parse: {e}")))?;
    // v1 is the baseline. Additive fields use serde defaults.
    // version 0 is invalid; > current means this build must not open/write-back
    // (could strand or rewrite secrets the newer format expects).
    if file.header.version == 0 {
        return Err(AppError::Message(format!(
            "vault format version {} is invalid",
            file.header.version
        )));
    }
    if file.header.version > VAULT_VERSION {
        return Err(AppError::Message(format!(
            "vault format version {} is newer than this app supports ({}) — upgrade SecretFolder; refusing to open so saved files are not rewritten",
            file.header.version, VAULT_VERSION
        )));
    }
    Ok(file)
}

/// Copy backup into the live vault path without deleting backup first.
fn restore_backup(bak: &Path, live: &Path) -> AppResult<()> {
    if let Some(parent) = live.parent() {
        fs::create_dir_all(parent)?;
    }
    // Prefer copy so the .bak remains if the subsequent live write fails.
    fs::copy(bak, live)?;
    Ok(())
}

/// Atomic write for blobs: temp + replace (no .bak — blob ids are immutable content).
fn atomic_write(path: &Path, data: &[u8]) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Io("no parent".into()))?;
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(
        ".{}.tmp",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("blob")
    ));
    {
        let mut f = File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    replace_file(&tmp, path)?;
    Ok(())
}

/// Write vault index via temp + last-known-good bak + OS replace-in-place.
///
/// Never truncates the live `vault.json` in place: a failed write must not
/// destroy the previous good file (update / crash safety for saved secrets).
fn atomic_write_with_backup(path: &Path, data: &[u8]) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Io("no parent".into()))?;
    fs::create_dir_all(parent)?;
    let tmp = path.with_extension("json.tmp");
    // Write + flush temp fully before swapping into place so a power loss
    // cannot leave a half-written live vault as the only copy.
    {
        let mut f = File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    // Keep a last-known-good sibling before replacing the live file.
    // replace_file must never delete the live vault before the new bytes
    // are durable at the destination path (see SECURITY.md invariant).
    if path.exists() {
        let bak = backup_path_for(path);
        // Best-effort snapshot of the previous good file. Failure here must
        // not block the save — the live file is still intact until replace.
        let _ = fs::copy(path, &bak);
    }
    replace_file(&tmp, path)?;
    Ok(())
}

/// Replace `from` → `to` without a delete-then-rename hole.
///
/// **Critical for vault safety:** older Windows code did `remove_file(to)` then
/// `rename(from, to)`. If rename failed (or the process died between the two),
/// the only copy of `vault.json` was gone. That must never happen.
fn replace_file(from: &Path, to: &Path) -> AppResult<()> {
    #[cfg(windows)]
    {
        windows_replace_file(from, to)
    }
    #[cfg(not(windows))]
    {
        // POSIX rename replaces atomically on the same filesystem.
        fs::rename(from, to)?;
        Ok(())
    }
}

#[cfg(windows)]
fn windows_replace_file(from: &Path, to: &Path) -> AppResult<()> {
    use std::os::windows::ffi::OsStrExt;

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    // MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH
    // Replaces the destination in one kernel call — never deletes `to` first.
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(
            lp_existing_file_name: *const u16,
            lp_new_file_name: *const u16,
            dw_flags: u32,
        ) -> i32;
    }

    let from_w = wide(from);
    let to_w = wide(to);
    let ok = unsafe {
        MoveFileExW(
            from_w.as_ptr(),
            to_w.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        let err = std::io::Error::last_os_error();
        return Err(AppError::Message(format!(
            "atomic vault replace failed (saved files left untouched): {err}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::tempdir;

    fn test_vault() -> (tempfile::TempDir, Vault) {
        let dir = tempdir().unwrap();
        let v = Vault::open_path_for_test(dir.path().to_path_buf()).unwrap();
        (dir, v)
    }

    #[test]
    fn setup_unlock_crud_text() {
        let (_dir, mut v) = test_vault();
        assert!(!v.status().initialized);
        let recovery = v.setup("password1234").unwrap();
        assert!(!recovery.is_empty());
        assert!(v.status().unlocked);

        let item = v.create_text("notes.txt", "hello vault", None).unwrap();
        assert_eq!(item.name, "notes.txt");
        assert_eq!(item.kind, ItemKind::Text);
        assert!(item.parent_id.is_none());

        v.lock();
        assert!(!v.status().unlocked);
        assert!(v.unlock("wrong-password").is_err());
        v.unlock("password1234").unwrap();

        let got = v.get_item(&item.id).unwrap();
        assert_eq!(got.text.as_deref(), Some("hello vault"));

        v.update_text(&item.id, Some("renamed.txt".into()), "updated".into())
            .unwrap();
        let got = v.get_item(&item.id).unwrap();
        assert_eq!(got.name, "renamed.txt");
        assert_eq!(got.text.as_deref(), Some("updated"));

        v.delete_item(&item.id, false).unwrap();
        assert!(v.get_item(&item.id).is_err());
    }

    #[test]
    fn locked_ops_fail() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        v.lock();
        assert!(v.list_items().is_err());
        assert!(v.create_text("a.txt", "", None).is_err());
    }

    #[test]
    fn recovery_unlock_works() {
        let (_dir, mut v) = test_vault();
        let recovery = v.setup("password1234").unwrap();
        let item = v.create_text("s.txt", "secret-body", None).unwrap();
        v.lock();
        v.unlock_with_recovery(&recovery).unwrap();
        let got = v.get_item(&item.id).unwrap();
        assert_eq!(got.text.as_deref(), Some("secret-body"));
    }

    #[test]
    fn change_password_keeps_items_and_recovery() {
        let (_dir, mut v) = test_vault();
        let recovery = v.setup("password1234").unwrap();
        let item = v.create_text("keep.txt", "sk-keep-me", None).unwrap();

        v.change_password("password1234", "new-password-456")
            .unwrap();
        assert!(v.status().unlocked);
        assert!(v.status().has_recovery_key);

        v.lock();
        assert!(v.unlock("password1234").is_err());
        v.unlock("new-password-456").unwrap();
        let got = v.get_item(&item.id).unwrap();
        assert_eq!(got.text.as_deref(), Some("sk-keep-me"));

        v.lock();
        v.unlock_with_recovery(&recovery).unwrap();
        let got = v.get_item(&item.id).unwrap();
        assert_eq!(got.name, "keep.txt");
    }

    #[test]
    fn binary_import_export_roundtrip() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        let data = vec![0u8, 1, 2, 3, 255, 128];
        let item = v
            .import_bytes(
                "blob.bin".into(),
                "application/octet-stream".into(),
                data.clone(),
                None,
            )
            .unwrap();
        assert_eq!(item.kind, ItemKind::Binary);
        let (name, out) = v.export_bytes(&item.id).unwrap();
        assert_eq!(name, "blob.bin");
        assert_eq!(out, data);
    }

    #[test]
    fn image_classified() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        // minimal 1x1 png-ish bytes
        let data = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
        let item = v
            .import_bytes("pic.png".into(), "image/png".into(), data, None)
            .unwrap();
        assert_eq!(item.kind, ItemKind::Image);
        let got = v.get_item(&item.id).unwrap();
        assert!(got
            .data_url
            .as_ref()
            .unwrap()
            .starts_with("data:image/png;base64,"));
    }

    #[test]
    fn filenames_not_plaintext_in_index() {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();
        {
            let mut v = Vault::open_path_for_test(root.clone()).unwrap();
            v.setup("password1234").unwrap();
            v.create_text("super-secret-name.txt", "x", None).unwrap();
        }
        let index = fs::read_to_string(root.join("vault.json")).unwrap();
        assert!(!index.contains("super-secret-name.txt"));
    }

    #[test]
    fn persist_survives_reload() {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let id = {
            let mut v = Vault::open_path_for_test(root.clone()).unwrap();
            v.setup("password1234").unwrap();
            let item = v.create_text("one.txt", "body-one", None).unwrap();
            item.id
        };
        let mut v2 = Vault::open_path_for_test(root).unwrap();
        v2.unlock("password1234").unwrap();
        let got = v2.get_item(&id).unwrap();
        assert_eq!(got.text.as_deref(), Some("body-one"));
    }

    #[test]
    fn rejects_huge_file() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        let big = vec![0u8; (MAX_FILE_BYTES as usize) + 1];
        let err = v
            .import_bytes(
                "big.bin".into(),
                "application/octet-stream".into(),
                big,
                None,
            )
            .unwrap_err();
        assert!(matches!(err, AppError::TooLarge(_)));
    }

    #[test]
    fn folders_nest_list_and_delete() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        let folder = v.create_folder("Work", None).unwrap();
        assert_eq!(folder.kind, ItemKind::Folder);
        let nested = v.create_folder("Secrets", Some(folder.id.clone())).unwrap();
        let note = v
            .create_text("a.txt", "in-folder", Some(folder.id.clone()))
            .unwrap();
        assert_eq!(note.parent_id.as_deref(), Some(folder.id.as_str()));

        let root = v.list_items_in(None).unwrap();
        assert_eq!(root.len(), 1);
        assert_eq!(root[0].id, folder.id);

        let inside = v.list_items_in(Some(folder.id.clone())).unwrap();
        assert_eq!(inside.len(), 2); // nested folder + note

        let path = v.folder_path(Some(&nested.id)).unwrap();
        assert_eq!(path.len(), 2);
        assert_eq!(path[0].name, "Work");
        assert_eq!(path[1].name, "Secrets");

        v.move_item(&note.id, Some(nested.id.clone())).unwrap();
        assert!(v
            .list_items_in(Some(folder.id.clone()))
            .unwrap()
            .iter()
            .all(|i| i.id != note.id));
        assert_eq!(
            v.list_items_in(Some(nested.id.clone()))
                .unwrap()
                .iter()
                .filter(|i| i.id == note.id)
                .count(),
            1
        );

        // Non-empty folder refuses delete without cascade.
        assert_eq!(v.folder_content_count(&folder.id).unwrap(), 2);
        assert!(v.delete_item(&folder.id, false).is_err());
        assert!(v.get_item(&note.id).is_ok());

        // Empty nested folder deletes cleanly.
        // Move note to root first, delete empty nested, then cascade parent.
        v.move_item(&note.id, None).unwrap();
        // nested still under folder and empty
        assert_eq!(v.folder_content_count(&nested.id).unwrap(), 0);
        v.delete_item(&nested.id, false).unwrap();
        assert!(v.get_item(&nested.id).is_err());

        // Put note back and cascade-delete parent.
        let nested2 = v.create_folder("Secrets", Some(folder.id.clone())).unwrap();
        v.move_item(&note.id, Some(nested2.id.clone())).unwrap();
        v.delete_item(&folder.id, true).unwrap();
        assert!(v.get_item(&note.id).is_err());
        assert!(v.get_item(&nested2.id).is_err());
        assert!(v.get_item(&folder.id).is_err());
    }

    #[test]
    fn sanitize_blocks_paths() {
        assert!(sanitize_name("../x").is_err());
        assert!(sanitize_name("a/b").is_err());
        assert!(sanitize_name("").is_err());
        assert_eq!(sanitize_name(" ok.txt ").unwrap(), "ok.txt");
    }

    #[test]
    fn large_image_skips_inline_data_url() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        // Just over the preview gate, well under MAX_FILE_BYTES.
        let size = (MAX_PREVIEW_SIZE as usize) + 1;
        let mut data = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
        data.resize(size, 0xAB);
        let item = v
            .import_bytes("huge.png".into(), "image/png".into(), data, None)
            .unwrap();
        assert_eq!(item.kind, ItemKind::Image);
        let got = v.get_item(&item.id).unwrap();
        assert!(
            got.data_url.is_none(),
            "images > MAX_PREVIEW_SIZE must not embed base64 data URLs"
        );
        // Export still works for oversized previews.
        let (name, bytes) = v.export_bytes(&item.id).unwrap();
        assert_eq!(name, "huge.png");
        assert_eq!(bytes.len(), size);
    }

    #[test]
    fn small_image_keeps_inline_data_url() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        let data = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
        let item = v
            .import_bytes("tiny.png".into(), "image/png".into(), data, None)
            .unwrap();
        let got = v.get_item(&item.id).unwrap();
        assert!(got
            .data_url
            .as_ref()
            .unwrap()
            .starts_with("data:image/png;base64,"));
    }

    #[test]
    fn idle_lock_zero_means_disabled() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        v.set_idle_lock_secs(0).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(!v.check_idle_lock());
        assert!(v.status().unlocked);
    }

    #[test]
    fn idle_lock_triggers_after_timeout() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        v.set_idle_lock_secs(1).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert!(v.check_idle_lock());
        assert!(!v.status().unlocked);
    }

    #[test]
    fn short_password_rejected_on_setup() {
        let (_dir, mut v) = test_vault();
        let err = v.setup("short").unwrap_err();
        assert!(format!("{err}").contains("at least 12"));
    }

    #[test]
    fn wrong_recovery_key_fails() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        v.lock();
        assert!(v
            .unlock_with_recovery(
                "0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000-0000"
            )
            .is_err());
    }

    #[test]
    fn rename_and_move_to_root() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        let folder = v.create_folder("Box", None).unwrap();
        let note = v
            .create_text("old.txt", "body", Some(folder.id.clone()))
            .unwrap();
        let renamed = v.rename_item(&note.id, "new.txt").unwrap();
        assert_eq!(renamed.name, "new.txt");
        let moved = v.move_item(&note.id, None).unwrap();
        assert!(moved.parent_id.is_none());
        let root = v.list_items_in(None).unwrap();
        assert!(root.iter().any(|i| i.id == note.id && i.name == "new.txt"));
    }

    #[test]
    fn double_setup_fails() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        assert!(v.setup("other").is_err());
    }

    #[test]
    fn guess_mime_common_extensions() {
        assert_eq!(guess_mime("a.PNG"), "image/png");
        assert_eq!(guess_mime("x.jpg"), "image/jpeg");
        assert_eq!(guess_mime("doc.txt"), "text/plain");
        assert_eq!(guess_mime("noext"), "application/octet-stream");
    }

    #[test]
    fn sanitize_blocks_dot_dot_backslash_nul_and_long() {
        assert!(sanitize_name(".").is_err());
        assert!(sanitize_name("..").is_err());
        assert!(sanitize_name("a\\b").is_err());
        assert!(sanitize_name("a\0b").is_err());
        assert!(sanitize_name(&"x".repeat(256)).is_err());
        assert_eq!(sanitize_name("notes.md").unwrap(), "notes.md");
    }

    #[test]
    fn classify_kind_by_mime_and_extension() {
        assert_eq!(classify_kind("text/plain", "x"), ItemKind::Text);
        assert_eq!(classify_kind("image/png", "x"), ItemKind::Image);
        assert_eq!(
            classify_kind("application/octet-stream", "a.md"),
            ItemKind::Text
        );
        assert_eq!(
            classify_kind("application/octet-stream", "a.png"),
            ItemKind::Image
        );
        assert_eq!(
            classify_kind("application/octet-stream", "a.bin"),
            ItemKind::Binary
        );
        assert_eq!(classify_kind("application/json", "data"), ItemKind::Text);
    }

    #[test]
    fn setup_rejects_short_password() {
        let (_dir, mut v) = test_vault();
        let err = v.setup("short").unwrap_err();
        assert!(
            err.to_string().contains("8") || err.to_string().to_lowercase().contains("password")
        );
        assert!(!v.status().initialized);
    }

    #[test]
    fn double_unlock_fails() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        assert!(matches!(
            v.unlock("password1234"),
            Err(AppError::AlreadyUnlocked)
        ));
    }

    #[test]
    fn bad_recovery_key_fails() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        v.lock();
        assert!(v.unlock_with_recovery("not-a-real-key").is_err());
        assert!(v
            .unlock_with_recovery(
                "0000000000000000000000000000000000000000000000000000000000000000"
            )
            .is_err());
    }

    #[test]
    fn change_password_rejects_wrong_current_and_short_new() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        assert!(v
            .change_password("wrong-current", "new-password-456")
            .is_err());
        assert!(v.change_password("password1234", "short").is_err());
        // original still works
        v.lock();
        v.unlock("password1234").unwrap();
    }

    #[test]
    fn idle_lock_zero_never_auto_locks() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        v.set_idle_lock_secs(0).unwrap();
        assert!(!v.check_idle_lock());
        assert!(v.status().unlocked);
    }

    #[test]
    fn idle_lock_triggers_after_elapsed() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        v.set_idle_lock_secs(1).unwrap();
        if let Some(s) = v.session.as_mut() {
            s.last_activity = Instant::now() - Duration::from_secs(5);
        }
        assert!(v.check_idle_lock());
        assert!(!v.status().unlocked);
    }

    #[test]
    fn get_missing_item_errors() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        assert!(matches!(
            v.get_item("00000000-0000-0000-0000-000000000000"),
            Err(AppError::NotFound)
        ));
    }

    #[test]
    fn status_reports_counts_when_locked() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        v.create_text("a.txt", "x", None).unwrap();
        v.create_folder("F", None).unwrap();
        v.lock();
        let s = v.status();
        assert!(s.initialized);
        assert!(!s.unlocked);
        assert_eq!(s.item_count, 2);
        assert!(s.has_recovery_key);
    }

    #[test]
    fn default_vault_root_is_secretfolder_not_sticky() {
        let root = default_vault_root().unwrap();
        let s = root.to_string_lossy().to_lowercase();
        assert!(
            s.contains("secretfolder") || s.contains("com.ahmi.secretfolder"),
            "unexpected root: {s}"
        );
        assert!(!s.contains("secretsticky"));
    }

    // ── Edge-case coverage ─────────────────────────────────────────────────

    #[test]
    fn export_bytes_on_folder_errors() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        let folder = v.create_folder("MyFolder", None).unwrap();
        let err = v.export_bytes(&folder.id).unwrap_err();
        assert!(err.to_string().contains("folder"));
    }

    #[test]
    fn update_text_on_binary_item_errors() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        let item = v
            .import_bytes(
                "data.bin".into(),
                "application/octet-stream".into(),
                vec![1, 2, 3],
                None,
            )
            .unwrap();
        let err = v
            .update_text(&item.id, None, "new body".into())
            .unwrap_err();
        assert!(err.to_string().contains("not a text"));
    }

    #[test]
    fn delete_nonexistent_item_errors() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        let err = v
            .delete_item("00000000-0000-0000-0000-000000000000", false)
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound));
    }

    #[test]
    fn create_text_invalid_parent_errors() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        // A text item cannot serve as a parent.
        let item = v.create_text("ref.txt", "x", None).unwrap();
        let err = v
            .create_text("child.txt", "body", Some(item.id.clone()))
            .unwrap_err();
        assert!(err.to_string().contains("not a folder"));
    }

    #[test]
    fn empty_body_update_text_works() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        let item = v.create_text("emptyable.txt", "initial", None).unwrap();
        v.update_text(&item.id, None, String::new()).unwrap();
        let detail = v.get_item(&item.id).unwrap();
        assert_eq!(detail.text.as_deref(), Some(""));
        assert_eq!(detail.size, 0);
    }

    #[test]
    fn cascade_delete_on_file_works() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        let item = v.create_text("alone.txt", "body", None).unwrap();
        // cascade=true on a file is a no-op but should succeed.
        v.delete_item(&item.id, true).unwrap();
        assert!(v.get_item(&item.id).is_err());
    }

    #[test]
    fn import_bytes_at_max_boundary_works() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        let data = vec![0xABu8; MAX_FILE_BYTES as usize];
        let item = v
            .import_bytes(
                "maxfile.bin".into(),
                "application/octet-stream".into(),
                data,
                None,
            )
            .unwrap();
        let (name, out) = v.export_bytes(&item.id).unwrap();
        assert_eq!(name, "maxfile.bin");
        assert_eq!(out.len(), MAX_FILE_BYTES as usize);
        assert_eq!(out[0], 0xAB);
        assert_eq!(out[out.len() - 1], 0xAB);
    }

    #[test]
    fn sanitize_unicode_and_weird_names() {
        // Valid: emoji, accented chars, spaces
        let ok = sanitize_name(" résumé (2024).txt ").unwrap();
        assert_eq!(ok, "résumé (2024).txt");
        let ok = sanitize_name("📁 project").unwrap();
        assert_eq!(ok, "📁 project");
        let ok = sanitize_name("a_b-c.d").unwrap();
        assert_eq!(ok, "a_b-c.d");
        // Invalid
        assert!(sanitize_name("/root").is_err());
        assert!(sanitize_name(".").is_err());
        assert!(sanitize_name("..").is_err());
    }

    #[test]
    fn set_idle_lock_persists_across_reload() {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();
        {
            let mut v = Vault::open_path_for_test(root.clone()).unwrap();
            v.setup("password1234").unwrap();
            v.set_idle_lock_secs(120).unwrap();
        }
        {
            let mut v = Vault::open_path_for_test(root).unwrap();
            v.unlock("password1234").unwrap();
            assert_eq!(v.status().idle_lock_secs, 120);
        }
    }

    #[test]
    fn move_to_nonexistent_parent_errors() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        let item = v.create_text("move-me.txt", "body", None).unwrap();
        let fake_id = "00000000-0000-0000-0000-000000000000";
        let err = v.move_item(&item.id, Some(fake_id.into())).unwrap_err();
        assert!(matches!(err, AppError::NotFound));
    }

    #[test]
    fn create_folder_under_nonexistent_parent_fails() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        let fake_id = "00000000-0000-0000-0000-000000000000";
        let err = v.create_folder("Lost", Some(fake_id.into())).unwrap_err();
        assert!(matches!(err, AppError::NotFound));
    }

    #[test]
    fn status_reports_zero_items_in_empty_vault() {
        let (_dir, mut v) = test_vault();
        v.setup("password1234").unwrap();
        let s = v.status();
        assert_eq!(s.item_count, 0);
    }

    #[test]
    fn vault_format_version_is_backward_compatible_baseline() {
        assert_eq!(
            VAULT_VERSION, 1,
            "bumping VAULT_VERSION requires a compatible reader for prior on-disk vaults; \
             updates must never strand saved files"
        );
    }

    #[test]
    fn persist_replace_writes_bak_sibling() {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let index = root.join("vault.json");
        let id = {
            let mut v = Vault::open_path_for_test(root.clone()).unwrap();
            v.setup("password1234").unwrap();
            let item = v.create_text("one.txt", "body-one", None).unwrap();
            // Second write must replace vault.json and snapshot .bak first.
            v.update_text(&item.id, Some("one.txt".into()), "body-two".into())
                .unwrap();
            item.id
        };
        assert!(index.exists());
        let bak = backup_path_for(&index);
        assert!(
            bak.exists(),
            "vault.json.bak must be written before replace so a failed swap cannot strand secrets"
        );
        let mut v2 = Vault::open_path_for_test(root).unwrap();
        v2.unlock("password1234").unwrap();
        let got = v2.get_item(&id).unwrap();
        assert_eq!(got.text.as_deref(), Some("body-two"));
    }

    /// If vault.json is missing but vault.json.bak remains (classic delete-then-rename
    /// crash on older builds), open_root must restore secrets — never look uninitialized.
    #[test]
    fn missing_live_vault_restores_from_bak() {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let index = root.join("vault.json");
        let bak = backup_path_for(&index);
        let id = {
            let mut v = Vault::open_path_for_test(root.clone()).unwrap();
            v.setup("password1234").unwrap();
            let item = v
                .create_text("bak-name.txt", "bak-body-secret", None)
                .unwrap();
            // Second persist: bak freezes on the secret-bearing revision.
            v.update_text(
                &item.id,
                Some("bak-name.txt".into()),
                "bak-body-secret".into(),
            )
            .unwrap();
            item.id
        };
        assert!(bak.exists());
        // Simulate catastrophic mid-replace: live file gone, bak still present.
        fs::remove_file(&index).unwrap();
        assert!(!index.exists());

        let mut v2 = Vault::open_path_for_test(root.clone()).unwrap();
        assert!(
            v2.status().initialized,
            "must not treat missing live + present bak as fresh install"
        );
        assert!(index.exists(), "live vault.json must be restored from bak");
        v2.unlock("password1234").unwrap();
        let got = v2.get_item(&id).unwrap();
        assert_eq!(got.text.as_deref(), Some("bak-body-secret"));
        assert_eq!(got.name, "bak-name.txt");
    }

    /// Corrupt live file must not wipe secrets when a good bak exists.
    #[test]
    fn corrupt_live_vault_falls_back_to_bak() {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let index = root.join("vault.json");
        let bak = backup_path_for(&index);
        let id = {
            let mut v = Vault::open_path_for_test(root.clone()).unwrap();
            v.setup("password1234").unwrap();
            let item = v.create_text("good.txt", "still-here", None).unwrap();
            // Advance live once more so bak freezes on the secret-bearing revision.
            v.update_text(&item.id, Some("good.txt".into()), "still-here".into())
                .unwrap();
            item.id
        };
        // Poison the live file; bak still holds the previous good snapshot.
        fs::write(&index, "{not-json").unwrap();
        assert!(bak.exists());

        let mut v2 = Vault::open_path_for_test(root).unwrap();
        assert!(v2.status().initialized);
        v2.unlock("password1234").unwrap();
        let got = v2.get_item(&id).unwrap();
        assert_eq!(got.text.as_deref(), Some("still-here"));
        assert_eq!(got.name, "good.txt");
    }

    /// replace_file must never delete the destination before the new file is in place.
    #[test]
    fn replace_file_keeps_destination_if_source_missing() {
        let dir = tempdir().unwrap();
        let live = dir.path().join("vault.json");
        let missing = dir.path().join("nope.json.tmp");
        fs::write(&live, r#"{"keep":true}"#).unwrap();
        let err = replace_file(&missing, &live);
        assert!(err.is_err(), "missing source must fail replace");
        assert!(
            live.exists(),
            "failed replace must leave existing vault.json intact"
        );
        let body = fs::read_to_string(&live).unwrap();
        assert!(
            body.contains("keep"),
            "destination contents must survive: {err:?}"
        );
    }

    #[test]
    fn refuses_newer_vault_format_version() {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();
        {
            let mut v = Vault::open_path_for_test(root.clone()).unwrap();
            v.setup("password1234").unwrap();
        }
        let index = root.join("vault.json");
        let raw = fs::read_to_string(&index).unwrap();
        let mut file: serde_json::Value = serde_json::from_str(&raw).unwrap();
        file["header"]["version"] = serde_json::json!(VAULT_VERSION + 1);
        fs::write(&index, serde_json::to_string_pretty(&file).unwrap()).unwrap();
        // Drop bak so open cannot silently fall back past the version check.
        let bak = backup_path_for(&index);
        let _ = fs::remove_file(&bak);
        let opened = Vault::open_path_for_test(root);
        assert!(opened.is_err(), "newer vault format must be refused");
        let msg = opened.err().unwrap().to_string();
        assert!(
            msg.contains("newer") || msg.contains("upgrade"),
            "expected version refusal, got: {msg}"
        );
    }
}
