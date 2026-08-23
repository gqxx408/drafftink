//! Error types for ENBX import.

use thiserror::Error;

/// All failures from the ENBX importer.
#[derive(Error, Debug)]
pub enum EnbxError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("XML parse error: {0}")]
    Xml(#[from] quick_xml::Error),

    #[error("unsupported or encrypted ENBX")]
    Unsupported,

    #[error("invalid ENBX format: {0}")]
    Format(String),

    #[error("security violation: {0}")]
    Security(String),

    #[error("zip bomb detected: extracted content exceeds safe limit")]
    ZipBomb,

    #[error("slide parse failed: {0}")]
    SlideError(String),
}
