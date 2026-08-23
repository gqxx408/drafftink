//! Binary entry point.
//!
//! On WASM (`wasm32-unknown-unknown`) this file exports a `start` function
//! that initialises the eframe web runner and launches [`WasmApp`].
//!
//! On native targets a stub `main` is provided so that `cargo clippy` and
//! `cargo test` work without a WASM toolchain.

// ════════════════════════════════════════════════════════════════════════════
//  WASM entry point
// ════════════════════════════════════════════════════════════════════════════

#[cfg(target_arch = "wasm32")]
mod wasm_entry {
    use wasm_bindgen::prelude::*;

    /// Called from JavaScript to start the eframe web app.
    ///
    /// Looks up the `<canvas id="canvas">` element in the DOM, creates a
    /// [`eframe::WebRunner`], and installs [`WasmApp`](drafftink_wasm::WasmApp).
    #[wasm_bindgen]
    pub async fn start() -> Result<(), JsValue> {
        // Redirect log messages to console
        eframe::WebLogger::init(log::LevelFilter::Debug).ok();

        let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
        let document = window
            .document()
            .ok_or_else(|| JsValue::from_str("no document"))?;

        let canvas = document
            .get_element_by_id("canvas")
            .ok_or_else(|| JsValue::from_str("canvas element not found"))?;
        let canvas: web_sys::HtmlCanvasElement = canvas
            .dyn_into()
            .map_err(|_| JsValue::from_str("element is not a canvas"))?;

        // Register the service worker for offline support
        drafftink_wasm::offline::register_service_worker();

        // Start eframe
        eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(drafftink_wasm::WasmApp::new(cc)))),
            )
            .await
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  main()
// ════════════════════════════════════════════════════════════════════════════

#[cfg(target_arch = "wasm32")]
fn main() {
    // On WASM the entry point is the `start` function exported above,
    // which is called from JavaScript after the module is loaded.
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!(
        "drafftink-wasm is a WASM-only binary.\n\
         Build with: cargo build --target wasm32-unknown-unknown"
    );
}
