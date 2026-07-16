//! Encryption for credentials at rest.
//!
//! Secret option values (tokens, API keys, passwords) are AES-256-GCM encrypted
//! before they're stored, and decrypted only when building a connector. The key
//! comes from `BUDBUK_SECRET_KEY` (base64, 32 bytes); if unset, a random key is
//! generated at startup (fine for the in-memory MVP — restart drops all state).

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use rand::RngCore;

pub struct Cipher {
    inner: Aes256Gcm,
}

impl Cipher {
    /// Build the cipher from `BUDBUK_SECRET_KEY`, or a random key if unset.
    pub fn from_env() -> Self {
        let key = std::env::var("BUDBUK_SECRET_KEY")
            .ok()
            .and_then(|s| STANDARD.decode(s).ok())
            .filter(|b| b.len() == 32)
            .unwrap_or_else(|| {
                let mut k = vec![0u8; 32];
                rand::rngs::OsRng.fill_bytes(&mut k);
                k
            });
        Self {
            inner: Aes256Gcm::new_from_slice(&key).expect("32-byte key"),
        }
    }

    /// Encrypt a plaintext string → base64(`nonce || ciphertext`).
    pub fn encrypt(&self, plaintext: &str) -> String {
        let mut nonce = [0u8; 12];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let ct = self
            .inner
            .encrypt(Nonce::from_slice(&nonce), plaintext.as_bytes())
            .expect("aead encrypt");
        let mut buf = nonce.to_vec();
        buf.extend(ct);
        STANDARD.encode(buf)
    }

    /// Decrypt a token produced by [`encrypt`](Self::encrypt).
    pub fn decrypt(&self, token: &str) -> Result<String, String> {
        let data = STANDARD.decode(token).map_err(|e| e.to_string())?;
        if data.len() < 12 {
            return Err("ciphertext too short".into());
        }
        let (nonce, ct) = data.split_at(12);
        let pt = self
            .inner
            .decrypt(Nonce::from_slice(nonce), ct)
            .map_err(|_| "decrypt failed".to_string())?;
        String::from_utf8(pt).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_hides_plaintext() {
        let c = Cipher::from_env();
        let secret = "sk_live_abc123";
        let token = c.encrypt(secret);
        assert_ne!(token, secret);
        assert!(!token.contains(secret));
        assert_eq!(c.decrypt(&token).unwrap(), secret);
    }

    #[test]
    fn distinct_nonces_produce_distinct_ciphertext() {
        let c = Cipher::from_env();
        assert_ne!(c.encrypt("same"), c.encrypt("same"));
    }

    #[test]
    fn bad_input_errors() {
        let c = Cipher::from_env();
        assert!(c.decrypt("not base64 !!!").is_err());
        assert!(c.decrypt("QUJD").is_err()); // valid base64 but too short
    }
}
