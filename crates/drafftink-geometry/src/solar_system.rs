//! SeewoSolarSystemLoader — Parse and render EasiNote 5's geography modules.
//!
//! This module implements a complete pipeline for loading Seewo EasiNote's
//! SolarSystem geography slides from `.enbx` files (OPC-like zip archives)
//! and rendering them with enhanced visuals (textured IcoSphere + Lambert
//! lighting + atmosphere scattering + multi-layer blending).
//!
//! # Architecture
//! 1. **Package Parser** — Opens `.enbx` as zip, parses `Reference.xml` and
//!    `Slide_1.xml` to extract the SolarSystem scene and resource mappings.
//! 2. **Scene Deserializer** — Converts XML into strongly-typed Rust structs.
//! 3. **Resource Loader** — Resolves `id://` URIs to image bytes in the zip,
//!    decodes them via the `image` crate, and uploads to `egui::TextureHandle`.
//! 4. **Renderer** — Generates an IcoSphere with UV coordinates, projects to
//!    2D screen space (CPU-side), and builds `egui::Mesh` with texture UVs
//!    for GPU-accelerated textured rendering via egui's wgpu backend.
//!
//! # Rendering Approach
//! Since the project mandates `egui::Painter` for all rendering, we use
//! egui's built-in texture support (`egui::TextureHandle` + `egui::Mesh`
//! with UV coordinates) instead of a custom wgpu pipeline. The accompanying
//! `shaders/solar_system.wgsl` file documents the ideal shader for future
//! migration to a direct wgpu render pass.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use anyhow::{anyhow, Result};
use egui::{Color32, ColorImage, Mesh, Painter, Pos2, TextureHandle, TextureOptions};
use nalgebra::{Matrix3, Point3, Rotation3, UnitQuaternion, Vector3};
use quick_xml::events::Event;
use quick_xml::Reader;

use crate::primitives3d::{Camera3D, ProjectionMode};

/// Projected vertex: (optional screen coords with depth, original 3D position on unit sphere)
type ProjectedVerts = Vec<(Option<(f32, f32, f32)>, [f32; 3])>;

// ════════════════════════════════════════════════════════════════════════════
//  DATA STRUCTURES
// ════════════════════════════════════════════════════════════════════════════

/// Camera parameters extracted from EasiNote XML.
#[derive(Debug, Clone)]
pub struct CameraParams {
    /// Camera world-space position.
    pub position: [f32; 3],
    /// Direction the camera is looking (from eye toward target).
    pub look_direction: [f32; 3],
    /// Camera up vector.
    pub up_direction: [f32; 3],
}

/// A single texture layer in the SolarSystem element.
#[derive(Debug, Clone)]
pub struct TextureLayer {
    /// Texture key, e.g. "Satellite", "Rainfall", "Temperature", "Population".
    pub key: String,
    /// Resource ID from `id://` URI (hex hash).
    pub resource_id: String,
    /// Opacity from XML (typically 1.0).
    pub opacity: f32,
}

/// Parsed SolarSystem scene from `Slide_1.xml`.
#[derive(Debug, Clone)]
pub struct SolarSystemScene {
    /// Planet type, e.g. "Earth".
    pub planet_type: String,
    /// Currently selected texture type, e.g. "Satellite".
    pub texture_type: String,
    /// Currently selected texture key.
    pub texture_key: String,
    /// View mode, e.g. "ThreeD".
    pub planet_view: String,
    /// State, e.g. "Overall".
    pub planet_state: String,
    /// Camera parameters.
    pub camera: CameraParams,
    /// 2×3 texture transform matrix (usually identity).
    pub texture_matrix: [f32; 6],
    /// All available texture layers.
    pub textures: Vec<TextureLayer>,
    /// Screen-space position and size.
    pub screen_x: f32,
    pub screen_y: f32,
    pub screen_width: f32,
    pub screen_height: f32,
}

/// Mapping from resource ID to zip-internal path (from `Reference.xml`).
pub struct ResourceMap {
    pub id_to_path: HashMap<String, String>,
}

/// A decoded texture ready for GPU upload.
pub struct LoadedTexture {
    pub key: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

// ════════════════════════════════════════════════════════════════════════════
//  PACKAGE PARSER
// ════════════════════════════════════════════════════════════════════════════

/// Load a SolarSystem scene and all its textures from a `.enbx` file.
///
/// # Arguments
/// * `path` — Path to the `.enbx` file (a zip archive).
///
/// # Returns
/// A tuple of (scene, loaded_textures). Returns `Ok(None)` if the file
/// does not contain a SolarSystem element.
pub fn load_enbx_solar_system(path: &Path) -> Result<Option<(SolarSystemScene, Vec<LoadedTexture>)>> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    // 1. Parse Reference.xml
    let reference_xml = read_zip_file(&mut archive, "Reference.xml")?;
    let resource_map = parse_reference_xml(&reference_xml)?;

    // 2. Find and parse Slide_*.xml files
    let mut scene: Option<SolarSystemScene> = None;
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        if name.starts_with("Slide_") && name.ends_with(".xml") {
            drop(entry);
            let slide_xml = read_zip_file(&mut archive, &name)?;
            if let Some(s) = parse_slide_xml(&slide_xml)? {
                scene = Some(s);
                break;
            }
        }
    }

    let scene = match scene {
        Some(s) => s,
        None => return Ok(None),
    };

    // 3. Load all textures referenced by the scene
    let mut loaded_textures = Vec::new();
    for layer in &scene.textures {
        if let Some(zip_path) = resource_map.id_to_path.get(&layer.resource_id) {
            match load_texture_from_zip(&mut archive, zip_path, &layer.key) {
                Ok(tex) => loaded_textures.push(tex),
                Err(e) => {
                    log::warn!("Failed to load texture '{}': {}", layer.key, e);
                }
            }
        } else {
            log::warn!(
                "Resource ID '{}' not found in Reference.xml for texture '{}'",
                layer.resource_id,
                layer.key
            );
        }
    }

    Ok(Some((scene, loaded_textures)))
}

/// Read a file from the zip archive as a UTF-8 string.
fn read_zip_archive<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<String> {
    let mut entry = archive.by_name(name)?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)?;
    // Strip BOM if present
    if buf.starts_with(&[0xEF, 0xBB, 0xBF]) {
        buf.drain(..3);
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Read a file from a `ZipArchive<File>` as a UTF-8 string.
fn read_zip_file(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Result<String> {
    read_zip_archive(archive, name)
}

/// Parse `Reference.xml` to build a resource ID → zip path mapping.
///
/// Supports two formats:
/// - OPC attributes: `<Relationship Id="abc" Target="path" />`
/// - Child elements: `<Relationship><Id>abc</Id><Target>path</Target></Relationship>`
fn parse_reference_xml(xml: &str) -> Result<ResourceMap> {
    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);

    let mut id_to_path = HashMap::new();
    let mut buf = Vec::new();

    // State for child-element-based parsing
    let mut in_relationship = false;
    let mut rel_id: Option<String> = None;
    let mut rel_target: Option<String> = None;
    let mut child_tag: Option<String> = None;
    let mut text_buf = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "Relationship" {
                    in_relationship = true;
                    rel_id = None;
                    rel_target = None;

                    // Also try attributes (OPC format)
                    for attr in e.attributes().flatten() {
                        match attr.key.as_ref() {
                            b"Id" => {
                                rel_id = Some(attr.unescape_value()?.to_string());
                            }
                            b"Target" => {
                                rel_target = Some(attr.unescape_value()?.to_string());
                            }
                            _ => {}
                        }
                    }
                }
                if in_relationship {
                    child_tag = Some(name);
                }
                text_buf.clear();
            }
            Ok(Event::Empty(e)) if e.name().as_ref() == b"Relationship" => {
                // Self-closing with attributes (OPC format)
                let mut id = None;
                let mut target = None;
                for attr in e.attributes().flatten() {
                    match attr.key.as_ref() {
                        b"Id" => id = Some(attr.unescape_value()?.to_string()),
                        b"Target" => target = Some(attr.unescape_value()?.to_string()),
                        _ => {}
                    }
                }
                if let (Some(id), Some(target)) = (id, target) {
                    let normalized = target.replace('\\', "/");
                    id_to_path.insert(id, normalized);
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                if in_relationship {
                    // Process child element text
                    let text = text_buf.trim().to_string();
                    if !text.is_empty() {
                        match child_tag.as_deref() {
                            Some("Id") => rel_id = Some(text),
                            Some("Target") => rel_target = Some(text),
                            _ => {}
                        }
                    }
                    child_tag = None;
                }

                if name == "Relationship" {
                    // Finalize relationship from child elements if not already set via attributes
                    if let (Some(id), Some(target)) = (rel_id.take(), rel_target.take()) {
                        let normalized = target.replace('\\', "/");
                        id_to_path.insert(id, normalized);
                    }
                    in_relationship = false;
                }

                text_buf.clear();
            }
            Ok(Event::Text(e)) => {
                if in_relationship {
                    text_buf.push_str(&e.unescape()?);
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow!("XML parse error in Reference.xml: {e}")),
            _ => {}
        }
    }

    Ok(ResourceMap { id_to_path })
}

/// Parse `Slide_*.xml` to extract the SolarSystem scene.
///
/// Returns `Ok(None)` if no SolarSystem element is found.
fn parse_slide_xml(xml: &str) -> Result<Option<SolarSystemScene>> {
    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);

    let mut in_solar_system = false;
    let mut in_textures = false;
    let mut in_look_dir = false;
    let mut in_up_dir = false;
    let mut in_brush = false;
    let mut in_image_brush = false;

    let mut current_texture_key: Option<String> = None;
    let mut current_texture_source: Option<String> = None;

    // Scene fields
    let mut planet_type = String::new();
    let mut texture_type = String::new();
    let mut texture_key = String::new();
    let mut planet_view = String::new();
    let mut planet_state = String::new();
    let mut cam_pos = [0.0f32; 3];
    let mut look_dir = [0.0f32; 3];
    let mut up_dir = [0.0f32, 1.0, 0.0];
    let mut texture_matrix = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut textures: Vec<TextureLayer> = Vec::new();
    let mut screen_x = 0.0f32;
    let mut screen_y = 0.0f32;
    let mut screen_width = 1280.0f32;
    let mut screen_height = 720.0f32;

    let mut buf = Vec::new();
    let mut text_buf = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "SolarSystem" => in_solar_system = true,
                    "Textures" if in_solar_system => in_textures = true,
                    "SolarSystemTexture" if in_textures => {
                        current_texture_key = None;
                        current_texture_source = None;
                    }
                    "Brush" if in_textures => in_brush = true,
                    "ImageBrush" if in_brush => in_image_brush = true,
                    "CameraLookDirection" if in_solar_system => in_look_dir = true,
                    "CameraUpDirection" if in_solar_system => in_up_dir = true,
                    _ => {}
                }
                text_buf.clear();
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "SolarSystem" => in_solar_system = false,
                    "Textures" => in_textures = false,
                    "Key" if in_textures => {
                        current_texture_key = Some(text_buf.trim().to_string());
                    }
                    "SolarSystemTexture" if in_textures => {
                        if let (Some(key), Some(source)) =
                            (current_texture_key.take(), current_texture_source.take())
                        {
                            // Extract resource ID from id:// URI
                            let resource_id = source
                                .strip_prefix("id://")
                                .unwrap_or(&source)
                                .to_string();
                            textures.push(TextureLayer {
                                key,
                                resource_id,
                                opacity: 1.0,
                            });
                        }
                    }
                    "Brush" => in_brush = false,
                    "ImageBrush" => in_image_brush = false,
                    "CameraLookDirection" => in_look_dir = false,
                    "CameraUpDirection" => in_up_dir = false,
                    _ => {}
                }

                // Process text content for scalar fields
                if in_solar_system && !in_textures {
                    let text = text_buf.trim().to_string();
                    if !text.is_empty() {
                        match name.as_str() {
                            "PlanetSystemType" => planet_type = text,
                            "EarthTextureType" => texture_type = text,
                            "EarthTextureKey" => texture_key = text,
                            "PlanetView" => planet_view = text,
                            "PlanetState" => planet_state = text,
                            "CameraPosition" => {
                                cam_pos = parse_vec3_str(&text);
                            }
                            "Texture2DMatrix" => {
                                texture_matrix = parse_matrix6_str(&text);
                            }
                            "X" if !in_look_dir && !in_up_dir => screen_x = text.parse().unwrap_or(0.0),
                            "Y" if !in_look_dir && !in_up_dir => screen_y = text.parse().unwrap_or(0.0),
                            "Width" if !in_look_dir && !in_up_dir => {
                                screen_width = text.parse().unwrap_or(1280.0)
                            }
                            "Height" if !in_look_dir && !in_up_dir => {
                                screen_height = text.parse().unwrap_or(720.0)
                            }
                            "X" if in_look_dir => look_dir[0] = text.parse().unwrap_or(0.0),
                            "Y" if in_look_dir => look_dir[1] = text.parse().unwrap_or(0.0),
                            "Z" if in_look_dir => look_dir[2] = text.parse().unwrap_or(0.0),
                            "X" if in_up_dir => up_dir[0] = text.parse().unwrap_or(0.0),
                            "Y" if in_up_dir => up_dir[1] = text.parse().unwrap_or(0.0),
                            "Z" if in_up_dir => up_dir[2] = text.parse().unwrap_or(0.0),
                            _ => {}
                        }
                    }
                }

                text_buf.clear();
            }
            Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "ImageBrush" && in_image_brush {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"Source" {
                            current_texture_source =
                                Some(attr.unescape_value()?.to_string());
                        }
                    }
                }
            }
            Ok(Event::Text(e)) => {
                if in_image_brush && in_textures {
                    // Source might be text content
                    let text = e.unescape()?;
                    if text.contains("id://") {
                        current_texture_source = Some(text.to_string());
                    }
                }
                text_buf.push_str(&e.unescape()?);
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow!("XML parse error in Slide XML: {e}")),
            _ => {}
        }
    }

    if planet_type.is_empty() {
        return Ok(None);
    }

    Ok(Some(SolarSystemScene {
        planet_type,
        texture_type,
        texture_key,
        planet_view,
        planet_state,
        camera: CameraParams {
            position: cam_pos,
            look_direction: look_dir,
            up_direction: up_dir,
        },
        texture_matrix,
        textures,
        screen_x,
        screen_y,
        screen_width,
        screen_height,
    }))
}

/// Parse "x,y,z" into [f32; 3].
fn parse_vec3_str(s: &str) -> [f32; 3] {
    let parts: Vec<f32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    if parts.len() >= 3 {
        [parts[0], parts[1], parts[2]]
    } else {
        [0.0, 0.0, 0.0]
    }
}

/// Parse "1,0,0,1,0,0" into [f32; 6].
fn parse_matrix6_str(s: &str) -> [f32; 6] {
    let parts: Vec<f32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    if parts.len() >= 6 {
        [parts[0], parts[1], parts[2], parts[3], parts[4], parts[5]]
    } else {
        [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
    }
}

/// Load and decode an image from the zip archive.
fn load_texture_from_zip(
    archive: &mut zip::ZipArchive<std::fs::File>,
    zip_path: &str,
    key: &str,
) -> Result<LoadedTexture> {
    let mut entry = archive.by_name(zip_path)?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)?;
    drop(entry);

    // Decode image using the `image` crate (auto-detects format)
    let img = image::load_from_memory(&buf)?;
    let rgba = img.to_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    let pixels = rgba.into_raw();

    Ok(LoadedTexture {
        key: key.to_string(),
        width,
        height,
        rgba: pixels,
    })
}

// ════════════════════════════════════════════════════════════════════════════
//  ICOSPHERE GENERATOR (with UV coordinates)
// ════════════════════════════════════════════════════════════════════════════

/// A sphere mesh with UV coordinates for texture mapping.
///
/// Generated as an icosphere (subdivided icosahedron) for uniform
/// vertex distribution, with equirectangular UV projection.
#[derive(Debug, Clone)]
pub struct TexturedSphere {
    /// Vertex positions on the unit sphere.
    pub vertices: Vec<[f32; 3]>,
    /// Vertex normals (same as positions for a unit sphere).
    pub normals: Vec<[f32; 3]>,
    /// UV coordinates (equirectangular projection).
    pub uvs: Vec<[f32; 2]>,
    /// Triangle indices (counter-clockwise from outside).
    pub indices: Vec<u32>,
}

/// Generate an icosphere with UV coordinates.
///
/// # Arguments
/// * `subdivisions` — Number of subdivision passes (0 = base icosahedron).
///   Recommended: 3 (320 faces, 162 vertices) for good quality/performance.
pub fn generate_icosphere(subdivisions: u32) -> TexturedSphere {
    let (mut vertices, mut faces) = icosahedron_base();

    for _ in 0..subdivisions {
        let (new_v, new_f) = subdivide(&vertices, &faces);
        vertices = new_v;
        faces = new_f;
    }

    // Normals = positions (unit sphere)
    let normals: Vec<[f32; 3]> = vertices.iter().map(|v| [v[0], v[1], v[2]]).collect();

    // UV: equirectangular projection
    let uvs: Vec<[f32; 2]> = vertices
        .iter()
        .map(|v| {
            let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            let nx = v[0] / len;
            let ny = v[1] / len;
            let nz = v[2] / len;
            let u = nz.atan2(nx) / (2.0 * std::f32::consts::PI) + 0.5;
            let v_coord = ny.asin() / std::f32::consts::PI + 0.5;
            [u, v_coord]
        })
        .collect();

    let indices: Vec<u32> = faces.iter().flat_map(|&[a, b, c]| [a, b, c]).collect();

    TexturedSphere {
        vertices,
        normals,
        uvs,
        indices,
    }
}

/// Build the base icosahedron (12 vertices, 20 faces).
fn icosahedron_base() -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
    let phi = (1.0 + 5.0_f32.sqrt()) / 2.0;

    let raw = [
        [-1.0, phi, 0.0],
        [1.0, phi, 0.0],
        [-1.0, -phi, 0.0],
        [1.0, -phi, 0.0],
        [0.0, -1.0, phi],
        [0.0, 1.0, phi],
        [0.0, -1.0, -phi],
        [0.0, 1.0, -phi],
        [phi, 0.0, -1.0],
        [phi, 0.0, 1.0],
        [-phi, 0.0, -1.0],
        [-phi, 0.0, 1.0],
    ];

    let vertices: Vec<[f32; 3]> = raw
        .iter()
        .map(|&[x, y, z]| {
            let len = (x * x + y * y + z * z).sqrt();
            [x / len, y / len, z / len]
        })
        .collect();

    let faces = vec![
        [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11],
        [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9],
        [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
        [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
    ];

    (vertices, faces)
}

/// Subdivide each triangle into 4 smaller triangles.
fn subdivide(
    vertices: &[[f32; 3]],
    faces: &[[u32; 3]],
) -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
    let mut new_vertices = vertices.to_vec();
    let mut new_faces = Vec::with_capacity(faces.len() * 4);
    let mut midpoints: HashMap<(u32, u32), u32> = HashMap::new();

    for &[i0, i1, i2] in faces {
        let m01 = midpoint(i0, i1, vertices, &mut new_vertices, &mut midpoints);
        let m12 = midpoint(i1, i2, vertices, &mut new_vertices, &mut midpoints);
        let m20 = midpoint(i2, i0, vertices, &mut new_vertices, &mut midpoints);

        new_faces.push([i0, m01, m20]);
        new_faces.push([i1, m12, m01]);
        new_faces.push([i2, m20, m12]);
        new_faces.push([m01, m12, m20]);
    }

    (new_vertices, new_faces)
}

/// Get or create the midpoint vertex for edge (a, b), projected to unit sphere.
fn midpoint(
    a: u32,
    b: u32,
    old_vertices: &[[f32; 3]],
    new_vertices: &mut Vec<[f32; 3]>,
    cache: &mut HashMap<(u32, u32), u32>,
) -> u32 {
    let key = if a < b { (a, b) } else { (b, a) };
    if let Some(&idx) = cache.get(&key) {
        return idx;
    }

    let va = old_vertices[a as usize];
    let vb = old_vertices[b as usize];
    let mid = [
        (va[0] + vb[0]) * 0.5,
        (va[1] + vb[1]) * 0.5,
        (va[2] + vb[2]) * 0.5,
    ];
    let len = (mid[0] * mid[0] + mid[1] * mid[1] + mid[2] * mid[2]).sqrt();
    let projected = [mid[0] / len, mid[1] / len, mid[2] / len];

    let idx = new_vertices.len() as u32;
    new_vertices.push(projected);
    cache.insert(key, idx);
    idx
}

// ════════════════════════════════════════════════════════════════════════════
//  RENDERER
// ════════════════════════════════════════════════════════════════════════════

/// Selectable data layer for visualization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataLayer {
    Satellite,
    Rainfall,
    Temperature,
    Population,
}

impl DataLayer {
    /// Returns the texture key string matching EasiNote's XML.
    pub fn key(&self) -> &'static str {
        match self {
            DataLayer::Satellite => "Satellite",
            DataLayer::Rainfall => "Rainfall",
            DataLayer::Temperature => "Temperature",
            DataLayer::Population => "Population",
        }
    }

    /// Returns a human-readable label for the UI.
    pub fn label(&self) -> &'static str {
        match self {
            DataLayer::Satellite => "Satellite",
            DataLayer::Rainfall => "Rainfall",
            DataLayer::Temperature => "Temperature",
            DataLayer::Population => "Population",
        }
    }

    /// All variants for iteration.
    pub fn all() -> [DataLayer; 4] {
        [
            DataLayer::Satellite,
            DataLayer::Rainfall,
            DataLayer::Temperature,
            DataLayer::Population,
        ]
    }
}

/// Solar system renderer state.
///
/// Manages texture handles, icosphere mesh, camera, and rendering parameters.
pub struct SolarSystemRenderer {
    /// Parsed scene (None until a file is loaded).
    pub scene: Option<SolarSystemScene>,
    /// Uploaded egui texture handles, keyed by texture name.
    pub textures: HashMap<String, TextureHandle>,
    /// Pre-generated icosphere mesh.
    pub sphere: TexturedSphere,
    /// 3D orbit camera.
    pub camera: Camera3D,
    /// Currently selected data layer.
    pub current_layer: DataLayer,
    /// Enhancement mode (true = textured + lighting + atmosphere; false = flat).
    pub enhancement_mode: bool,
    /// Blend factor for overlay layer (0.0 = base only, 1.0 = overlay only).
    pub blend_factor: f32,
    /// Sphere display radius in world units.
    pub sphere_radius: f32,
}

impl Default for SolarSystemRenderer {
    fn default() -> Self {
        Self {
            scene: None,
            textures: HashMap::new(),
            sphere: generate_icosphere(3),
            camera: Camera3D::default(),
            current_layer: DataLayer::Satellite,
            enhancement_mode: true,
            blend_factor: 0.3,
            sphere_radius: 3.0,
        }
    }
}

impl SolarSystemRenderer {
    /// Create a new renderer with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Upload decoded textures to egui's GPU texture manager.
    pub fn upload_textures(&mut self, ctx: &egui::Context, loaded: &[LoadedTexture]) {
        self.textures.clear();
        for tex in loaded {
            let color_image = ColorImage::from_rgba_unmultiplied(
                [tex.width as usize, tex.height as usize],
                &tex.rgba,
            );
            let handle = ctx.load_texture(
                format!("solar_{}", tex.key),
                color_image,
                TextureOptions::LINEAR,
            );
            self.textures.insert(tex.key.clone(), handle);
        }
    }

    /// Initialize camera from EasiNote camera parameters.
    pub fn init_camera_from_scene(&mut self, scene: &SolarSystemScene) {
        let cam = &scene.camera;
        let eye = Vector3::new(cam.position[0], cam.position[1], cam.position[2]);
        let look = Vector3::new(
            cam.look_direction[0],
            cam.look_direction[1],
            cam.look_direction[2],
        );
        let up = Vector3::new(cam.up_direction[0], cam.up_direction[1], cam.up_direction[2]);

        // Target = eye + look_dir (the point the camera looks at)
        self.camera.target = look + eye;
        self.camera.distance = look.norm().max(1.0);

        // Compute rotation from basis vectors
        let forward = look.normalize();
        let right = forward.cross(&up).normalize();
        let up_corrected = right.cross(&forward).normalize();

        // Camera3D rotation maps (0,0,1)→forward, (0,1,0)→up, (1,0,0)→right
        let rot_matrix = Matrix3::new(
            right[0], up_corrected[0], forward[0],
            right[1], up_corrected[1], forward[1],
            right[2], up_corrected[2], forward[2],
        );
        self.camera.rotation =
            UnitQuaternion::from_rotation_matrix(&Rotation3::from_matrix_unchecked(rot_matrix));
        self.camera.projection = ProjectionMode::Perspective;
        self.camera.fov = std::f32::consts::PI / 4.0;
    }

    /// Render the solar system to the egui Painter.
    ///
    /// This is the main render entry point. It draws:
    /// 1. Textured sphere with Lambert lighting (enhancement mode)
    ///    or flat textured sphere (compatibility mode)
    /// 2. Atmosphere scattering overlay (enhancement mode only)
    /// 3. Optional overlay texture blending
    pub fn render(&self, painter: &Painter, screen_size: (f32, f32), screen_rect: egui::Rect) {
        // Dark space background
        painter.rect_filled(
            screen_rect,
            0.0,
            Color32::from_rgb(8, 10, 20),
        );

        if self.textures.is_empty() {
            return;
        }

        let aspect = screen_size.0 / screen_size.1;

        // Determine which textures to use
        let base_key = "Satellite";
        let overlay_key = self.current_layer.key();

        let base_texture = self.textures.get(base_key);
        let overlay_texture = if self.current_layer != DataLayer::Satellite {
            self.textures.get(overlay_key)
        } else {
            None
        };

        // Render base textured sphere
        if let Some(tex) = base_texture {
            self.render_textured_sphere(painter, tex, screen_size, aspect, 1.0);
        }

        // Render overlay texture with blend factor
        if let Some(tex) = overlay_texture {
            if self.blend_factor > 0.0 {
                self.render_textured_sphere(painter, tex, screen_size, aspect, self.blend_factor);
            }
        }

        // Render atmosphere scattering (enhancement mode only)
        if self.enhancement_mode {
            self.render_atmosphere(painter, screen_size, aspect);
        }
    }

    /// Render a textured sphere with Lambert lighting.
    ///
    /// Projects the icosphere to 2D screen space, performs back-face culling,
    /// calculates Lambert diffuse lighting per face, and builds an egui::Mesh
    /// with UV coordinates for GPU texture sampling.
    fn render_textured_sphere(
        &self,
        painter: &Painter,
        texture: &TextureHandle,
        screen_size: (f32, f32),
        aspect: f32,
        alpha: f32,
    ) {
        let vp = self.camera.view_projection_matrix(aspect);
        let cam_pos = self.camera.position();

        // Light direction (from surface toward light source)
        let light_dir = Vector3::new(-0.4_f32, -1.0, -0.6).normalize();
        let ambient: f32 = 0.35;

        let radius = self.sphere_radius;

        // Project all vertices
        let projected: ProjectedVerts = self
            .sphere
            .vertices
            .iter()
            .map(|v| {
                let world = Point3::new(v[0] * radius, v[1] * radius, v[2] * radius);
                let proj = vp.transform_point(&world);
                let screen = if proj.z < -1.0 || proj.z > 1.0 {
                    None
                } else {
                    let sx = (proj.x + 1.0) * 0.5 * screen_size.0;
                    let sy = (1.0 - (proj.y + 1.0) * 0.5) * screen_size.1;
                    Some((sx, sy, proj.z))
                };
                (screen, [v[0], v[1], v[2]])
            })
            .collect();

        // Build mesh: collect visible faces with Lambert lighting
        let mut mesh = Mesh {
            texture_id: texture.id(),
            ..Default::default()
        };

        for chunk in self.sphere.indices.chunks(3) {
            if chunk.len() < 3 {
                break;
            }
            let (a, b, c) = (chunk[0], chunk[1], chunk[2]);

            let (pa, na) = match projected.get(a as usize) {
                Some(v) => v,
                None => continue,
            };
            let (pb, nb) = match projected.get(b as usize) {
                Some(v) => v,
                None => continue,
            };
            let (pc, nc) = match projected.get(c as usize) {
                Some(v) => v,
                None => continue,
            };

            let sa = match pa {
                Some(s) => s,
                None => continue,
            };
            let sb = match pb {
                Some(s) => s,
                None => continue,
            };
            let sc = match pc {
                Some(s) => s,
                None => continue,
            };

            // Face normal (world space, using original sphere normals)
            let normal = Vector3::new(
                na[0] + nb[0] + nc[0],
                na[1] + nb[1] + nc[1],
                na[2] + nb[2] + nc[2],
            )
            .normalize();

            // Face center in world space
            let face_center = Point3::new(
                (na[0] + nb[0] + nc[0]) / 3.0 * radius,
                (na[1] + nb[1] + nc[1]) / 3.0 * radius,
                (na[2] + nb[2] + nc[2]) / 3.0 * radius,
            );

            // View direction (from face to camera)
            let view_dir = cam_pos - face_center.coords;
            if view_dir.norm() < 1e-6 {
                continue;
            }
            let view_n = view_dir.normalize();

            // Back-face culling
            if normal.dot(&view_n) < 0.0 {
                continue;
            }

            // Lambert lighting
            let diffuse = (-normal.dot(&light_dir)).max(0.0);
            let brightness = ambient + (1.0 - ambient) * diffuse;
            let light_val = (brightness * 255.0).clamp(0.0, 255.0) as u8;
            let alpha_val = (alpha * 255.0).clamp(0.0, 255.0) as u8;
            let vertex_color = Color32::from_rgba_unmultiplied(light_val, light_val, light_val, alpha_val);

            // Get UVs
            let uva = self.sphere.uvs[a as usize];
            let uvb = self.sphere.uvs[b as usize];
            let uvc = self.sphere.uvs[c as usize];

            // Add vertices and indices
            let base_idx = mesh.vertices.len() as u32;
            mesh.vertices.push(egui::epaint::Vertex {
                pos: Pos2::new(sa.0, sa.1),
                uv: Pos2::new(uva[0], uva[1]),
                color: vertex_color,
            });
            mesh.vertices.push(egui::epaint::Vertex {
                pos: Pos2::new(sb.0, sb.1),
                uv: Pos2::new(uvb[0], uvb[1]),
                color: vertex_color,
            });
            mesh.vertices.push(egui::epaint::Vertex {
                pos: Pos2::new(sc.0, sc.1),
                uv: Pos2::new(uvc[0], uvc[1]),
                color: vertex_color,
            });
            mesh.indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2]);
        }

        if !mesh.indices.is_empty() {
            painter.add(egui::Shape::mesh(mesh));
        }
    }

    /// Render atmosphere scattering (Fresnel-based blue glow around edges).
    ///
    /// Renders a slightly larger transparent sphere where the alpha is
    /// proportional to the Fresnel factor (1 - dot(view, normal)),
    /// creating a blue glow at the silhouette edges.
    fn render_atmosphere(
        &self,
        painter: &Painter,
        screen_size: (f32, f32),
        aspect: f32,
    ) {
        let vp = self.camera.view_projection_matrix(aspect);
        let cam_pos = self.camera.position();
        let radius = self.sphere_radius * 1.04; // Slightly larger than the planet

        // Project all vertices
        let projected: ProjectedVerts = self
            .sphere
            .vertices
            .iter()
            .map(|v| {
                let world = Point3::new(v[0] * radius, v[1] * radius, v[2] * radius);
                let proj = vp.transform_point(&world);
                let screen = if proj.z < -1.0 || proj.z > 1.0 {
                    None
                } else {
                    let sx = (proj.x + 1.0) * 0.5 * screen_size.0;
                    let sy = (1.0 - (proj.y + 1.0) * 0.5) * screen_size.1;
                    Some((sx, sy, proj.z))
                };
                (screen, [v[0], v[1], v[2]])
            })
            .collect();

        // Use a white texture (egui's default) so vertex color is the final color
        let mut mesh = Mesh {
            texture_id: egui::TextureId::default(),
            ..Default::default()
        };

        for chunk in self.sphere.indices.chunks(3) {
            if chunk.len() < 3 {
                break;
            }
            let (a, b, c) = (chunk[0], chunk[1], chunk[2]);

            let (pa, na) = match projected.get(a as usize) {
                Some(v) => v,
                None => continue,
            };
            let (pb, nb) = match projected.get(b as usize) {
                Some(v) => v,
                None => continue,
            };
            let (pc, nc) = match projected.get(c as usize) {
                Some(v) => v,
                None => continue,
            };

            let sa = match pa {
                Some(s) => s,
                None => continue,
            };
            let sb = match pb {
                Some(s) => s,
                None => continue,
            };
            let sc = match pc {
                Some(s) => s,
                None => continue,
            };

            // Face center in world space
            let center = Vector3::new(
                (na[0] + nb[0] + nc[0]) / 3.0 * radius,
                (na[1] + nb[1] + nc[1]) / 3.0 * radius,
                (na[2] + nb[2] + nc[2]) / 3.0 * radius,
            );

            // Normal at face center (outward from sphere center)
            let normal = center.normalize();

            // View direction
            let view_dir = (cam_pos - center).normalize();

            // Fresnel factor: high at silhouette edges, low at center
            let fresnel = (1.0 - normal.dot(&view_dir).max(0.0)).powi(3);

            // Atmosphere color (blue glow)
            let alpha = (fresnel * 180.0).clamp(0.0, 255.0) as u8;
            let color = Color32::from_rgba_unmultiplied(80, 140, 255, alpha);

            let base_idx = mesh.vertices.len() as u32;
            mesh.vertices.push(egui::epaint::Vertex {
                pos: Pos2::new(sa.0, sa.1),
                uv: Pos2::ZERO,
                color,
            });
            mesh.vertices.push(egui::epaint::Vertex {
                pos: Pos2::new(sb.0, sb.1),
                uv: Pos2::ZERO,
                color,
            });
            mesh.vertices.push(egui::epaint::Vertex {
                pos: Pos2::new(sc.0, sc.1),
                uv: Pos2::ZERO,
                color,
            });
            mesh.indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2]);
        }

        if !mesh.indices.is_empty() {
            painter.add(egui::Shape::mesh(mesh));
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  TESTS
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_icosphere_base() {
        let sphere = generate_icosphere(0);
        assert_eq!(sphere.vertices.len(), 12);
        assert_eq!(sphere.indices.len() / 3, 20);
        assert_eq!(sphere.normals.len(), 12);
        assert_eq!(sphere.uvs.len(), 12);
    }

    #[test]
    fn test_icosphere_subdivision_1() {
        let sphere = generate_icosphere(1);
        assert_eq!(sphere.indices.len() / 3, 80);
        assert_eq!(sphere.vertices.len(), 42);
    }

    #[test]
    fn test_icosphere_subdivision_3() {
        let sphere = generate_icosphere(3);
        assert_eq!(sphere.indices.len() / 3, 1280);
    }

    #[test]
    fn test_icosphere_unit_radius() {
        let sphere = generate_icosphere(2);
        for v in &sphere.vertices {
            let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-5, "Vertex not on unit sphere: {len}");
        }
    }

    #[test]
    fn test_icosphere_uv_range() {
        let sphere = generate_icosphere(2);
        for uv in &sphere.uvs {
            assert!(uv[0] >= 0.0 && uv[0] <= 1.0, "U out of range: {}", uv[0]);
            assert!(uv[1] >= 0.0 && uv[1] <= 1.0, "V out of range: {}", uv[1]);
        }
    }

    #[test]
    fn test_parse_vec3_str() {
        assert_eq!(parse_vec3_str("0,-8.817,1.804"), [0.0, -8.817, 1.804]);
        assert_eq!(parse_vec3_str("1, 2, 3"), [1.0, 2.0, 3.0]);
        assert_eq!(parse_vec3_str("invalid"), [0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_parse_matrix6_str() {
        assert_eq!(parse_matrix6_str("1,0,0,1,0,0"), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        assert_eq!(
            parse_matrix6_str("0.5, 0.1, 0.2, 0.8, 10, 20"),
            [0.5, 0.1, 0.2, 0.8, 10.0, 20.0]
        );
    }

    #[test]
    fn test_parse_reference_xml() {
        let xml = r#"<?xml version="1.0"?>
<Reference>
  <Relationships>
    <Relationship>
      <Id>abc123</Id>
      <Target>Resources\abc123.jpg</Target>
      <Hash>aaa</Hash>
    </Relationship>
    <Relationship>
      <Id>def456</Id>
      <Target>Resources\def456</Target>
      <Hash>bbb</Hash>
    </Relationship>
  </Relationships>
</Reference>"#;

        let map = parse_reference_xml(xml).unwrap();
        assert_eq!(map.id_to_path.len(), 2);
        assert_eq!(map.id_to_path.get("abc123").unwrap(), "Resources/abc123.jpg");
        assert_eq!(map.id_to_path.get("def456").unwrap(), "Resources/def456");
    }

    #[test]
    fn test_parse_slide_xml_with_solar_system() {
        let xml = r#"<?xml version="1.0"?>
<Slide>
  <Width>1280</Width>
  <Height>720</Height>
  <Elements>
    <SolarSystem>
      <PlanetSystemType>Earth</PlanetSystemType>
      <EarthTextureType>Satellite</EarthTextureType>
      <EarthTextureKey>Satellite</EarthTextureKey>
      <PlanetView>ThreeD</PlanetView>
      <PlanetState>Overall</PlanetState>
      <CameraPosition>0,-8.817,1.804</CameraPosition>
      <CameraLookDirection>
        <X>0</X>
        <Y>8.817</Y>
        <Z>-1.804</Z>
      </CameraLookDirection>
      <CameraUpDirection>
        <X>0</X>
        <Y>0</Y>
        <Z>1</Z>
      </CameraUpDirection>
      <Texture2DMatrix>1,0,0,1,0,0</Texture2DMatrix>
      <Textures>
        <SolarSystemTexture>
          <Key>Satellite</Key>
          <Brush>
            <ImageBrush>
              <Source>id://d24b175ad8f50c9978bb1c560f087dee</Source>
              <Stretch>Fill</Stretch>
            </ImageBrush>
          </Brush>
        </SolarSystemTexture>
        <SolarSystemTexture>
          <Key>Temperature</Key>
          <Brush>
            <ImageBrush>
              <Source>id://9ff3f46e12405736f3129c7271cf7882</Source>
              <Stretch>Fill</Stretch>
            </ImageBrush>
          </Brush>
        </SolarSystemTexture>
      </Textures>
      <X>0</X>
      <Y>0</Y>
      <Width>1280</Width>
      <Height>720</Height>
    </SolarSystem>
  </Elements>
</Slide>"#;

        let scene = parse_slide_xml(xml).unwrap().unwrap();
        assert_eq!(scene.planet_type, "Earth");
        assert_eq!(scene.texture_type, "Satellite");
        assert_eq!(scene.planet_view, "ThreeD");
        assert_eq!(scene.camera.position, [0.0, -8.817, 1.804]);
        assert_eq!(scene.camera.look_direction, [0.0, 8.817, -1.804]);
        assert_eq!(scene.camera.up_direction, [0.0, 0.0, 1.0]);
        assert_eq!(scene.textures.len(), 2);
        assert_eq!(scene.textures[0].key, "Satellite");
        assert_eq!(
            scene.textures[0].resource_id,
            "d24b175ad8f50c9978bb1c560f087dee"
        );
        assert_eq!(scene.textures[1].key, "Temperature");
    }

    #[test]
    fn test_parse_slide_xml_without_solar_system() {
        let xml = r#"<?xml version="1.0"?>
<Slide>
  <Width>1280</Width>
  <Height>720</Height>
  <Elements>
    <Text>Hello</Text>
  </Elements>
</Slide>"#;

        let result = parse_slide_xml(xml).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_data_layer_keys() {
        assert_eq!(DataLayer::Satellite.key(), "Satellite");
        assert_eq!(DataLayer::Rainfall.key(), "Rainfall");
        assert_eq!(DataLayer::Temperature.key(), "Temperature");
        assert_eq!(DataLayer::Population.key(), "Population");
    }

    #[test]
    fn test_renderer_default() {
        let renderer = SolarSystemRenderer::new();
        assert_eq!(renderer.current_layer, DataLayer::Satellite);
        assert!(renderer.enhancement_mode);
        assert!((renderer.blend_factor - 0.3).abs() < 1e-6);
        assert!((renderer.sphere_radius - 3.0).abs() < 1e-6);
        assert!(renderer.scene.is_none());
        assert!(renderer.textures.is_empty());
        assert_eq!(renderer.sphere.indices.len() / 3, 1280); // subdivision 3
    }
}
