//! # drafftink-wasm
//!
//! Student-facing WASM client for the drafftink homework system.
//!
//! Runs in the browser via egui-web (wasm-bindgen), enabling zero-install
//! homework editing with offline support. All core logic — crypto, drftx
//! format, business logic — is delegated to [`drafftink_core`]; no JS
//! reimplementation.
//!
//! ## Target
//!
//! Primary target: `wasm32-unknown-unknown`.
//!
//! The crate also compiles on native targets so that `cargo clippy` and
//! `cargo test` can be run without a WASM toolchain. WASM-specific code is
//! guarded by `#[cfg(target_arch = "wasm32")]`; native stubs use
//! `#[cfg(not(target_arch = "wasm32"))]`.

pub mod app;
pub mod browser;
pub mod crypto;
pub mod offline;
pub mod ui;

pub use app::{AnswerPayload, WasmApp};
