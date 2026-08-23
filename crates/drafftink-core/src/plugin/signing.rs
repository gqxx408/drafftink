//! Plugin signature verification using Ed25519.
//!
//! Workflow:
//!   1. Developer signs the plugin DLL's SHA-512 hash with their Ed25519 private key.
//!   2. The signature is embedded in the plugin manifest (Base64-encoded).
//!   3. At load time, the host re-hashes the DLL and verifies the signature
//!      against either the built-in trusted public key (official plugins) or
//!      the developer's declared public key (community plugins).

use sha2::{Digest, Sha512};

/// Result of verifying a plugin signature.
#[derive(Debug, Clone, PartialEq)]
pub enum SigStatus {
    /// Signature verified against the trusted or declared key.
    Verified,
    /// Signature verified against a non-official key.
    SelfSigned,
    /// Signature present but could not be verified (tampered).
    Rejected,
    /// No signature field in manifest.
    Unsigned,
}

/// Generate a fresh Ed25519 keypair.
pub fn generate_keypair() -> ([u8; 32], [u8; 32]) {
    use rand::rngs::OsRng;
    let mut csprng = OsRng;
    let signing_key = ed25519_dalek::SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();
    (
        signing_key.to_bytes(),
        verifying_key.to_bytes(),
    )
}

/// Compute the SHA-512 hash of a file's contents.
pub fn hash_file(path: &std::path::Path) -> Result<[u8; 64], String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    Ok(hash_bytes(&data))
}

/// Compute SHA-512 of a byte slice.
pub fn hash_bytes(data: &[u8]) -> [u8; 64] {
    let mut h = Sha512::new();
    h.update(data);
    h.finalize().into()
}

/// Sign a byte buffer. Returns raw 64-byte signature.
pub fn sign_data(private_key: &[u8; 32], data: &[u8]) -> Result<[u8; 64], String> {
    use ed25519_dalek::Signer;
    let signing_key = ed25519_dalek::SigningKey::from_bytes(private_key);
    let hash = hash_bytes(data);
    let sig = signing_key.sign(&hash);
    Ok(sig.to_bytes())
}

/// Verify a plugin signature.
///
/// - `dll_data`  : the raw bytes of the plugin DLL.
/// - `signature_b64` : Base64-encoded Ed25519 signature from the manifest.
/// - `public_key_b64` : optional developer public key (Base64). If `None`,
///   the built-in trusted key is used (compile-time embedded).
pub fn verify_signature(
    dll_data: &[u8],
    signature_b64: &str,
    public_key_b64: Option<&str>,
    trusted_key: Option<&[u8; 32]>,
) -> Result<SigStatus, String> {
    let sig_bytes = base64_decode(signature_b64)?;
    let signature = ed25519_dalek::Signature::try_from(sig_bytes.as_slice())
        .map_err(|e| format!("Invalid signature bytes: {e}"))?;

    let hash = hash_bytes(dll_data);

    // Determine which public key to verify against
    let pubkey_bytes = match public_key_b64 {
        Some(b64) => base64_decode(b64)?,
        None => match trusted_key {
            Some(k) => k.to_vec(),
            None => return Err("No public key available for verification".into()),
        },
    };

    let pubkey: [u8; 32] = pubkey_bytes
        .try_into()
        .map_err(|_| "Public key must be 32 bytes".to_string())?;
    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pubkey)
        .map_err(|e| format!("Invalid public key: {e}"))?;

    use ed25519_dalek::Verifier;
    match verifying_key.verify(&hash, &signature) {
        Ok(()) => {
            let is_self_signed = public_key_b64.is_some()
                || trusted_key.map(|k| k.as_slice() != pubkey.as_slice()).unwrap_or(true);
            if is_self_signed {
                Ok(SigStatus::SelfSigned)
            } else {
                Ok(SigStatus::Verified)
            }
        }
        Err(_) => Ok(SigStatus::Rejected),
    }
}

/// Sign a DLL file and return the Base64-encoded signature.
pub fn sign_file(path: &std::path::Path, private_key: &[u8; 32]) -> Result<String, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let sig = sign_data(private_key, &data)?;
    Ok(base64_encode(&sig))
}

fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|e| e.to_string())
}

pub(crate) fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify_roundtrip() {
        let (sk, pk) = generate_keypair();
        let dll = b"fake plugin dll content";

        let sig_bytes = sign_data(&sk, dll).unwrap();
        let sig_b64 = base64_encode(&sig_bytes);
        let pk_b64 = base64_encode(&pk);

        let status = verify_signature(dll, &sig_b64, Some(&pk_b64), None).unwrap();
        assert_eq!(status, SigStatus::SelfSigned);
    }

    #[test]
    fn tampered_dll_fails_verification() {
        let (sk, pk) = generate_keypair();
        let original = b"original dll";
        let tampered = b"tampered dll!";

        let sig_bytes = sign_data(&sk, original).unwrap();
        let sig_b64 = base64_encode(&sig_bytes);
        let pk_b64 = base64_encode(&pk);

        let status = verify_signature(tampered, &sig_b64, Some(&pk_b64), None).unwrap();
        assert_eq!(status, SigStatus::Rejected);
    }

    #[test]
    fn trusted_key_returns_verified() {
        let (sk, pk) = generate_keypair();
        let dll = b"official plugin v1";

        let sig_bytes = sign_data(&sk, dll).unwrap();
        let sig_b64 = base64_encode(&sig_bytes);

        // No developer key → uses trusted key
        let status = verify_signature(dll, &sig_b64, None, Some(&pk)).unwrap();
        assert_eq!(status, SigStatus::Verified);
    }

    #[test]
    fn wrong_key_rejected() {
        let (sk1, _pk1) = generate_keypair();
        let (_sk2, pk2) = generate_keypair();
        let dll = b"some dll";

        let sig_bytes = sign_data(&sk1, dll).unwrap();
        let sig_b64 = base64_encode(&sig_bytes);
        let pk_b64 = base64_encode(&pk2);

        let status = verify_signature(dll, &sig_b64, Some(&pk_b64), None).unwrap();
        assert_eq!(status, SigStatus::Rejected);
    }

    #[test]
    fn empty_dll_hashes_fine() {
        let h = hash_bytes(b"");
        assert_eq!(h.len(), 64);
    }

    /// 集成测试：用测试私钥签名一个 mock 插件二进制，写入同目录 `.sig` 文件，
    /// 验证通过；篡改磁盘上的二进制后验证失败。
    #[test]
    fn sign_and_verify_mock_plugin_disk_sig_file() {
        let (sk, pk) = generate_keypair();
        let dll = b"mock plugin binary for enbx importer v1.0";
        let sig = sign_data(&sk, dll).unwrap();
        let sig_b64 = base64_encode(&sig);

        let dir = std::env::temp_dir().join(format!("drafftink_sign_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dll_path = dir.join("plugin.dll");
        let sig_path = dir.join("plugin.sig");
        std::fs::write(&dll_path, dll).unwrap();
        std::fs::write(&sig_path, &sig_b64).unwrap();

        // 读取磁盘上的插件与签名，验证应成功
        let read_dll = std::fs::read(&dll_path).unwrap();
        let read_sig = std::fs::read_to_string(&sig_path).unwrap();
        let status = verify_signature(&read_dll, &read_sig, None, Some(&pk)).unwrap();
        assert_eq!(status, SigStatus::Verified);

        // 篡改磁盘上的插件二进制后重新校验应失败
        std::fs::write(&dll_path, b"tampered malicious binary").unwrap();
        let tampered = std::fs::read(&dll_path).unwrap();
        let status2 = verify_signature(&tampered, &read_sig, None, Some(&pk)).unwrap();
        assert_eq!(status2, SigStatus::Rejected);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
