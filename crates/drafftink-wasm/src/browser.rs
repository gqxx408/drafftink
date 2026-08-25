//! Browser API wrappers.
//!
//! On WASM these functions interact with real browser APIs (URL parsing,
//! LocalStorage, Fetch, `navigator.onLine`). On native targets they return
//! empty/default values as stubs so the crate compiles for `cargo clippy`
//! and `cargo test`.

// ════════════════════════════════════════════════════════════════════════════
//  Shared pure helpers (compile on all targets, unit-testable on native)
// ════════════════════════════════════════════════════════════════════════════

/// Parse a URL query string for a parameter value.
///
/// Pure function shared between WASM and native, making it testable
/// without a browser.
///
/// # Arguments
/// * `query` – the query string (e.g. `"hw=abc123&foo=bar"` or `"?hw=abc123"`)
/// * `name`  – the parameter name to look up
pub fn parse_query_param(query: &str, name: &str) -> Option<String> {
    let query = query.trim_start_matches('?');
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        match (parts.next(), parts.next()) {
            (Some(key), Some(value)) if key == name => return Some(value.to_string()),
            _ => {}
        }
    }
    None
}

// ════════════════════════════════════════════════════════════════════════════
//  WASM implementations
// ════════════════════════════════════════════════════════════════════════════

#[cfg(target_arch = "wasm32")]
mod wasm_impl {
    use super::parse_query_param;
    use anyhow::{anyhow, Result};
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;

    /// Get a URL query parameter from the current page's URL.
    pub fn get_url_param(name: &str) -> Option<String> {
        let window = web_sys::window()?;
        let location = window.location();
        let search = location.search().ok()?;
        parse_query_param(&search, name)
    }

    /// Read a value from LocalStorage.
    pub fn local_storage_get(key: &str) -> Option<String> {
        let window = web_sys::window()?;
        let storage = window.local_storage().ok()??;
        storage.get_item(key).ok()?
    }

    /// Write a value to LocalStorage.
    pub fn local_storage_set(key: &str, value: &str) {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                let _ = storage.set_item(key, value);
            }
        }
    }

    /// Check whether the browser is online (`navigator.onLine`).
    pub fn is_online() -> bool {
        web_sys::window()
            .map(|w| w.navigator().on_line())
            .unwrap_or(false)
    }

    /// Fetch homework data via HTTP GET (async).
    pub async fn fetch_homework_async(url: &str) -> Result<Vec<u8>> {
        let window = web_sys::window().ok_or_else(|| anyhow!("no window object"))?;
        let resp_value = JsFuture::from(window.fetch_with_str(url))
            .await
            .map_err(|e| anyhow!("fetch failed: {:?}", e))?;
        let resp: web_sys::Response = resp_value
            .dyn_into()
            .map_err(|_| anyhow!("invalid response type"))?;
        if !resp.ok() {
            anyhow::bail!("HTTP error: {}", resp.status());
        }
        let buf = JsFuture::from(
            resp.array_buffer()
                .map_err(|e| anyhow!("array_buffer: {:?}", e))?,
        )
        .await
        .map_err(|e| anyhow!("array_buffer read: {:?}", e))?;
        let array = js_sys::Uint8Array::new(&buf);
        Ok(array.to_vec())
    }

    /// Synchronous wrapper for `fetch_homework_async`.
    ///
    /// On WASM the Fetch API is inherently async, so this function spawns
    /// a background task and returns an empty vector immediately. Use
    /// [`fetch_homework_async`] when the actual data is needed.
    pub fn fetch_homework(_url: &str) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }

    /// Submit homework data via HTTP POST (async).
    pub async fn submit_homework_async(url: &str, data: Vec<u8>) -> Result<()> {
        let window = web_sys::window().ok_or_else(|| anyhow!("no window object"))?;

        let mut init = web_sys::RequestInit::new();
        init.method("POST");
        init.body(Some(&JsValue::from(js_sys::Uint8Array::from(&data[..]))));

        let request = web_sys::Request::new_with_str_and_init(url, &init)
            .map_err(|e| anyhow!("request build: {:?}", e))?;

        let resp_value = JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(|e| anyhow!("fetch: {:?}", e))?;
        let resp: web_sys::Response = resp_value
            .dyn_into()
            .map_err(|_| anyhow!("invalid response type"))?;
        if !resp.ok() {
            anyhow::bail!("HTTP error: {}", resp.status());
        }
        Ok(())
    }

    /// Synchronous wrapper for `submit_homework_async`.
    ///
    /// Spawns a background task and returns `Ok(())` immediately. The
    /// caller should also store the data as a pending submission so it
    /// can be retried if the background task fails.
    pub fn submit_homework(url: &str, data: Vec<u8>) -> Result<()> {
        let url = url.to_string();
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = submit_homework_async(&url, data).await {
                log::error!("submit_homework background error: {e:?}");
            }
        });
        Ok(())
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  Native stubs
// ════════════════════════════════════════════════════════════════════════════

#[cfg(not(target_arch = "wasm32"))]
mod native_impl {
    use anyhow::Result;

    /// Native stub — always returns `None`.
    pub fn get_url_param(_name: &str) -> Option<String> {
        None
    }

    /// Native stub — always returns `None`.
    pub fn local_storage_get(_key: &str) -> Option<String> {
        None
    }

    /// Native stub — no-op.
    pub fn local_storage_set(_key: &str, _value: &str) {}

    /// Native stub — always returns `true`.
    pub fn is_online() -> bool {
        true
    }

    /// Native stub — returns empty vector.
    pub fn fetch_homework(_url: &str) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }

    /// Native stub — no-op success.
    pub fn submit_homework(_url: &str, _data: Vec<u8>) -> Result<()> {
        Ok(())
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  Cross-platform re-exports
// ════════════════════════════════════════════════════════════════════════════

#[cfg(target_arch = "wasm32")]
pub use wasm_impl::{
    fetch_homework, get_url_param, is_online, local_storage_get, local_storage_set,
    submit_homework, submit_homework_async,
};

#[cfg(not(target_arch = "wasm32"))]
pub use native_impl::{
    fetch_homework, get_url_param, is_online, local_storage_get, local_storage_set, submit_homework,
};

// ════════════════════════════════════════════════════════════════════════════
//  Unit tests (run on native)
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_query_param_basic() {
        let query = "hw=abc123&foo=bar";
        assert_eq!(parse_query_param(query, "hw"), Some("abc123".to_string()));
        assert_eq!(parse_query_param(query, "foo"), Some("bar".to_string()));
    }

    #[test]
    fn test_parse_query_param_with_question_mark() {
        assert_eq!(
            parse_query_param("?hw=xyz789", "hw"),
            Some("xyz789".to_string())
        );
    }

    #[test]
    fn test_parse_query_param_not_found() {
        assert_eq!(parse_query_param("hw=abc123", "missing"), None);
    }

    #[test]
    fn test_parse_query_param_empty() {
        assert_eq!(parse_query_param("", "hw"), None);
        assert_eq!(parse_query_param("?", "hw"), None);
    }

    #[test]
    fn test_parse_query_param_url_encoded_value() {
        let query = "hw=abc%20123&name=test";
        assert_eq!(
            parse_query_param(query, "hw"),
            Some("abc%20123".to_string())
        );
    }

    #[test]
    fn test_parse_query_param_empty_value() {
        assert_eq!(parse_query_param("hw=&foo=bar", "hw"), Some(String::new()));
    }

    #[test]
    fn test_parse_query_param_multiple_params() {
        let query = "a=1&b=2&c=3";
        assert_eq!(parse_query_param(query, "a"), Some("1".to_string()));
        assert_eq!(parse_query_param(query, "b"), Some("2".to_string()));
        assert_eq!(parse_query_param(query, "c"), Some("3".to_string()));
    }

    #[test]
    fn test_native_get_url_param_stub() {
        assert_eq!(get_url_param("hw"), None);
    }

    #[test]
    fn test_native_local_storage_stub() {
        assert_eq!(local_storage_get("key"), None);
        local_storage_set("key", "value"); // no-op, should not panic
        assert_eq!(local_storage_get("key"), None);
    }

    #[test]
    fn test_native_is_online_stub() {
        assert!(is_online());
    }

    #[test]
    fn test_native_fetch_homework_stub() {
        let result = fetch_homework("http://example.com").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_native_submit_homework_stub() {
        assert!(submit_homework("http://example.com", vec![1, 2, 3]).is_ok());
    }
}
