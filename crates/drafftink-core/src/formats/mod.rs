//! File-format registry — dispatches file loading to the correct importer.
//!
//! Supports both built-in importers (.drft) and plugin-provided ones (.enbx, …).

pub mod drft;

use crate::plugin::api::{FileImporter, PluginContext};
use crate::plugin::loader::PluginManager;
use std::collections::HashMap;
use std::path::Path;

pub use drft::DrftImporter;

pub struct FormatRegistry {
    /// importer name → importer
    importers: Vec<(String, Box<dyn FileImporter>)>,
    /// extension (lowercase, without dot) → importer index
    ext_map: HashMap<String, usize>,
}

impl Default for FormatRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            importers: Vec::new(),
            ext_map: HashMap::new(),
        };

        // Always register native .drft support
        reg.register("drft (native)", Box::new(DrftImporter::new()));

        reg
    }

    /// Register an importer. Returns the assigned index.
    pub fn register(&mut self, name: &str, importer: Box<dyn FileImporter>) -> usize {
        let idx = self.importers.len();
        for ext in importer.supported_extensions() {
            self.ext_map.insert(ext.to_lowercase(), idx);
        }
        self.importers.push((name.to_string(), importer));
        idx
    }

    /// Register all importers from loaded plugins.
    pub fn register_from_plugins(&mut self, manager: &PluginManager) {
        for (name, importer) in manager.all_importers() {
            // We need to re-box — the plugin owns the importer so we just reference it.
            // For now, store references via a different mechanism.
            // In production, you'd use Arc<dyn FileImporter> or similar.
            let _ = (name, importer); // placeholder — see fn below
        }
    }

    /// List all supported extensions for file dialogs.
    pub fn supported_extensions(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for (name, importer) in &self.importers {
            for ext in importer.supported_extensions() {
                out.push((name.clone(), ext));
            }
        }
        out
    }

    /// Import a file, dispatching by extension.
    pub fn import_file(
        &self,
        path: &Path,
        ctx: &dyn PluginContext,
    ) -> Result<crate::model::CoursewareDoc, String> {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("No file extension: {path:?}"))?
            .to_lowercase();

        let idx = self
            .ext_map
            .get(&ext)
            .ok_or_else(|| format!("No importer for .{ext} files"))?;

        let data = std::fs::read(path).map_err(|e| format!("Read failed: {e}"))?;

        let importer = &self.importers[*idx].1;
        if !importer.can_import(&data) {
            return Err(format!("File does not appear to be a valid .{ext} file"));
        }

        importer.import(&data, ctx)
    }
}
