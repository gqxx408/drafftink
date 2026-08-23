//! Multi-page state management.
//!
//! Manages a list of pages, each with elements and annotations.
#![allow(dead_code)]
//! Syncs with `CoursewareDoc` for save/load compatibility.

use crate::annotation::{AnnotationState, StrokeData};
use drafftink_core::model::{CoursewareDoc, Element, PageContent};

/// Page-level content including element data and annotation layer.
#[derive(Debug, Clone, Default)]
pub struct PageData {
    pub elements: Vec<Element>,
    pub annotations: Vec<StrokeData>,
}

/// State machine for multi-page courseware.
#[derive(Debug, Clone, Default)]
pub struct MultiPageState {
    pub pages: Vec<PageData>,
    pub current_page: usize,
}

impl MultiPageState {
    pub fn new() -> Self {
        Self {
            pages: vec![PageData::default()],
            current_page: 0,
        }
    }

    // ------------------------------------------------------------------
    // Page navigation
    // ------------------------------------------------------------------

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    #[allow(dead_code)]
    pub fn go_to_page(&mut self, index: usize) {
        if index < self.pages.len() {
            self.current_page = index;
        }
    }

    pub fn prev_page(&mut self) {
        if self.current_page > 0 {
            self.current_page -= 1;
        }
    }

    pub fn next_page(&mut self) {
        if self.current_page + 1 < self.pages.len() {
            self.current_page += 1;
        }
    }

    pub fn add_page(&mut self) {
        self.pages.push(PageData::default());
        self.current_page = self.pages.len() - 1;
    }

    /// Reference to the current page.
    pub fn current(&self) -> &PageData {
        &self.pages[self.current_page]
    }

    /// Mutable reference to the current page.
    pub fn current_mut(&mut self) -> &mut PageData {
        &mut self.pages[self.current_page]
    }

    // ------------------------------------------------------------------
    // Annotation sync (page -> annotation state)
    // ------------------------------------------------------------------

    /// Copy the annotation state's strokes into the current page's data.
    pub fn save_annotations(&mut self, annotation: &AnnotationState) {
        self.current_mut().annotations = annotation.strokes.clone();
    }

    /// Load the current page's annotations into the annotation state.
    pub fn load_annotations(&self, annotation: &mut AnnotationState) {
        annotation.set_strokes(self.current().annotations.clone());
    }

    /// Clear annotations for the current page.
    pub fn clear_page_annotations(&mut self) {
        self.current_mut().annotations.clear();
    }

    // ------------------------------------------------------------------
    // Sync with CoursewareDoc (for file I/O)
    // ------------------------------------------------------------------

    /// Populate MultiPageState from a CoursewareDoc.
    pub fn from_doc(doc: &CoursewareDoc) -> Self {
        if !doc.pages.is_empty() {
            let pages: Vec<PageData> = doc
                .pages
                .iter()
                .map(|p| PageData {
                    elements: p.elements.clone(),
                    annotations: Vec::new(), // loaded later via load_page_annotations
                })
                .collect();
            Self {
                pages,
                current_page: 0,
            }
        } else {
            // Legacy single-page document
            Self {
                pages: vec![PageData {
                    elements: doc.elements.clone(),
                    annotations: Vec::new(),
                }],
                current_page: 0,
            }
        }
    }

    /// Sync MultiPageState back into CoursewareDoc.
    pub fn sync_to_doc(&self, doc: &mut CoursewareDoc) {
        // Sync first page's elements to doc.elements for backward compat
        if let Some(page) = self.pages.first() {
            doc.elements = page.elements.clone();
        }
        // Build PageContent vec
        doc.pages = self
            .pages
            .iter()
            .map(|p| {
                let annotations_data =
                    bincode::serialize(&p.annotations).unwrap_or_default();
                PageContent {
                    elements: p.elements.clone(),
                    annotations_data,
                    ..Default::default()
                }
            })
            .collect();
    }

    /// Load annotations from the serialised bincode blob in each page.
    pub fn load_page_annotations(&mut self, doc: &CoursewareDoc) {
        for (i, page_content) in doc.pages.iter().enumerate() {
            if i < self.pages.len() && !page_content.annotations_data.is_empty() {
                if let Ok(strokes) =
                    bincode::deserialize::<Vec<StrokeData>>(&page_content.annotations_data)
                {
                    self.pages[i].annotations = strokes;
                }
            }
        }
    }
}
