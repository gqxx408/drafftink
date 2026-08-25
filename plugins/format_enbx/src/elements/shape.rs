//! Shape-element data structures for .enbx parsing.

use super::text::ArgbColor;

/// Geometry type extracted from Seewo Slide XML.
#[derive(Debug, Clone, PartialEq)]
pub enum GeometryKind {
    /// Straight line, no arrows.
    Line,
    /// Straight line with arrow at end.
    LineArrowEnd,
    /// Straight line with arrow at start.
    LineArrowStart,
    /// Straight line with arrows at both ends.
    LineArrowStartEnd,
    /// Freehand curve (SVG Path), may have arrow at tail.
    FreeLine,
    /// Rectangle (filled/outlined).
    Rectangle,
    /// Ellipse.
    Ellipse,
    /// Circle (Seewo uses "Circle" type, rendered via SVG Path like Ellipse).
    Circle,
    /// Square bracket [
    Bracket,
    /// Curly brace {
    Brace,
    /// Circular sector / fan shape (filled wedge with SVG Path).
    Fan,
    /// Other/unknown geometry (string preserved).
    Other(String),
}

/// A parsed shape element from a Seewo Slide XML.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeElementData {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,
    pub is_locked: bool,
    /// Stroke color (Foreground in Seewo XML).
    pub stroke_color: ArgbColor,
    /// Fill color (Background in Seewo XML).
    pub fill_color: ArgbColor,
    /// Stroke width (Thickness).
    pub thickness: f32,
    /// Geometry type.
    pub geometry: GeometryKind,
    /// Raw SVG path string for FreeLine / CustomGeometry shapes.
    pub svg_path: String,
    /// ScaleY parameter from Adjusts (used by Brace curvature).
    pub scale_y: f32,
    /// Arrow at start (HeadEnd).
    pub has_start_arrow: bool,
    /// Arrow at end (TailEnd).
    pub has_end_arrow: bool,
}

impl Default for ShapeElementData {
    fn default() -> Self {
        Self {
            id: String::new(),
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            rotation: 0.0,
            is_locked: false,
            stroke_color: ArgbColor {
                a: 255,
                r: 0,
                g: 0,
                b: 0,
            },
            fill_color: ArgbColor {
                a: 0,
                r: 255,
                g: 255,
                b: 255,
            },
            thickness: 2.0,
            geometry: GeometryKind::Line,
            svg_path: String::new(),
            scale_y: 0.0,
            has_start_arrow: false,
            has_end_arrow: false,
        }
    }
}

/// A parsed element from a Seewo Slide XML — either Text or Shape.
#[derive(Debug, Clone, PartialEq)]
pub enum SlideElement {
    Text(super::text::TextElement),
    Shape(ShapeElementData),
}
