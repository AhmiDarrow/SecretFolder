//! Argon2id + XChaCha20-Poly1305 helpers.
//! AAD namespace: secretfolder-* (never reuse SecretSticky AAD strings).

use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use rand::RngCore;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{AppError, AppResult};

pub const KEY_LEN: usize = 32;
pub const SALT_LEN: usize = 16;
pub const NONCE_LEN: usize = 24;

/// Tuned for interactive unlock on modern desktops (~0.3–1s).
pub const ARGON2_M_KIB: u32 = 64 * 1024; // 64 MiB
pub const ARGON2_T: u32 = 3;
pub const ARGON2_P: u32 = 1;

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct MasterKey {
    bytes: [u8; KEY_LEN],
}

impl MasterKey {
    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self { bytes }
    }

    pub fn from_slice(slice: &[u8]) -> AppResult<Self> {
        if slice.len() != KEY_LEN {
            return Err(AppError::Crypto("bad key length".into()));
        }
        let mut bytes = [0u8; KEY_LEN];
        bytes.copy_from_slice(slice);
        Ok(Self { bytes })
    }

    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.bytes
    }
}

pub fn random_bytes(len: usize) -> Vec<u8> {
    let mut buf = vec![0u8; len];
    rand::thread_rng().fill_bytes(&mut buf);
    buf
}

pub fn random_array<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    rand::thread_rng().fill_bytes(&mut buf);
    buf
}

/// Derive a 32-byte master key from password + salt via Argon2id.
pub fn derive_key(password: &str, salt: &[u8], m_kib: u32, t: u32, p: u32) -> AppResult<MasterKey> {
    let params = Params::new(m_kib, t, p, Some(KEY_LEN))
        .map_err(|e| AppError::Crypto(format!("argon2 params: {e}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = [0u8; KEY_LEN];
    argon
        .hash_password_into(password.as_bytes(), salt, &mut out)
        .map_err(|e| AppError::Crypto(format!("argon2: {e}")))?;
    Ok(MasterKey::from_bytes(out))
}

/// Encrypt plaintext with XChaCha20-Poly1305. Returns nonce || ciphertext+tag.
pub fn encrypt(key: &MasterKey, plaintext: &[u8], aad: &[u8]) -> AppResult<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new_from_slice(key.as_bytes())
        .map_err(|e| AppError::Crypto(format!("cipher: {e}")))?;
    let nonce_bytes = random_array::<NONCE_LEN>();
    let nonce = XNonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| AppError::Crypto("encrypt failed".into()))?;
    let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Decrypt nonce || ciphertext+tag produced by [`encrypt`].
pub fn decrypt(key: &MasterKey, blob: &[u8], aad: &[u8]) -> AppResult<Vec<u8>> {
    if blob.len() < NONCE_LEN + 16 {
        return Err(AppError::Crypto("ciphertext too short".into()));
    }
    let (nonce_bytes, ct) = blob.split_at(NONCE_LEN);
    let cipher = XChaCha20Poly1305::new_from_slice(key.as_bytes())
        .map_err(|e| AppError::Crypto(format!("cipher: {e}")))?;
    let nonce = XNonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, Payload { msg: ct, aad })
        .map_err(|_| AppError::BadPassword)
}

pub fn b64_encode(data: &[u8]) -> String {
    B64.encode(data)
}

pub fn b64_decode(data: &str) -> AppResult<Vec<u8>> {
    B64.decode(data)
        .map_err(|e| AppError::Crypto(format!("b64: {e}")))
}

/// 32-byte recovery key rendered as 8 groups of 4 hex chars.
pub fn generate_recovery_key() -> String {
    let raw = random_array::<32>();
    let hex = hex::encode(raw);
    hex.as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).unwrap_or("0000"))
        .collect::<Vec<_>>()
        .join("-")
        .to_uppercase()
}

pub fn normalize_recovery_key(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect::<String>()
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_encrypt() {
        let key = MasterKey::from_bytes(random_array());
        let pt = b"hello secretfolder";
        let blob = encrypt(&key, pt, b"secretfolder-test").unwrap();
        let out = decrypt(&key, &blob, b"secretfolder-test").unwrap();
        assert_eq!(out, pt);
    }

    #[test]
    fn wrong_aad_fails() {
        let key = MasterKey::from_bytes(random_array());
        let blob = encrypt(&key, b"data", b"aad-a").unwrap();
        assert!(decrypt(&key, &blob, b"aad-b").is_err());
    }

    #[test]
    fn recovery_normalize() {
        let k = generate_recovery_key();
        let n = normalize_recovery_key(&k);
        assert_eq!(n.len(), 64);
        assert!(n.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn derive_key_deterministic() {
        let salt = [7u8; SALT_LEN];
        let a = derive_key("password123", &salt, 8 * 1024, 1, 1).unwrap();
        let b = derive_key("password123", &salt, 8 * 1024, 1, 1).unwrap();
        assert_eq!(a.as_bytes(), b.as_bytes());
    }
}
