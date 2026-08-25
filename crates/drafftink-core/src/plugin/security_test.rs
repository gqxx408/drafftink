#[cfg(test)]
mod tests {
    use crate::plugin::sandbox::safe_call;
    use crate::plugin::signing::SigStatus;

    // ── Signing tests ────────────────────────────────────────────

    #[test]
    fn sign_and_verify_roundtrip() {
        let (sk, pk) = crate::plugin::signing::generate_keypair();
        let dll = b"fake dll content for testing";
        let sig_bytes = crate::plugin::signing::sign_data(&sk, dll).unwrap();
        let sig_b64 = crate::plugin::signing::base64_encode(&sig_bytes);
        let pk_b64 = crate::plugin::signing::base64_encode(&pk);

        let status =
            crate::plugin::signing::verify_signature(dll, &sig_b64, Some(&pk_b64), None).unwrap();
        assert_eq!(status, SigStatus::SelfSigned);
    }

    #[test]
    fn tampered_dll_rejected() {
        let (sk, pk) = crate::plugin::signing::generate_keypair();
        let sig_bytes = crate::plugin::signing::sign_data(&sk, b"original").unwrap();
        let sig_b64 = crate::plugin::signing::base64_encode(&sig_bytes);
        let pk_b64 = crate::plugin::signing::base64_encode(&pk);

        let status =
            crate::plugin::signing::verify_signature(b"tampered!", &sig_b64, Some(&pk_b64), None)
                .unwrap();
        assert_eq!(status, SigStatus::Rejected);
    }

    #[test]
    fn trusted_key_returns_verified() {
        let (sk, pk) = crate::plugin::signing::generate_keypair();
        let sig_bytes = crate::plugin::signing::sign_data(&sk, b"official").unwrap();
        let sig_b64 = crate::plugin::signing::base64_encode(&sig_bytes);

        let status =
            crate::plugin::signing::verify_signature(b"official", &sig_b64, None, Some(&pk))
                .unwrap();
        assert_eq!(status, SigStatus::Verified);
    }

    #[test]
    fn wrong_key_rejected() {
        let (sk1, _) = crate::plugin::signing::generate_keypair();
        let (_, pk2) = crate::plugin::signing::generate_keypair();
        let sig_bytes = crate::plugin::signing::sign_data(&sk1, b"test").unwrap();
        let sig_b64 = crate::plugin::signing::base64_encode(&sig_bytes);
        let pk_b64 = crate::plugin::signing::base64_encode(&pk2);

        let status =
            crate::plugin::signing::verify_signature(b"test", &sig_b64, Some(&pk_b64), None)
                .unwrap();
        assert_eq!(status, SigStatus::Rejected);
    }

    // ── Panic isolation tests ────────────────────────────────────

    #[test]
    fn safe_call_captures_panic() {
        let result: Result<(), String> = safe_call("test_plugin", "do_work", || {
            panic!("intentional test panic");
        });
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("intentional test panic"));
    }

    #[test]
    fn safe_call_returns_value_on_success() {
        let result = safe_call("test_plugin", "calc", || 42);
        assert_eq!(result, Ok(42));
    }

    // ── Audit test ────────────────────────────────────────────────

    #[test]
    fn audit_logger_writes_and_flushes() {
        let tmp = std::env::temp_dir().join("drafftink_test_audit");
        let _ = std::fs::create_dir_all(&tmp);

        let mut logger = crate::plugin::audit::AuditLogger::new(&tmp).expect("create audit logger");

        logger.log_event("test_plugin", "test_action", "none", "ok", true);
        logger.log_event("test_plugin", "test_action2", "data", "err", false);

        // Verify the file exists and has content
        let today = chrono::Utc::now().format("%Y%m%d");
        let log_path = tmp.join(format!("plugin_audit_{today}.jsonl"));
        assert!(log_path.exists());

        let contents = std::fs::read_to_string(&log_path).unwrap();
        assert!(contents.contains("test_action"));
        assert!(contents.contains("test_action2"));
        assert!(contents.contains("err"));

        // Cleanup
        let _ = std::fs::remove_file(&log_path);
    }
}
