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

#[allow(dead_code)]
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

#[allow(dead_code)]
pub fn b64_encode(data: &[u8]) -> String {
    B64.encode(data)
}

#[allow(dead_code)]
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

    #[test]
    fn empty_plaintext_roundtrip() {
        let key = MasterKey::from_bytes(random_array());
        let blob = encrypt(&key, b"", b"empty-test").unwrap();
        let out = decrypt(&key, &blob, b"empty-test").unwrap();
        assert_eq!(out, b"");
    }

    #[test]
    fn wrong_key_fails() {
        let key_a = MasterKey::from_bytes(random_array());
        let key_b = MasterKey::from_bytes(random_array());
        let blob = encrypt(&key_a, b"sensitive-data", b"key-test").unwrap();
        assert!(decrypt(&key_b, &blob, b"key-test").is_err());
    }

    #[test]
    fn wrong_key_same_bytes_but_different_instance_fails() {
        // Verify that a different key derivation produces different cipher state.
        let bytes_a = random_array::<32>();
        let bytes_b = random_array::<32>();
        // ensure they differ
        if bytes_a == bytes_b {
            // incredibly unlikely (1/2^256), but skip to avoid flake
            return;
        }
        let key_a = MasterKey::from_bytes(bytes_a);
        let key_b = MasterKey::from_bytes(bytes_b);
        let blob = encrypt(&key_a, b"hi", b"same-test").unwrap();
        assert!(decrypt(&key_b, &blob, b"same-test").is_err());
    }

    #[test]
    fn recovery_key_format() {
        let k = generate_recovery_key();
        // 32 bytes → 64 hex chars → 16 groups of 4, hyphen-separated, uppercase
        let parts: Vec<&str> = k.split('-').collect();
        assert_eq!(parts.len(), 16, "expected 16 groups, got {k}");
        for part in &parts {
            assert_eq!(part.len(), 4);
            assert!(part.chars().all(|c| c.is_ascii_hexdigit()));
            assert_eq!(*part, part.to_uppercase());
        }
        let norm = normalize_recovery_key(&k);
        assert_eq!(norm.len(), 64);
        assert!(!norm.contains('-'));
        assert_eq!(norm, normalize_recovery_key(&k));
        // Mixed case / spaces still normalize
        let messy = format!("  {}  ", k.to_lowercase());
        assert_eq!(normalize_recovery_key(&messy), norm);
    }

    #[test]
    fn decrypt_rejects_short_blob() {
        let key = MasterKey::from_bytes(random_array());
        assert!(decrypt(&key, b"short", b"aad").is_err());
        assert!(decrypt(&key, &vec![0u8; NONCE_LEN + 15], b"aad").is_err());
    }

    #[test]
    fn master_key_from_slice_rejects_bad_len() {
        assert!(MasterKey::from_slice(&[0u8; 16]).is_err());
        assert!(MasterKey::from_slice(&[0u8; 32]).is_ok());
    }

    #[test]
    fn large_plaintext_roundtrip() {
        let key = MasterKey::from_bytes(random_array());
        let pt = vec![0xABu8; 100_000]; // 100 KB
        let blob = encrypt(&key, &pt, b"large-test").unwrap();
        let out = decrypt(&key, &blob, b"large-test").unwrap();
        assert_eq!(out.len(), 100_000);
        assert_eq!(out, pt);
        // Ciphertext should be longer (nonce + tag overhead)
        assert!(blob.len() > pt.len());
    }

    #[test]
    fn wrong_aad_on_large_blob() {
        let key = MasterKey::from_bytes(random_array());
        let pt = b"large enough to matter";
        let blob = encrypt(&key, pt, b"correct-aad").unwrap();
        assert!(decrypt(&key, &blob, b"wrong-aad").is_err());
        assert!(decrypt(&key, &blob, b"correct-aad").is_ok());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = MasterKey::from_bytes(random_array());
        let pt = b"don't touch me";
        let mut blob = encrypt(&key, pt, b"tamper-test").unwrap();
        // Flip a bit in the ciphertext body (past the nonce)
        if blob.len() > 20 {
            blob[15] ^= 0xFF;
            assert!(decrypt(&key, &blob, b"tamper-test").is_err());

            // Flip a bit in the poly1305 tag (last 16 bytes)
            let tag_start = blob.len() - 16;
            blob[tag_start] ^= 0x01;
            assert!(decrypt(&key, &blob, b"tamper-test").is_err());
        }
    }

    #[test]
    fn normalized_recovery_invalid() {
        // Too short
        let n = "abc";
        assert_eq!(normalize_recovery_key(n), "abc");

        // Too long
        let long = "a".repeat(100);
        assert_eq!(normalize_recovery_key(&long), long);

        // Already normalized (no hyphens, hex only)
        let hex = "abcd1234efab5678";
        assert_eq!(normalize_recovery_key(hex), "abcd1234efab5678");
    }
}
