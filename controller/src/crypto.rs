//! Secrets-at-rest and token primitives (docs/SECURITY.md).
//!
//! `SecretBox` is AES-256-GCM with the key from `FOUNDRY_ENCRYPTION_KEY`
//! (base64, 32 bytes). Ciphertext layout: 12-byte nonce ‖ ciphertext.
//! The same key must be configured for every controller process that
//! shares the database — rotating it requires re-encrypting stored
//! secrets.

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::Engine as _;
use sha2::{Digest, Sha256};

const NONCE_LEN: usize = 12;

#[derive(Clone)]
pub struct SecretBox {
    cipher: Aes256Gcm,
}

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("FOUNDRY_ENCRYPTION_KEY must be base64 of exactly 32 bytes")]
    BadKey,
    #[error("decryption failed (wrong key or corrupted ciphertext)")]
    Decrypt,
}

impl SecretBox {
    pub fn from_base64_key(b64: &str) -> Result<Self, CryptoError> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|_| CryptoError::BadKey)?;
        if bytes.len() != 32 {
            return Err(CryptoError::BadKey);
        }
        let key = Key::<Aes256Gcm>::from_slice(&bytes);
        Ok(Self {
            cipher: Aes256Gcm::new(key),
        })
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Vec<u8> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let mut out = nonce.to_vec();
        // Encryption with a fresh random nonce cannot fail.
        let ct = self
            .cipher
            .encrypt(&nonce, plaintext)
            .expect("AES-GCM encryption is infallible with a valid key");
        out.extend_from_slice(&ct);
        out
    }

    pub fn encrypt_str(&self, plaintext: &str) -> Vec<u8> {
        self.encrypt(plaintext.as_bytes())
    }

    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if data.len() < NONCE_LEN {
            return Err(CryptoError::Decrypt);
        }
        let (nonce, ct) = data.split_at(NONCE_LEN);
        self.cipher
            .decrypt(Nonce::from_slice(nonce), ct)
            .map_err(|_| CryptoError::Decrypt)
    }

    pub fn decrypt_str(&self, data: &[u8]) -> Result<String, CryptoError> {
        String::from_utf8(self.decrypt(data)?).map_err(|_| CryptoError::Decrypt)
    }
}

/// 256-bit URL-safe random token (session tokens, enrollment tokens).
pub fn random_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// SHA-256 of a presented token — the only form stored at rest.
pub fn token_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_box() -> SecretBox {
        let key = base64::engine::general_purpose::STANDARD.encode([7u8; 32]);
        SecretBox::from_base64_key(&key).expect("valid key")
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        let sb = test_box();
        let ct = sb.encrypt_str("glpat-secret-token");
        assert_ne!(ct, b"glpat-secret-token");
        assert_eq!(sb.decrypt_str(&ct).expect("decrypts"), "glpat-secret-token");
    }

    #[test]
    fn distinct_nonces_distinct_ciphertexts() {
        let sb = test_box();
        assert_ne!(sb.encrypt_str("x"), sb.encrypt_str("x"));
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let sb = test_box();
        let mut ct = sb.encrypt_str("x");
        let last = ct.len() - 1;
        ct[last] ^= 0xff;
        assert!(sb.decrypt(&ct).is_err());
    }

    #[test]
    fn bad_key_rejected() {
        assert!(SecretBox::from_base64_key("dG9vLXNob3J0").is_err());
        assert!(SecretBox::from_base64_key("not base64!!").is_err());
    }

    #[test]
    fn random_tokens_unique_and_hashable() {
        let a = random_token();
        let b = random_token();
        assert_ne!(a, b);
        assert_eq!(token_hash(&a).len(), 32);
    }
}
