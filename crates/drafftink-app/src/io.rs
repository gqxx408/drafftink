//! File I/O — save, load, and export courseware documents.
#![allow(dead_code)]
use anyhow::{Context, Result};
use drafftink_core::model::{CoursewareDoc, Element};
use image::{Rgba, RgbaImage};
use std::path::Path;

// ---------------------------------------------------------------------------
// Save / Load .courseware
// ---------------------------------------------------------------------------

/// Serialize a `CoursewareDoc` as a compact binary blob and write it to `path`.
///
/// The format is `bincode` (no version header needed — bincode includes
/// its own serialization frame).  Image assets referenced by `ImageElement`
/// are copied into a sibling directory `<name>_assets/`.
pub fn save_courseware(path: &Path, doc: &CoursewareDoc) -> Result<()> {
    let bytes = bincode::serialize(doc).context("serialize document")?;
    std::fs::write(path, &bytes).context("write .courseware file")?;

    // Create assets directory next to the .courseware file
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("courseware");
    let parent = path.parent().unwrap_or(Path::new("."));
    let assets_dir = parent.join(format!("{}_assets", stem));
    std::fs::create_dir_all(&assets_dir).ok();

    for elem in &doc.elements {
        if let Element::Image(img) = elem {
            if let Some(ref data) = img.image_data {
                let img_name = Path::new(&img.image_path)
                    .file_name()
                    .unwrap_or_default();
                let dest = assets_dir.join(img_name);
                std::fs::write(&dest, data).ok();
            }
        }
    }

    Ok(())
}

/// Deserialize a `CoursewareDoc` from a `.courseware` file.
///
/// Tries binary (bincode) first; falls back to JSON for files saved
/// by older versions.  Image data is loaded lazily.
pub fn load_courseware(path: &Path) -> Result<CoursewareDoc> {
    let bytes = std::fs::read(path).context("read .courseware file")?;

    // Try bincode (current format)
    if let Ok(doc) = bincode::deserialize::<CoursewareDoc>(&bytes) {
        return Ok(doc);
    }

    // Fall back to legacy JSON
    let json = std::str::from_utf8(&bytes).context("corrupt .courseware file (not valid UTF-8)")?;
    serde_json::from_str(json).context("deserialize .courseware file")
}

/// Load pixel data for every `ImageElement` in the document from the
/// `_assets/` directory next to the .courseware file.
pub fn load_image_data(doc: &mut CoursewareDoc, courseware_path: &Path) -> Result<()> {
    let stem = courseware_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("courseware");
    let parent = courseware_path.parent().unwrap_or(Path::new("."));
    let assets_dir = parent.join(format!("{}_assets", stem));

    for elem in &mut doc.elements {
        if let Element::Image(img) = elem {
            let img_path = assets_dir.join(
                Path::new(&img.image_path)
                    .file_name()
                    .unwrap_or_default(),
            );
            if img_path.exists() {
                img.image_data = Some(std::fs::read(&img_path)?);
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// PNG export
// ---------------------------------------------------------------------------

/// Off-screen render and export the document as a PNG image at the given
/// resolution.  Common sizes: 1920×1080 or 3840×2160.
pub fn export_png(path: &Path, doc: &CoursewareDoc, width: u32, height: u32) -> Result<()> {
    let scale_x = width as f32 / doc.page_size[0];
    let scale_y = height as f32 / doc.page_size[1];
    let scale = scale_x.min(scale_y);

    let offset_x = (width as f32 - doc.page_size[0] * scale) * 0.5;
    let offset_y = (height as f32 - doc.page_size[1] * scale) * 0.5;

    let mut img = RgbaImage::from_pixel(width, height, Rgba([255, 255, 255, 255]));

    for elem in &doc.elements {
        draw_element_pixels(&mut img, elem, scale, offset_x, offset_y);
    }

    img.save(path).context("write PNG file")?;
    Ok(())
}

fn draw_element_pixels(img: &mut RgbaImage, elem: &Element, scale: f32, ox: f32, oy: f32) {
    let base = elem.base();
    if !base.visible {
        return;
    }

    let x = (base.position[0] * scale + ox).round() as i32;
    let y = (base.position[1] * scale + oy).round() as i32;
    let w = (base.size[0] * scale).round() as i32;
    let h = (base.size[1] * scale).round() as i32;

    let fill = Rgba([
        base.fill_color.r(),
        base.fill_color.g(),
        base.fill_color.b(),
        (base.fill_color.a() as f32 * base.opacity).round() as u8,
    ]);
    let stroke_clr = Rgba([
        base.stroke_color.r(),
        base.stroke_color.g(),
        base.stroke_color.b(),
        (base.stroke_color.a() as f32 * base.opacity).round() as u8,
    ]);

    match elem {
        Element::Shape(shape) => {
            use drafftink_core::model::ShapeType;
            let sw = (base.stroke_width * scale).round() as i32;
            match shape.shape_type {
                ShapeType::Rectangle => {
                    fill_rect(img, x, y, w, h, fill);
                    if sw > 0 {
                        stroke_rect(img, x, y, w, h, sw, stroke_clr);
                    }
                }
                ShapeType::Ellipse => {
                    let cx = x + w / 2;
                    let cy = y + h / 2;
                    let rx = (w / 2).max(1);
                    let ry = (h / 2).max(1);
                    fill_ellipse(img, cx, cy, rx, ry, fill);
                    if sw > 0 {
                        stroke_ellipse(img, cx, cy, rx, ry, sw, stroke_clr);
                    }
                }
                ShapeType::Line => {
                    stroke_line(img, x, y, x + w, y + h, sw.max(1), stroke_clr);
                }
                ShapeType::Arrow => {
                    stroke_line(img, x, y, x + w, y + h, sw.max(1), stroke_clr);
                    // Simple arrow head
                    let ex = x + w;
                    let ey = y + h;
                    let head = (sw * 4).max(6);
                    stroke_line(img, ex, ey, ex - head, ey - head / 2, sw.max(1), stroke_clr);
                    stroke_line(img, ex, ey, ex - head / 2, ey - head, sw.max(1), stroke_clr);
                }
                ShapeType::Bracket | ShapeType::Brace => {
                    // Placeholder: draw a diagonal line segment
                    stroke_line(img, x, y, x + w, y + h, sw.max(1), stroke_clr);
                }
                ShapeType::Fan => {
                    // Placeholder: draw as a filled rectangle
                    fill_rect(img, x, y, w, h, fill);
                    if sw > 0 {
                        stroke_rect(img, x, y, w, h, sw, stroke_clr);
                    }
                }
            }
        }
        Element::Text(_text) => {
            // Placeholder — draw a rect with the text as a label
            fill_rect(img, x, y, w, h, fill);
            stroke_rect(img, x, y, w, h, 1, stroke_clr);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Pixel helpers (Bresenham-style)
// ---------------------------------------------------------------------------

fn put_pixel(img: &mut RgbaImage, x: i32, y: i32, color: Rgba<u8>) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    if x >= 0 && x < w && y >= 0 && y < h {
        img.put_pixel(x as u32, y as u32, color);
    }
}

fn fill_rect(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, color: Rgba<u8>) {
    for dy in 0..h {
        for dx in 0..w {
            put_pixel(img, x + dx, y + dy, color);
        }
    }
}

fn stroke_rect(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, sw: i32, color: Rgba<u8>) {
    for s in 0..sw {
        // Top & bottom
        for dx in 0..w {
            put_pixel(img, x + dx, y + s, color);
            put_pixel(img, x + dx, y + h - 1 - s, color);
        }
        // Left & right
        for dy in 0..h {
            put_pixel(img, x + s, y + dy, color);
            put_pixel(img, x + w - 1 - s, y + dy, color);
        }
    }
}

fn fill_ellipse(img: &mut RgbaImage, cx: i32, cy: i32, rx: i32, ry: i32, color: Rgba<u8>) {
    for dy in -ry..=ry {
        for dx in -rx..=rx {
            if dx * dx * ry * ry + dy * dy * rx * rx <= rx * rx * ry * ry {
                put_pixel(img, cx + dx, cy + dy, color);
            }
        }
    }
}

fn stroke_ellipse(
    img: &mut RgbaImage,
    cx: i32,
    cy: i32,
    rx: i32,
    ry: i32,
    sw: i32,
    color: Rgba<u8>,
) {
    for dy in -ry..=ry {
        for dx in -rx..=rx {
            let d2 = dx * dx * ry * ry + dy * dy * rx * rx;
            let inner = (rx - sw) * (rx - sw) * ry * ry;
            let in_ring = d2 <= rx * rx * ry * ry && d2 >= inner.max(0);
            if in_ring {
                put_pixel(img, cx + dx, cy + dy, color);
            }
        }
    }
}

fn stroke_line(img: &mut RgbaImage, x0: i32, y0: i32, x1: i32, y1: i32, sw: i32, color: Rgba<u8>) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx: i32 = if x0 < x1 { 1 } else { -1 };
    let sy: i32 = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut x = x0;
    let mut y = y0;

    loop {
        // Fat line: draw sw x sw block
        for fy in -(sw / 2)..=(sw / 2) {
            for fx in -(sw / 2)..=(sw / 2) {
                put_pixel(img, x + fx, y + fy, color);
            }
        }

        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            if x == x1 {
                break;
            }
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            if y == y1 {
                break;
            }
            err += dx;
            y += sy;
        }
    }
}
