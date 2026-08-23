//! ENBX import/export UI integration.
//!
//! Wraps `drafftink_enbx::parse_enbx` and `drafftink_enbx::generate_enbx`
//! to provide a thin compatibility layer between the desktop app and the
//! enbx compatibility module.
//!
//! ## Functions
//!
//! | Function        | Direction                  |
//! |-----------------|----------------------------|
//! | `import_enbx`   | `.enbx` file → `ElementData` |
//! | `export_enbx`   | `ElementData` → `.enbx` file |

use std::path::Path;

use anyhow::Result;
use drafftink_core::element::ElementData;

// ════════════════════════════════════════════════════════════════════════════
//  Import
// ════════════════════════════════════════════════════════════════════════════

/// Import a `.enbx` file and return the parsed elements as `ElementData`.
///
/// Delegates to `drafftink_enbx::parse_enbx` which handles ZIP streaming,
/// XML parsing, and Seewo-to-internal element conversion via
/// `map_element_from_enbx`.
///
/// # Errors
///
/// Returns an error if the file cannot be read, the ZIP is malformed,
/// or the XML parsing encounters an error.
pub fn import_enbx(path: &Path) -> Result<Vec<ElementData>> {
    log::info!("[enbx-ui] Importing: {}", path.display());
    let enbx_file = drafftink_enbx::parse_enbx(path)?;

    let mut elements = Vec::new();
    for slide in &enbx_file.slides {
        for enbx_elem in &slide.elements {
            // `map_element_from_enbx` never drops elements — `Unknown` and
            // unsupported types degrade to labelled placeholders instead.
            elements.push(drafftink_enbx::map_element_from_enbx(enbx_elem));
        }
    }

    log::info!(
        "[enbx-ui] Import complete: {} element(s) from {} slide(s)",
        elements.len(),
        enbx_file.slides.len(),
    );
    Ok(elements)
}

// ════════════════════════════════════════════════════════════════════════════
//  Export
// ════════════════════════════════════════════════════════════════════════════

/// Export a slice of `ElementData` to a `.enbx` file.
///
/// Maps each `ElementData` to its Seewo equivalent via
/// `map_element_to_enbx`, wraps them in a single `EnbxSlide`, and
/// delegates to `drafftink_enbx::generate_enbx` which creates a
/// ZIP archive with `Reference.xml` and `Slide_1.xml`.
///
/// # Errors
///
/// Returns an error if the file cannot be created or the ZIP write fails.
pub fn export_enbx(elements: &[ElementData], path: &Path) -> Result<()> {
    log::info!(
        "[enbx-ui] Exporting {} element(s) → {}",
        elements.len(),
        path.display(),
    );

    let enbx_elements: Vec<drafftink_enbx::EnbxElement> = elements
        .iter()
        .filter_map(drafftink_enbx::map_element_to_enbx)
        .collect();

    let slide = drafftink_enbx::EnbxSlide {
        elements: enbx_elements,
        ..Default::default()
    };

    drafftink_enbx::generate_enbx(&[slide], &std::collections::HashMap::new(), path)?;
    log::info!("[enbx-ui] Export complete");
    Ok(())
}

// ════════════════════════════════════════════════════════════════════════════
//  Tests
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use drafftink_core::model::{BaseElement, ShapeElement, ShapeType};
    use std::path::PathBuf;

    fn make_test_elements() -> Vec<ElementData> {
        vec![
            ElementData::Shape(ShapeElement {
                base: BaseElement {
                    position: [100.0, 100.0],
                    size: [200.0, 150.0],
                    ..Default::default()
                },
                shape_type: ShapeType::Rectangle,
                has_start_arrow: false,
                has_end_arrow: false,
                scale_y: 0.0,
            }),
            ElementData::formula(BaseElement::default(), "sin(x)"),
        ]
    }

    #[test]
    fn test_export_then_verify() {
        let dir = std::env::temp_dir().join("drafftink_desktop_enbx_test");
        std::fs::create_dir_all(&dir).ok();
        let path: PathBuf = dir.join("roundtrip.enbx");

        let elements = make_test_elements();

        // Export
        let result = export_enbx(&elements, &path);
        assert!(result.is_ok(), "export should succeed");
        assert!(path.exists(), "exported file should exist");

        // Verify the file is a valid ZIP with expected entries
        let file = std::fs::File::open(&path).expect("open file");
        let mut zip = zip::ZipArchive::new(file).expect("valid zip");
        let names: Vec<String> = (0..zip.len())
            .filter_map(|i| zip.by_index(i).ok().map(|f| f.name().to_string()))
            .collect();
        assert!(
            names.contains(&"Reference.xml".to_string()),
            "ZIP should contain Reference.xml, got: {names:?}"
        );
        assert!(
            names.contains(&"Slide_1.xml".to_string()),
            "ZIP should contain Slide_1.xml, got: {names:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_export_empty_elements() {
        let dir = std::env::temp_dir().join("drafftink_desktop_enbx_empty_test");
        std::fs::create_dir_all(&dir).ok();
        let path: PathBuf = dir.join("empty.enbx");

        let result = export_enbx(&[], &path);
        assert!(result.is_ok());
        assert!(path.exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_import_nonexistent_file() {
        let path = Path::new("/nonexistent/file.enbx");
        let result = import_enbx(path);
        assert!(result.is_err(), "importing nonexistent file should fail");
    }
}
