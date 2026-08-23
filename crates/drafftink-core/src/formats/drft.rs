//! Native .drft format importer — built-in, no plugin required.

use crate::document;
use crate::model::CoursewareDoc;
use crate::plugin::api::{FileImporter, PluginContext};

pub struct DrftImporter;

impl Default for DrftImporter {
    fn default() -> Self {
        Self::new()
    }
}

impl DrftImporter {
    pub fn new() -> Self {
        Self
    }
}

impl FileImporter for DrftImporter {
    fn supported_extensions(&self) -> Vec<String> {
        vec!["drft".into()]
    }

    fn can_import(&self, _data: &[u8]) -> bool {
        // .drft is the native serialization of CoursewareDoc via bincode.
        // We accept any file with the .drft extension; bincode::deserialize
        // will validate during import.
        true
    }

    fn import(&self, data: &[u8], _ctx: &dyn PluginContext) -> Result<CoursewareDoc, String> {
        document::load_document_slice(data).map_err(|e| e.to_string())
    }
}
