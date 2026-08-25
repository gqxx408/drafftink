//! Core element traits and the central `ElementData` enum.
//!
//! # Architecture
//!
//! ```text
//!  ┌──────────────┐      ┌──────────────────────┐
//!  │  Element     │      │  SaveInfo            │
//!  │  (trait)     │      │  (trait)             │
//!  └──────┬───────┘      └──────────┬───────────┘
//!         │                         │
//!         │ impl                    │ impl
//!         │                         │
//!  ┌──────▼──────────────────────────▼───────┐
//!  │           ElementData (enum)             │
//!  ├──────────────────────────────────────────┤
//!  │ Shape | Text | Image | Path | SvgShape   │  ← legacy
//!  │ Geometry | Formula | MindMap | Quiz | …  │  ← new
//!  └──────────────────────────────────────────┘
//! ```
//!
//! Adding a new element type requires:
//! 1. Define the data struct (with `base: BaseElement` + specific fields)
//! 2. Add a variant to `ElementData`
//! 3. Call `impl_element_via_base!` for the new struct
//! 4. Add a match arm in `ElementData`'s `Element` impl
//! 5. Implement a renderer for the new type (outside core)
//!
//! Core logic (`BoardContext`, `BoardCommand`, etc.) never matches on
//! `ElementData` variants — it uses the `Element` trait exclusively.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;

use crate::model::{
    BaseElement, ImageElement, PathElement, ShapeElement, SvgShapeElement, TextElement,
};

// ---------------------------------------------------------------------------
// ElementId
// ---------------------------------------------------------------------------

/// Unique identifier for every element on the board.
pub type ElementId = Uuid;

// ---------------------------------------------------------------------------
// Element trait
// ---------------------------------------------------------------------------

/// Common behaviour shared by every element type on the board.
///
/// This trait is **object-safe** — `dyn Element` is usable when needed,
/// though the primary dispatch path is through the `ElementData` enum.
///
/// # Design Rationale
///
/// In the original C# codebase (EasiNote.Extension), element behaviour was
/// scattered across static methods and switch statements.  By centralising
/// the contract in a single trait, the core engine (`BoardContext`,
/// `BoardCommand`) can manipulate elements generically without knowing
/// their concrete type.
pub trait Element: std::fmt::Debug + Send + Sync {
    // ── Identity ────────────────────────────────────────────────────

    /// Stable unique identifier.
    fn id(&self) -> ElementId;

    /// Human-readable type tag (e.g. `"shape"`, `"text"`, `"geometry"`).
    /// Used for serialisation tags and debugging.
    fn element_type(&self) -> &'static str;

    // ── Spatial ─────────────────────────────────────────────────────

    /// World-space position `[x, y]`.
    fn position(&self) -> [f32; 2];

    /// Set the world-space position.
    fn set_position(&mut self, pos: [f32; 2]);

    /// World-space size `[width, height]`.
    fn size(&self) -> [f32; 2];

    /// Set the world-space size.
    fn set_size(&mut self, size: [f32; 2]);

    /// Rotation in radians (clockwise).
    fn rotation(&self) -> f32;

    /// Z-order for depth sorting (higher = on top).
    fn z_order(&self) -> i32;

    /// Set the z-order.
    fn set_z_order(&mut self, z: i32);

    // ── Visibility ──────────────────────────────────────────────────

    /// Whether the element is rendered.
    fn visible(&self) -> bool;

    /// Whether the element is locked (cannot be selected or moved).
    fn locked(&self) -> bool;

    /// Opacity in `[0.0, 1.0]`.
    fn opacity(&self) -> f32;

    // ── Derived helpers (default impls) ────────────────────────────

    /// World-space bounding rect `[left, top, right, bottom]`.
    fn bounds(&self) -> [f32; 4] {
        let [x, y] = self.position();
        let [w, h] = self.size();
        [x, y, x + w, y + h]
    }

    /// Point-in-bounds hit test (default: rectangular).
    /// Override for non-rectangular shapes (e.g. circles).
    fn hit_test(&self, world_pt: [f32; 2]) -> bool {
        let [l, t, r, b] = self.bounds();
        world_pt[0] >= l && world_pt[0] <= r && world_pt[1] >= t && world_pt[1] <= b
    }
}

// ---------------------------------------------------------------------------
// SaveInfo trait
// ---------------------------------------------------------------------------

/// Serialization metadata for persistable element types.
///
/// Every element that can be saved to / loaded from disk must implement
/// this trait.  It provides version information for forward-compatible
/// deserialization and a file-extension hint for the format registry.
///
/// # Example
///
/// ```ignore
/// impl SaveInfo for ShapeElement {
///     fn format_version(&self) -> u32 { 1 }
///     fn file_extension(&self) -> &'static str { "shape" }
///     fn type_name(&self) -> &'static str { "shape" }
/// }
/// ```
pub trait SaveInfo: Serialize + DeserializeOwned {
    /// Schema version of this element's serialization format.
    /// Increment when breaking changes are made to the struct layout.
    fn format_version(&self) -> u32;

    /// File extension (without dot) used when exporting this element
    /// as a standalone file.
    fn file_extension(&self) -> &'static str;

    /// Human-readable type name for display in import/export dialogs.
    fn type_name(&self) -> &'static str;
}

// ---------------------------------------------------------------------------
// New element types (module-specific data structs)
// ---------------------------------------------------------------------------

/// Dynamic geometry element — powered by `drafftink-geometry`.
///
/// Stores opaque geometry definitions (points, lines, circles, constraints)
/// as a JSON blob.  The `drafftink-geometry` crate deserialises this into
/// its own `GeometryDocument` for solving and rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeometryElement {
    pub base: BaseElement,
    /// Opaque JSON payload consumed by `drafftink-geometry`.
    #[serde(default)]
    pub definitions: serde_json::Value,
}

/// Math function plot element — powered by `drafftink-functions`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormulaElement {
    pub base: BaseElement,
    /// Mathematical expression, e.g. `"sin(x) * cos(x)"`.
    pub expression: String,
    /// Line colour `[r, g, b, a]`.
    #[serde(default = "default_plot_color")]
    pub color: [u8; 4],
    /// Line width in world units.
    #[serde(default = "default_plot_width")]
    pub line_width: f32,
    /// X-axis range `[min, max]`.
    #[serde(default = "default_plot_range")]
    pub x_range: [f32; 2],
}

fn default_plot_color() -> [u8; 4] {
    [58, 134, 255, 255]
}

fn default_plot_width() -> f32 {
    2.0
}

fn default_plot_range() -> [f32; 2] {
    [-10.0, 10.0]
}

/// Mind map element — powered by `drafftink-mindmap`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MindMapElement {
    pub base: BaseElement,
    /// Layout algorithm: `"tree"`, `"fishbone"`, `"radial"`.
    #[serde(default = "default_layout")]
    pub layout: String,
    /// Opaque JSON payload consumed by `drafftink-mindmap`.
    #[serde(default)]
    pub nodes: serde_json::Value,
}

fn default_layout() -> String {
    "tree".to_string()
}

/// Quiz / interactive question element — powered by `drafftink-quiz`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuizElement {
    pub base: BaseElement,
    /// Question type tag: `"single_choice"`, `"multiple_choice"`, `"true_false"`.
    pub question_type: String,
    /// Question text (supports plain text).
    pub question: String,
    /// Opaque JSON payload for options, answers, and metadata.
    #[serde(default)]
    pub options: serde_json::Value,
}

/// Solar system / cosmos element — powered by `drafftink-cosmos`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CosmosElement {
    pub base: BaseElement,
    /// Opaque JSON payload consumed by `drafftink-cosmos`.
    #[serde(default)]
    pub bodies: serde_json::Value,
    /// Whether to show orbit lines.
    #[serde(default = "default_show_orbits")]
    pub show_orbits: bool,
}

fn default_show_orbits() -> bool {
    true
}

/// Video element — backed by an embedded media resource (e.g. an `.mkv`/`.mp4`
/// stream stored in the `.enbx` archive under `resources/`).
///
/// Rendering is delegated to a platform video player (Media Foundation on
/// Windows, VideoToolbox on macOS, GStreamer on Linux) once the host UI wires
/// up playback; the element itself only carries the resource reference and
/// presentation metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoElement {
    pub base: BaseElement,
    /// Resource id matching an entry in `EnbxFile.resources` (the embedded
    /// media bytes), e.g. `"5379b730..."`.
    pub resource_id: String,
    /// Loop playback when it reaches the end.
    #[serde(default)]
    pub is_loop: bool,
    /// Begin playback automatically once the player is created.
    #[serde(default)]
    pub is_auto_play: bool,
    /// Playback volume in the range `0.0`–`1.0`.
    #[serde(default = "default_video_volume")]
    pub volume: f64,
    /// Optional poster/thumbnail resource id (resolved like `resource_id`).
    #[serde(default)]
    pub thumbnail_id: Option<String>,
}

fn default_video_volume() -> f64 {
    1.0
}

/// Audio element — a pure-audio clip placed on the canvas (no video track).
///
/// Backed by a local file (`file://<abs-path>`) or an embedded `.enbx` resource id.
/// Playback is delegated to the host (ffmpeg → PCM → cpal, mirroring the video
/// audio pipeline); the element itself only carries the resource reference,
/// presentation metadata, and the probed duration for the control bar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioElement {
    pub base: BaseElement,
    /// Resource id: `file://<absolute-path>` for local files, or an embedded
    /// resource hex id (resolved like `VideoElement::resource_id`).
    pub resource_id: String,
    /// Total duration in milliseconds (probed at insertion time; 0 = unknown).
    #[serde(default)]
    pub duration_ms: u64,
    /// Loop playback when it reaches the end.
    #[serde(default)]
    pub is_loop: bool,
    /// Begin playback automatically once the player is created.
    #[serde(default)]
    pub is_auto_play: bool,
    /// Playback volume in the range `0.0`–`1.0`.
    #[serde(default = "default_audio_volume")]
    pub volume: f64,
}

fn default_audio_volume() -> f64 {
    1.0
}

// ---------------------------------------------------------------------------
// ElementData — central enum
// ---------------------------------------------------------------------------

/// The central element enum that holds every possible element variant.
///
/// # Adding a New Element Type
///
/// 1. Define the data struct in this file (or a sub-module).
/// 2. Add a variant to this enum with `#[serde(rename = "...")]`.
/// 3. Call `impl_element_via_base!(YourStruct)`.
/// 4. Add a match arm in the `Element` impl below.
/// 5. Implement a renderer in your feature crate.
///
/// Core logic (`BoardContext`, `BoardCommand`) **never** matches on
/// `ElementData` variants — it uses the `Element` trait.
///
/// # Serialization
///
/// Uses `#[serde(tag = "type")]` for externally-tagged JSON:
///
/// ```json
/// { "type": "shape", "base": { ... }, "shape_type": "Rectangle" }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ElementData {
    // ── Legacy element types (from model.rs) ───────────────────────
    #[serde(rename = "shape")]
    Shape(ShapeElement),

    #[serde(rename = "text")]
    Text(TextElement),

    #[serde(rename = "image")]
    Image(ImageElement),

    #[serde(rename = "path")]
    Path(PathElement),

    #[serde(rename = "svg_shape")]
    SvgShape(SvgShapeElement),

    // ── New element types ──────────────────────────────────────────
    #[serde(rename = "geometry")]
    Geometry(GeometryElement),

    #[serde(rename = "formula")]
    Formula(FormulaElement),

    #[serde(rename = "mindmap")]
    MindMap(MindMapElement),

    #[serde(rename = "quiz")]
    Quiz(QuizElement),

    #[serde(rename = "cosmos")]
    Cosmos(CosmosElement),

    /// Embedded video element — see [`VideoElement`].
    #[serde(rename = "video")]
    Video(VideoElement),

    /// Pure-audio clip element — see [`AudioElement`].
    #[serde(rename = "audio")]
    Audio(AudioElement),
}

// ---------------------------------------------------------------------------
// Macro: impl_element_via_base!
// ---------------------------------------------------------------------------

/// Declarative macro that implements the `Element` trait for any struct
/// that has a `base: BaseElement` field.
///
/// # Usage
///
/// ```ignore
/// impl_element_via_base!(ShapeElement, "shape");
/// impl_element_via_base!(GeometryElement, "geometry");
/// ```
macro_rules! impl_element_via_base {
    ($type:ty, $type_tag:expr) => {
        impl Element for $type {
            fn id(&self) -> ElementId {
                self.base.id
            }
            fn element_type(&self) -> &'static str {
                $type_tag
            }
            fn position(&self) -> [f32; 2] {
                self.base.position
            }
            fn set_position(&mut self, pos: [f32; 2]) {
                self.base.position = pos;
            }
            fn size(&self) -> [f32; 2] {
                self.base.size
            }
            fn set_size(&mut self, size: [f32; 2]) {
                self.base.size = size;
            }
            fn rotation(&self) -> f32 {
                self.base.rotation
            }
            fn z_order(&self) -> i32 {
                self.base.z_order
            }
            fn set_z_order(&mut self, z: i32) {
                self.base.z_order = z;
            }
            fn visible(&self) -> bool {
                self.base.visible
            }
            fn locked(&self) -> bool {
                self.base.locked
            }
            fn opacity(&self) -> f32 {
                self.base.opacity
            }
        }
    };
}

// Implement Element for all element types
impl_element_via_base!(ShapeElement, "shape");
impl_element_via_base!(TextElement, "text");
impl_element_via_base!(ImageElement, "image");
impl_element_via_base!(PathElement, "path");
impl_element_via_base!(SvgShapeElement, "svg_shape");
impl_element_via_base!(GeometryElement, "geometry");
impl_element_via_base!(FormulaElement, "formula");
impl_element_via_base!(MindMapElement, "mindmap");
impl_element_via_base!(QuizElement, "quiz");
impl_element_via_base!(CosmosElement, "cosmos");
impl_element_via_base!(VideoElement, "video");
impl_element_via_base!(AudioElement, "audio");

// ---------------------------------------------------------------------------
// Element impl for ElementData (enum dispatch)
// ---------------------------------------------------------------------------

impl Element for ElementData {
    fn id(&self) -> ElementId {
        match self {
            ElementData::Shape(e) => e.id(),
            ElementData::Text(e) => e.id(),
            ElementData::Image(e) => e.id(),
            ElementData::Path(e) => e.id(),
            ElementData::SvgShape(e) => e.id(),
            ElementData::Geometry(e) => e.id(),
            ElementData::Formula(e) => e.id(),
            ElementData::MindMap(e) => e.id(),
            ElementData::Quiz(e) => e.id(),
            ElementData::Cosmos(e) => e.id(),
            ElementData::Video(e) => e.id(),
            ElementData::Audio(e) => e.id(),
        }
    }

    fn element_type(&self) -> &'static str {
        match self {
            ElementData::Shape(e) => e.element_type(),
            ElementData::Text(e) => e.element_type(),
            ElementData::Image(e) => e.element_type(),
            ElementData::Path(e) => e.element_type(),
            ElementData::SvgShape(e) => e.element_type(),
            ElementData::Geometry(e) => e.element_type(),
            ElementData::Formula(e) => e.element_type(),
            ElementData::MindMap(e) => e.element_type(),
            ElementData::Quiz(e) => e.element_type(),
            ElementData::Cosmos(e) => e.element_type(),
            ElementData::Video(e) => e.element_type(),
            ElementData::Audio(e) => e.element_type(),
        }
    }

    fn position(&self) -> [f32; 2] {
        match self {
            ElementData::Shape(e) => e.position(),
            ElementData::Text(e) => e.position(),
            ElementData::Image(e) => e.position(),
            ElementData::Path(e) => e.position(),
            ElementData::SvgShape(e) => e.position(),
            ElementData::Geometry(e) => e.position(),
            ElementData::Formula(e) => e.position(),
            ElementData::MindMap(e) => e.position(),
            ElementData::Quiz(e) => e.position(),
            ElementData::Cosmos(e) => e.position(),
            ElementData::Video(e) => e.position(),
            ElementData::Audio(e) => e.position(),
        }
    }

    fn set_position(&mut self, pos: [f32; 2]) {
        match self {
            ElementData::Shape(e) => e.set_position(pos),
            ElementData::Text(e) => e.set_position(pos),
            ElementData::Image(e) => e.set_position(pos),
            ElementData::Path(e) => e.set_position(pos),
            ElementData::SvgShape(e) => e.set_position(pos),
            ElementData::Geometry(e) => e.set_position(pos),
            ElementData::Formula(e) => e.set_position(pos),
            ElementData::MindMap(e) => e.set_position(pos),
            ElementData::Quiz(e) => e.set_position(pos),
            ElementData::Cosmos(e) => e.set_position(pos),
            ElementData::Video(e) => e.set_position(pos),
            ElementData::Audio(e) => e.set_position(pos),
        }
    }

    fn size(&self) -> [f32; 2] {
        match self {
            ElementData::Shape(e) => e.size(),
            ElementData::Text(e) => e.size(),
            ElementData::Image(e) => e.size(),
            ElementData::Path(e) => e.size(),
            ElementData::SvgShape(e) => e.size(),
            ElementData::Geometry(e) => e.size(),
            ElementData::Formula(e) => e.size(),
            ElementData::MindMap(e) => e.size(),
            ElementData::Quiz(e) => e.size(),
            ElementData::Cosmos(e) => e.size(),
            ElementData::Video(e) => e.size(),
            ElementData::Audio(e) => e.size(),
        }
    }

    fn set_size(&mut self, size: [f32; 2]) {
        match self {
            ElementData::Shape(e) => e.set_size(size),
            ElementData::Text(e) => e.set_size(size),
            ElementData::Image(e) => e.set_size(size),
            ElementData::Path(e) => e.set_size(size),
            ElementData::SvgShape(e) => e.set_size(size),
            ElementData::Geometry(e) => e.set_size(size),
            ElementData::Formula(e) => e.set_size(size),
            ElementData::MindMap(e) => e.set_size(size),
            ElementData::Quiz(e) => e.set_size(size),
            ElementData::Cosmos(e) => e.set_size(size),
            ElementData::Video(e) => e.set_size(size),
            ElementData::Audio(e) => e.set_size(size),
        }
    }

    fn rotation(&self) -> f32 {
        match self {
            ElementData::Shape(e) => e.rotation(),
            ElementData::Text(e) => e.rotation(),
            ElementData::Image(e) => e.rotation(),
            ElementData::Path(e) => e.rotation(),
            ElementData::SvgShape(e) => e.rotation(),
            ElementData::Geometry(e) => e.rotation(),
            ElementData::Formula(e) => e.rotation(),
            ElementData::MindMap(e) => e.rotation(),
            ElementData::Quiz(e) => e.rotation(),
            ElementData::Cosmos(e) => e.rotation(),
            ElementData::Video(e) => e.rotation(),
            ElementData::Audio(e) => e.rotation(),
        }
    }

    fn z_order(&self) -> i32 {
        match self {
            ElementData::Shape(e) => e.z_order(),
            ElementData::Text(e) => e.z_order(),
            ElementData::Image(e) => e.z_order(),
            ElementData::Path(e) => e.z_order(),
            ElementData::SvgShape(e) => e.z_order(),
            ElementData::Geometry(e) => e.z_order(),
            ElementData::Formula(e) => e.z_order(),
            ElementData::MindMap(e) => e.z_order(),
            ElementData::Quiz(e) => e.z_order(),
            ElementData::Cosmos(e) => e.z_order(),
            ElementData::Video(e) => e.z_order(),
            ElementData::Audio(e) => e.z_order(),
        }
    }

    fn set_z_order(&mut self, z: i32) {
        match self {
            ElementData::Shape(e) => e.set_z_order(z),
            ElementData::Text(e) => e.set_z_order(z),
            ElementData::Image(e) => e.set_z_order(z),
            ElementData::Path(e) => e.set_z_order(z),
            ElementData::SvgShape(e) => e.set_z_order(z),
            ElementData::Geometry(e) => e.set_z_order(z),
            ElementData::Formula(e) => e.set_z_order(z),
            ElementData::MindMap(e) => e.set_z_order(z),
            ElementData::Quiz(e) => e.set_z_order(z),
            ElementData::Cosmos(e) => e.set_z_order(z),
            ElementData::Video(e) => e.set_z_order(z),
            ElementData::Audio(e) => e.set_z_order(z),
        }
    }

    fn visible(&self) -> bool {
        match self {
            ElementData::Shape(e) => e.visible(),
            ElementData::Text(e) => e.visible(),
            ElementData::Image(e) => e.visible(),
            ElementData::Path(e) => e.visible(),
            ElementData::SvgShape(e) => e.visible(),
            ElementData::Geometry(e) => e.visible(),
            ElementData::Formula(e) => e.visible(),
            ElementData::MindMap(e) => e.visible(),
            ElementData::Quiz(e) => e.visible(),
            ElementData::Cosmos(e) => e.visible(),
            ElementData::Video(e) => e.visible(),
            ElementData::Audio(e) => e.visible(),
        }
    }

    fn locked(&self) -> bool {
        match self {
            ElementData::Shape(e) => e.locked(),
            ElementData::Text(e) => e.locked(),
            ElementData::Image(e) => e.locked(),
            ElementData::Path(e) => e.locked(),
            ElementData::SvgShape(e) => e.locked(),
            ElementData::Geometry(e) => e.locked(),
            ElementData::Formula(e) => e.locked(),
            ElementData::MindMap(e) => e.locked(),
            ElementData::Quiz(e) => e.locked(),
            ElementData::Cosmos(e) => e.locked(),
            ElementData::Video(e) => e.locked(),
            ElementData::Audio(e) => e.locked(),
        }
    }

    fn opacity(&self) -> f32 {
        match self {
            ElementData::Shape(e) => e.opacity(),
            ElementData::Text(e) => e.opacity(),
            ElementData::Image(e) => e.opacity(),
            ElementData::Path(e) => e.opacity(),
            ElementData::SvgShape(e) => e.opacity(),
            ElementData::Geometry(e) => e.opacity(),
            ElementData::Formula(e) => e.opacity(),
            ElementData::MindMap(e) => e.opacity(),
            ElementData::Quiz(e) => e.opacity(),
            ElementData::Cosmos(e) => e.opacity(),
            ElementData::Video(e) => e.opacity(),
            ElementData::Audio(e) => e.opacity(),
        }
    }
}

// ---------------------------------------------------------------------------
// Convenience constructors
// ---------------------------------------------------------------------------

impl ElementData {
    /// Create a new `GeometryElement` with the given base and definitions.
    pub fn geometry(base: BaseElement, definitions: serde_json::Value) -> Self {
        ElementData::Geometry(GeometryElement { base, definitions })
    }

    /// Create a new `FormulaElement` with the given expression.
    pub fn formula(base: BaseElement, expression: impl Into<String>) -> Self {
        ElementData::Formula(FormulaElement {
            base,
            expression: expression.into(),
            color: default_plot_color(),
            line_width: default_plot_width(),
            x_range: default_plot_range(),
        })
    }

    /// Create a new `MindMapElement` with the given layout.
    pub fn mindmap(base: BaseElement, layout: impl Into<String>, nodes: serde_json::Value) -> Self {
        ElementData::MindMap(MindMapElement {
            base,
            layout: layout.into(),
            nodes,
        })
    }

    /// Create a new `QuizElement`.
    pub fn quiz(
        base: BaseElement,
        question_type: impl Into<String>,
        question: impl Into<String>,
    ) -> Self {
        ElementData::Quiz(QuizElement {
            base,
            question_type: question_type.into(),
            question: question.into(),
            options: serde_json::Value::Null,
        })
    }

    /// Create a new `CosmosElement`.
    pub fn cosmos(base: BaseElement, bodies: serde_json::Value) -> Self {
        ElementData::Cosmos(CosmosElement {
            base,
            bodies,
            show_orbits: true,
        })
    }

    /// Create a new `VideoElement`.
    pub fn video(
        base: BaseElement,
        resource_id: impl Into<String>,
        is_loop: bool,
        is_auto_play: bool,
        volume: f64,
        thumbnail_id: Option<String>,
    ) -> Self {
        ElementData::Video(VideoElement {
            base,
            resource_id: resource_id.into(),
            is_loop,
            is_auto_play,
            volume,
            thumbnail_id,
        })
    }

    /// Create a new `AudioElement` (pure-audio clip).
    pub fn audio(base: BaseElement, resource_id: impl Into<String>, duration_ms: u64) -> Self {
        ElementData::Audio(AudioElement {
            base,
            resource_id: resource_id.into(),
            duration_ms,
            is_loop: false,
            is_auto_play: false,
            volume: 1.0,
        })
    }

    /// Convert from a legacy `model::Element` into `ElementData`.
    ///
    /// This is the bridge for migrating existing code to the new architecture.
    pub fn from_legacy(element: crate::model::Element) -> Self {
        match element {
            crate::model::Element::Shape(e) => ElementData::Shape(e),
            crate::model::Element::Text(e) => ElementData::Text(e),
            crate::model::Element::Image(e) => ElementData::Image(e),
            crate::model::Element::Path(e) => ElementData::Path(e),
            crate::model::Element::SvgShape(e) => ElementData::SvgShape(e),
        }
    }

    /// Convert back to a legacy `model::Element` if possible.
    ///
    /// Returns `None` for new element types that have no legacy equivalent.
    pub fn to_legacy(&self) -> Option<crate::model::Element> {
        match self {
            ElementData::Shape(e) => Some(crate::model::Element::Shape(e.clone())),
            ElementData::Text(e) => Some(crate::model::Element::Text(e.clone())),
            ElementData::Image(e) => Some(crate::model::Element::Image(e.clone())),
            ElementData::Path(e) => Some(crate::model::Element::Path(e.clone())),
            ElementData::SvgShape(e) => Some(crate::model::Element::SvgShape(e.clone())),
            ElementData::Geometry(_)
            | ElementData::Formula(_)
            | ElementData::MindMap(_)
            | ElementData::Quiz(_)
            | ElementData::Cosmos(_)
            | ElementData::Video(_)
            | ElementData::Audio(_) => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_base() -> BaseElement {
        BaseElement {
            id: Uuid::new_v4(),
            position: [100.0, 200.0],
            size: [300.0, 150.0],
            ..Default::default()
        }
    }

    #[test]
    fn element_data_shape_trait_methods() {
        let base = make_base();
        let id = base.id;
        let data = ElementData::Shape(ShapeElement {
            base,
            shape_type: crate::model::ShapeType::Rectangle,
            has_start_arrow: false,
            has_end_arrow: false,
            scale_y: 0.0,
        });

        assert_eq!(data.id(), id);
        assert_eq!(data.element_type(), "shape");
        assert_eq!(data.position(), [100.0, 200.0]);
        assert_eq!(data.size(), [300.0, 150.0]);
        assert_eq!(data.bounds(), [100.0, 200.0, 400.0, 350.0]);
    }

    #[test]
    fn element_data_geometry_trait_methods() {
        let base = make_base();
        let id = base.id;
        let data = ElementData::geometry(base, serde_json::json!({"points": []}));

        assert_eq!(data.id(), id);
        assert_eq!(data.element_type(), "geometry");
        assert_eq!(data.position(), [100.0, 200.0]);
    }

    #[test]
    fn element_data_formula_constructor() {
        let base = make_base();
        let data = ElementData::formula(base, "sin(x)");

        match &data {
            ElementData::Formula(f) => {
                assert_eq!(f.expression, "sin(x)");
                assert_eq!(f.line_width, 2.0);
                assert_eq!(f.x_range, [-10.0, 10.0]);
            }
            _ => panic!("expected Formula variant"),
        }
    }

    #[test]
    fn element_data_set_position() {
        let base = make_base();
        let mut data = ElementData::geometry(base, serde_json::json!({}));

        data.set_position([500.0, 600.0]);
        assert_eq!(data.position(), [500.0, 600.0]);
    }

    #[test]
    fn element_data_set_size() {
        let base = make_base();
        let mut data = ElementData::formula(base, "x^2");

        data.set_size([800.0, 600.0]);
        assert_eq!(data.size(), [800.0, 600.0]);
        assert_eq!(data.bounds(), [100.0, 200.0, 900.0, 800.0]);
    }

    #[test]
    fn element_data_set_z_order() {
        let base = make_base();
        let mut data = ElementData::quiz(base, "single_choice", "What is 2+2?");

        data.set_z_order(42);
        assert_eq!(data.z_order(), 42);
    }

    #[test]
    fn element_data_hit_test() {
        let base = make_base();
        let data = ElementData::geometry(base, serde_json::json!({}));

        // Inside bounds
        assert!(data.hit_test([200.0, 250.0]));
        // Outside bounds
        assert!(!data.hit_test([50.0, 50.0]));
    }

    #[test]
    fn element_data_serialization_roundtrip() {
        let base = make_base();
        let data = ElementData::formula(base, "cos(x)");

        let json = serde_json::to_string(&data).expect("serialize");
        let decoded: ElementData = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(decoded.id(), data.id());
        assert_eq!(decoded.element_type(), "formula");
        match decoded {
            ElementData::Formula(f) => assert_eq!(f.expression, "cos(x)"),
            _ => panic!("expected Formula"),
        }
    }

    #[test]
    fn element_data_serialization_tag() {
        let base = make_base();
        let data = ElementData::geometry(base, serde_json::json!({}));
        let json = serde_json::to_string(&data).expect("serialize");
        assert!(json.contains(r#""type":"geometry""#));
    }

    #[test]
    fn legacy_roundtrip() {
        let base = make_base();
        let id = base.id;
        let legacy = crate::model::Element::Shape(ShapeElement {
            base,
            shape_type: crate::model::ShapeType::Ellipse,
            has_start_arrow: false,
            has_end_arrow: false,
            scale_y: 0.0,
        });

        let data = ElementData::from_legacy(legacy);
        assert_eq!(data.id(), id);
        assert_eq!(data.element_type(), "shape");

        let back = data.to_legacy();
        assert!(back.is_some());
        assert_eq!(back.unwrap().id(), id);
    }

    #[test]
    fn legacy_roundtrip_none_for_new_types() {
        let base = make_base();
        let data = ElementData::mindmap(base, "radial", serde_json::json!({}));
        assert!(data.to_legacy().is_none());
    }

    #[test]
    fn all_variants_implement_element() {
        let base = make_base();
        let variants: Vec<ElementData> = vec![
            ElementData::Shape(ShapeElement {
                base: base.clone(),
                shape_type: crate::model::ShapeType::Rectangle,
                has_start_arrow: false,
                has_end_arrow: false,
                scale_y: 0.0,
            }),
            ElementData::Text(TextElement {
                base: base.clone(),
                text: "hello".into(),
                font_size: 24.0,
                font_family: "sans".into(),
            }),
            ElementData::Image(ImageElement {
                base: base.clone(),
                image_path: "/tmp/x.png".into(),
                image_data: None,
                keep_aspect: true,
            }),
            ElementData::Path(PathElement {
                base: base.clone(),
                points: vec![[0.0, 0.0], [10.0, 10.0]],
                is_closed: false,
            }),
            ElementData::SvgShape(SvgShapeElement {
                base: base.clone(),
                svg_path: "M0,0 L10,10".into(),
                is_closed: false,
                has_end_arrow: false,
                has_start_arrow: false,
            }),
            ElementData::geometry(base.clone(), serde_json::json!({})),
            ElementData::formula(base.clone(), "x"),
            ElementData::mindmap(base.clone(), "tree", serde_json::json!({})),
            ElementData::quiz(base.clone(), "true_false", "Yes?"),
            ElementData::cosmos(base.clone(), serde_json::json!({})),
            ElementData::video(base.clone(), "file:///tmp/a.mp4", false, true, 1.0, None),
            ElementData::audio(base, "file:///tmp/a.mp3", 0),
        ];

        for v in &variants {
            // Every variant must return a non-empty type tag
            assert!(!v.element_type().is_empty());
            // Every variant must have valid bounds
            let [l, t, r, b] = v.bounds();
            assert!(r >= l && b >= t, "invalid bounds for {}", v.element_type());
        }
    }
}
