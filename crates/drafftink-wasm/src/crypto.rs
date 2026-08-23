//! Crypto integration.
//!
//! On WASM, keypairs are generated/loaded and persisted in LocalStorage.
//! Signing uses [`drafftink_core`]'s pure-Rust Ed25519 implementation which
//! works identically on both native and WASM targets.
//!
//! The Web Crypto SubtleCrypto API could be used for key generation in the
//! future, but `ed25519-dalek` (via `drafftink-core`) is already pure Rust
//! with no C dependencies, so it is used directly on both targets.

use anyhow::{anyhow, Result};

// ════════════════════════════════════════════════════════════════════════════
//  Shared hex helpers (testable on native)
// ════════════════════════════════════════════════════════════════════════════

/// Encode a byte slice as a lowercase hex string.
pub fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Decode a hex string into a byte vector.
pub fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

/// LocalStorage key for the student private key.
pub const KEY_STORAGE_SK: &str = "drafftink:student_sk";

/// LocalStorage key for the student public key.
pub const KEY_STORAGE_PK: &str = "drafftink:student_pk";

// ════════════════════════════════════════════════════════════════════════════
//  WASM: generate/load keypair with LocalStorage persistence
// ════════════════════════════════════════════════════════════════════════════

#[cfg(target_arch = "wasm32")]
mod wasm_impl {
    use super::{bytes_to_hex, hex_to_bytes, KEY_STORAGE_PK, KEY_STORAGE_SK};
    use anyhow::{anyhow, Result};
    use drafftink_core::crypto::generate_keypair;

    /// Generate or load an Ed25519 keypair.
    ///
    /// On first run a new keypair is generated with
    /// [`drafftink_core::crypto::generate_keypair`] and persisted to
    /// LocalStorage as hex strings. Subsequent calls return the stored
    /// keypair.
    pub fn generate_or_load_keypair() -> Result<([u8; 32], [u8; 32])> {
        // Try loading from LocalStorage
        if let (Some(sk_hex), Some(pk_hex)) = (
            crate::browser::local_storage_get(KEY_STORAGE_SK),
            crate::browser::local_storage_get(KEY_STORAGE_PK),
        ) {
            if let (Some(sk_bytes), Some(pk_bytes)) =
                (hex_to_bytes(&sk_hex), hex_to_bytes(&pk_hex))
            {
                if sk_bytes.len() == 32 && pk_bytes.len() == 32 {
                    let mut sk = [0u8; 32];
                    let mut pk = [0u8; 32];
                    sk.copy_from_slice(&sk_bytes);
                    pk.copy_from_slice(&pk_bytes);
                    return Ok((sk, pk));
                }
            }
        }

        // Generate new keypair
        let (sk, pk) = generate_keypair();

        // Persist
        crate::browser::local_storage_set(KEY_STORAGE_SK, &bytes_to_hex(&sk));
        crate::browser::local_storage_set(KEY_STORAGE_PK, &bytes_to_hex(&pk));

        Ok((sk, pk))
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  Native stub: use drafftink-core directly (no persistence)
// ════════════════════════════════════════════════════════════════════════════

#[cfg(not(target_arch = "wasm32"))]
mod native_impl {
    use anyhow::Result;
    use drafftink_core::crypto::generate_keypair;

    /// Generate a fresh Ed25519 keypair (no persistence on native).
    pub fn generate_or_load_keypair() -> Result<([u8; 32], [u8; 32])> {
        Ok(generate_keypair())
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  Cross-platform re-exports
// ════════════════════════════════════════════════════════════════════════════

#[cfg(target_arch = "wasm32")]
pub use wasm_impl::generate_or_load_keypair;

#[cfg(not(target_arch = "wasm32"))]
pub use native_impl::generate_or_load_keypair;

// ════════════════════════════════════════════════════════════════════════════
//  Signing — uses drafftink-core on both targets (pure Rust, no C deps)
// ════════════════════════════════════════════════════════════════════════════

/// Sign data with an Ed25519 private key.
///
/// Uses [`drafftink_core::crypto::sign_raw`] which is a pure-Rust
/// implementation via `ed25519-dalek`. On WASM the Web Crypto SubtleCrypto
/// API could be substituted in the future, but the result is identical.
pub fn sign_with_web_crypto(data: &[u8], private_key: &[u8]) -> Result<Vec<u8>> {
    let sk: [u8; 32] = private_key
        .try_into()
        .map_err(|_| anyhow!("private key must be 32 bytes"))?;
    let sig = drafftink_core::crypto::sign_raw(&sk, data);
    Ok(sig.to_vec())
}

// ════════════════════════════════════════════════════════════════════════════
//  Unit tests (run on native)
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use drafftink_core::crypto::{generate_keypair, verify_raw};

    #[test]
    fn test_bytes_to_hex_roundtrip() {
        let data = [0x00, 0xff, 0xab, 0x01, 0x80];
        let hex = bytes_to_hex(&data);
        assert_eq!(hex, "00ffab0180");
        let restored = hex_to_bytes(&hex).unwrap();
        assert_eq!(restored, data.to_vec());
    }

    #[test]
    fn test_hex_to_bytes_odd_length() {
        assert!(hex_to_bytes("abc").is_none());
    }

    #[test]
    fn test_hex_to_bytes_invalid_chars() {
        assert!(hex_to_bytes("xy").is_none());
    }

    #[test]
    fn test_hex_to_bytes_empty() {
        assert_eq!(hex_to_bytes("").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn test_generate_or_load_keypair_native() {
        let (sk, pk) = generate_or_load_keypair().unwrap();
        assert_eq!(sk.len(), 32);
        assert_eq!(pk.len(), 32);
        // Native has no persistence, so two calls produce different keys
        let (sk2, _) = generate_or_load_keypair().unwrap();
        assert_ne!(sk, sk2);
    }

    #[test]
    fn test_sign_and_verify() {
        let (sk, pk) = generate_keypair();
        let data = b"homework answer data";

        let signature = sign_with_web_crypto(data, &sk).unwrap();
        assert_eq!(signature.len(), 64);

        let sig_arr: [u8; 64] = signature
            .as_slice()
            .try_into()
            .expect("signature is 64 bytes");
        assert!(verify_raw(&pk, data, &sig_arr));
    }

    #[test]
    fn test_sign_wrong_key_rejected() {
        let (sk1, _) = generate_keypair();
        let (_, pk2) = generate_keypair();
        let data = b"test data";

        let signature = sign_with_web_crypto(data, &sk1).unwrap();
        let sig_arr: [u8; 64] = signature
            .as_slice()
            .try_into()
            .expect("signature is 64 bytes");
        assert!(!verify_raw(&pk2, data, &sig_arr));
    }

    #[test]
    fn test_sign_tampered_data_rejected() {
        let (sk, pk) = generate_keypair();
        let original = b"original answer";
        let tampered = b"tampered answer!";

        let signature = sign_with_web_crypto(original, &sk).unwrap();
        let sig_arr: [u8; 64] = signature
            .as_slice()
            .try_into()
            .expect("signature is 64 bytes");
        assert!(!verify_raw(&pk, tampered, &sig_arr));
    }

    #[test]
    fn test_sign_invalid_key_length() {
        let short_key = [0u8; 16];
        let result = sign_with_web_crypto(b"data", &short_key);
        assert!(result.is_err());
    }
}
