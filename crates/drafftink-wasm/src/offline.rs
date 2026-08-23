//! Offline cache and auto-resend logic.
//!
//! On WASM, drafts and pending submissions are stored in LocalStorage.
//! When the browser comes back online, [`try_resubmit`] attempts to resend
//! pending submissions and clears them on success.
//!
//! On native targets all functions are no-ops or return empty collections,
//! but the serialization helpers are shared and unit-tested.

use uuid::Uuid;

use crate::crypto::{bytes_to_hex, hex_to_bytes};

// ════════════════════════════════════════════════════════════════════════════
//  Shared key helpers (testable on native)
// ════════════════════════════════════════════════════════════════════════════

/// Prefix for draft keys in LocalStorage.
pub const DRAFT_KEY_PREFIX: &str = "drafftink:draft:";

/// Prefix for pending-submission keys in LocalStorage.
pub const PENDING_KEY_PREFIX: &str = "drafftink:pending:";

/// Build the LocalStorage key for a draft.
pub fn draft_key(homework_id: Uuid) -> String {
    format!("{DRAFT_KEY_PREFIX}{homework_id}")
}

/// Build the LocalStorage key for a pending submission.
pub fn pending_key(homework_id: Uuid) -> String {
    format!("{PENDING_KEY_PREFIX}{homework_id}")
}

/// Serialize draft bytes to a hex string for LocalStorage storage.
pub fn serialize_draft(data: &[u8]) -> String {
    bytes_to_hex(data)
}

/// Deserialize a hex string from LocalStorage back to draft bytes.
pub fn deserialize_draft(s: &str) -> Option<Vec<u8>> {
    hex_to_bytes(s)
}

// ════════════════════════════════════════════════════════════════════════════
//  WASM implementations
// ════════════════════════════════════════════════════════════════════════════

#[cfg(target_arch = "wasm32")]
mod wasm_impl {
    use super::{deserialize_draft, draft_key, pending_key, serialize_draft, DRAFT_KEY_PREFIX, PENDING_KEY_PREFIX};
    use uuid::Uuid;

    /// Save draft answer data to LocalStorage.
    pub fn save_draft(homework_id: Uuid, data: &[u8]) {
        let key = draft_key(homework_id);
        let value = serialize_draft(data);
        crate::browser::local_storage_set(&key, &value);
    }

    /// Load draft answer data from LocalStorage.
    pub fn load_draft(homework_id: Uuid) -> Option<Vec<u8>> {
        let key = draft_key(homework_id);
        let value = crate::browser::local_storage_get(&key)?;
        deserialize_draft(&value)
    }

    /// Add a pending submission to LocalStorage.
    pub fn add_pending_submission(homework_id: Uuid, data: Vec<u8>) {
        let key = pending_key(homework_id);
        let value = serialize_draft(&data);
        crate::browser::local_storage_set(&key, &value);
    }

    /// Remove a pending submission from LocalStorage.
    pub fn clear_pending_submission(homework_id: Uuid) {
        let key = pending_key(homework_id);
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                let _ = storage.remove_item(&key);
            }
        }
    }

    /// Return all pending submissions from LocalStorage.
    pub fn get_pending_submissions() -> Vec<(Uuid, Vec<u8>)> {
        let mut result = Vec::new();
        let Some(window) = web_sys::window() else {
            return result;
        };
        let Ok(Some(storage)) = window.local_storage() else {
            return result;
        };
        let len = storage.length().unwrap_or(0);
        for i in 0..len {
            if let Ok(Some(key)) = storage.key(i) {
                if let Some(rest) = key.strip_prefix(PENDING_KEY_PREFIX) {
                    if let Ok(hw_id) = Uuid::parse_str(rest) {
                        if let Ok(Some(value)) = storage.get_item(&key) {
                            if let Some(data) = deserialize_draft(&value) {
                                result.push((hw_id, data));
                            }
                        }
                    }
                }
            }
        }
        result
    }

    /// Attempt to resend all pending submissions.
    ///
    /// Spawns an async task per submission. On success the pending entry
    /// is cleared; on failure it remains for the next retry.
    pub fn try_resubmit() {
        let pending = get_pending_submissions();
        for (hw_id, data) in pending {
            wasm_bindgen_futures::spawn_local(async move {
                match crate::browser::submit_homework_async("/api/submit", data).await {
                    Ok(()) => {
                        clear_pending_submission(hw_id);
                        log::info!("pending submission {hw_id} sent");
                    }
                    Err(e) => {
                        log::warn!("resubmit {hw_id} failed: {e:?}");
                    }
                }
            });
        }
    }

    /// Register the Service Worker for offline caching.
    pub fn register_service_worker() {
        let Some(window) = web_sys::window() else {
            return;
        };
        let navigator = window.navigator();
        let promise = navigator.service_worker().register("/sw.js");
        wasm_bindgen_futures::spawn_local(async move {
            match wasm_bindgen_futures::JsFuture::from(promise).await {
                Ok(_) => log::info!("service worker registered"),
                Err(e) => log::warn!("service worker registration failed: {e:?}"),
            }
        });
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  Native stubs (no-op)
// ════════════════════════════════════════════════════════════════════════════

#[cfg(not(target_arch = "wasm32"))]
mod native_impl {
    use uuid::Uuid;

    /// Native stub — no-op.
    pub fn save_draft(_homework_id: Uuid, _data: &[u8]) {}

    /// Native stub — always returns `None`.
    pub fn load_draft(_homework_id: Uuid) -> Option<Vec<u8>> {
        None
    }

    /// Native stub — no-op.
    pub fn add_pending_submission(_homework_id: Uuid, _data: Vec<u8>) {}

    /// Native stub — no-op.
    pub fn clear_pending_submission(_homework_id: Uuid) {}

    /// Native stub — returns empty vector.
    pub fn get_pending_submissions() -> Vec<(Uuid, Vec<u8>)> {
        Vec::new()
    }

    /// Native stub — no-op.
    pub fn try_resubmit() {}

    /// Native stub — no-op.
    pub fn register_service_worker() {}
}

// ════════════════════════════════════════════════════════════════════════════
//  Cross-platform re-exports
// ════════════════════════════════════════════════════════════════════════════

#[cfg(target_arch = "wasm32")]
pub use wasm_impl::{
    add_pending_submission, clear_pending_submission, get_pending_submissions, load_draft,
    register_service_worker, save_draft, try_resubmit,
};

#[cfg(not(target_arch = "wasm32"))]
pub use native_impl::{
    add_pending_submission, clear_pending_submission, get_pending_submissions, load_draft,
    register_service_worker, save_draft, try_resubmit,
};

// ════════════════════════════════════════════════════════════════════════════
//  Unit tests (run on native)
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_draft_key_format() {
        let hw_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let key = draft_key(hw_id);
        assert_eq!(key, "drafftink:draft:550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_pending_key_format() {
        let hw_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let key = pending_key(hw_id);
        assert_eq!(key, "drafftink:pending:550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_serialize_deserialize_draft_roundtrip() {
        let data = vec![0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x00, 0xff];
        let serialized = serialize_draft(&data);
        let deserialized = deserialize_draft(&serialized).unwrap();
        assert_eq!(deserialized, data);
    }

    #[test]
    fn test_serialize_deserialize_empty_draft() {
        let data: Vec<u8> = Vec::new();
        let serialized = serialize_draft(&data);
        assert_eq!(serialized, "");
        let deserialized = deserialize_draft(&serialized).unwrap();
        assert!(deserialized.is_empty());
    }

    #[test]
    fn test_deserialize_invalid_hex() {
        assert!(deserialize_draft("xyz").is_none());
        assert!(deserialize_draft("abc").is_none()); // odd length
    }

    #[test]
    fn test_draft_key_unique_per_uuid() {
        let hw1 = Uuid::new_v4();
        let hw2 = Uuid::new_v4();
        assert_ne!(draft_key(hw1), draft_key(hw2));
    }

    #[test]
    fn test_pending_key_distinct_from_draft_key() {
        let hw_id = Uuid::new_v4();
        assert_ne!(draft_key(hw_id), pending_key(hw_id));
    }

    #[test]
    fn test_native_save_load_draft_stub() {
        let hw_id = Uuid::new_v4();
        save_draft(hw_id, b"test data");
        assert_eq!(load_draft(hw_id), None);
    }

    #[test]
    fn test_native_pending_submissions_stub() {
        let hw_id = Uuid::new_v4();
        add_pending_submission(hw_id, vec![1, 2, 3]);
        assert!(get_pending_submissions().is_empty());
        clear_pending_submission(hw_id); // no-op, should not panic
        try_resubmit(); // no-op
        register_service_worker(); // no-op
    }

    #[test]
    fn test_serialize_large_draft() {
        let data: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();
        let serialized = serialize_draft(&data);
        let deserialized = deserialize_draft(&serialized).unwrap();
        assert_eq!(deserialized, data);
        assert_eq!(serialized.len(), 2000); // 2 hex chars per byte
    }
}
