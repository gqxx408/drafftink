//! Multi-page state management.
//!
//! Manages pages with elements and annotations, syncs with CoursewareDoc.

use crate::annotation::InkStroke;
use drafftink_core::model::{CoursewareDoc, Element, PageContent};

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct PageData {
    pub elements: Vec<Element>,
    pub annotations: Vec<InkStroke>,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct MultiPageState {
    pub pages: Vec<PageData>,
    pub current_page: usize,
}

#[allow(dead_code)]
impl MultiPageState {
    pub fn new() -> Self {
        Self {
            pages: vec![PageData::default()],
            current_page: 0,
        }
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
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

    /// Populate from CoursewareDoc.
    pub fn from_doc(doc: &CoursewareDoc) -> Self {
        if !doc.pages.is_empty() {
            let pages: Vec<PageData> = doc
                .pages
                .iter()
                .map(|p| PageData {
                    elements: p.elements.clone(),
                    annotations: Vec::new(),
                })
                .collect();
            Self {
                pages,
                current_page: 0,
            }
        } else {
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
        if let Some(page) = self.pages.first() {
            doc.elements = page.elements.clone();
        }
        doc.pages = self
            .pages
            .iter()
            .map(|p| {
                let annotations_data = bincode::serialize(&p.annotations).unwrap_or_default();
                PageContent {
                    elements: p.elements.clone(),
                    annotations_data,
                    ..Default::default()
                }
            })
            .collect();
    }

    /// Load annotations from bincode blob in each page.
    pub fn load_page_annotations(&mut self, doc: &CoursewareDoc) {
        for (i, page) in doc.pages.iter().enumerate() {
            if i < self.pages.len() && !page.annotations_data.is_empty() {
                if let Ok(strokes) = bincode::deserialize::<Vec<InkStroke>>(&page.annotations_data)
                {
                    self.pages[i].annotations = strokes;
                }
            }
        }
    }
}
