//! Proc-macro for `#[export_plugin]` — generates the FFI entry point
//! that the host calls via `libloading` to instantiate a plugin.
//!
//! Usage:
//! ```ignore
//! #[export_plugin]
//! impl DrafftinkPlugin for MathModule {
//!     fn name(&self) -> &'static str { "MathModule" }
//!     fn version(&self) -> &'static str { "0.1.0" }
//!     fn initialize(&mut self, ctx: &mut PluginContext) { ... }
//!     fn shutdown(&mut self) { ... }
//! }
//! ```
//!
//! This generates `pub extern "C" fn create_plugin() -> *mut dyn DrafftinkPlugin`.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemImpl};

/// Attribute macro that wraps an `impl DrafftinkPlugin for T` block
/// and additionally generates a `create_plugin` FFI entry point.
///
/// The target type MUST implement `Default` so the entry point can
/// construct it with `T::default()`.
#[proc_macro_attribute]
pub fn export_plugin(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemImpl);

    // Extract the type being implemented, e.g. `MathModule`
    let self_ty = &input.self_ty;

    let expanded = quote! {
        #input

        /// FFI entry point — called by the host via `libloading`.
        /// Returns a heap-allocated trait object; the host is responsible
        /// for calling `Box::from_raw` and eventually dropping it.
        #[no_mangle]
        pub extern "C" fn create_plugin() -> *mut dyn ::drafftink_core::plugin::DrafftinkPlugin {
            let plugin: Box<dyn ::drafftink_core::plugin::DrafftinkPlugin> =
                Box::new(<#self_ty>::default());
            Box::into_raw(plugin)
        }
    };

    TokenStream::from(expanded)
}