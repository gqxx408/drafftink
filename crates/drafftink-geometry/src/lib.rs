//! drafftink-geometry — Definition-driven dynamic geometry engine
//!
//! Core principle: geometric elements store *mathematical definitions* and
//! *dependency references*, not direct coordinates. A solver resolves the
//! dependency graph in topological order, producing concrete point positions.
//!
//! # Architecture
//! - [`definitions`] — pure data structs (PointDef, LineDef, CircleDef, …)
//! - [`solver`] — topological-sort solver with dirty-state incremental updates
//! - [`mesh`] — lyon-based tessellation into vertex/index buffers
//! - [`renderer`] — egui Painter GPU-accelerated rendering
//! - [`primitives3d`] — 3D primitives (cube, sphere) + camera projection
//! - [`seewo_import`] — Seewo EasiNote XML parsing and 3D object import
//! - [`solar_system`] — SeewoSolarSystemLoader (geography slides with textures)
//! - [`solar_system_viewer`] — full-screen viewer for EasiNote geography modules
//! - [`viewer`] — self-contained egui widget with toolbar + canvas interaction
//! - [`persistence`] — JSON serialization

pub mod definitions;
pub mod mesh;
pub mod persistence;
pub mod primitives3d;
pub mod renderer;
pub mod seewo_import;
pub mod solar_system;
pub mod solar_system_viewer;
pub mod solver;
pub mod viewer;

pub use solar_system_viewer::SolarSystemViewer;
pub use viewer::GeometryViewer;
