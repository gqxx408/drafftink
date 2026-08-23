//! # drafftink-enbx
//!
//! Seewo EasiNote `.enbx` format compatibility module.
//!
//! Handles parsing and generation of `.enbx` files, enabling both importing
//! Seewo courseware and exporting to the Seewo format.  The implementation is
//! pure Rust with no C dependencies.
//!
//! ## Module Layout
//!
//! | Module      | Responsibility                                        |
//! |-------------|-------------------------------------------------------|
//! | `parser`    | `.enbx` file parsing (zip + XML)                      |
//! | `generator` | drftx → enbx generation                               |
//! | `mapper`    | Element mapping between internal and Seewo formats    |
//!
//! ## .enbx File Structure
//!
//! An `.enbx` file is a ZIP archive containing:
//! - `Reference.xml` — resource-id → filename mappings
//! - `Slide_1.xml`, `Slide_2.xml`, … — per-slide content
//! - Resource files (images, audio, etc.)

pub mod generator;
pub mod mapper;
pub mod parser;

// Re-export key public types and functions
pub use generator::generate_enbx;
pub use mapper::{
    argb_hex_to_color32, color32_to_argb_hex, flip_y, map_element_from_enbx, map_element_to_enbx,
};
pub use parser::{
    parse_enbx, EnbxElement, EnbxFile, EnbxGroup, EnbxImage, EnbxMetadata, EnbxPath, EnbxShape,
    EnbxSlide, EnbxText, XmlValue,
};
