pub mod cache;
pub mod input;
pub mod renderer;
pub mod smart_alpha;
pub mod spatial;
pub mod stroke;
pub mod toolbar;

use std::path::{Path, PathBuf};

pub use smart_alpha::SmartAlpha;
pub use spatial::Quadtree;
pub use stroke::{InkStroke, ToolType};
pub use toolbar::ToolbarAction;

pub struct AnnotationSystem {
    pub strokes: Vec<InkStroke>,
    pub current: Option<InkStroke>,
    pub tool: ToolType,
    pub color: [u8; 4],
    pub thickness: f32,
    pub input_processor: input::AnnotationInput,
    pub renderer: renderer::AnnotationRenderer,
    pub toolbar: toolbar::AnnotationToolbar,
    pub cache: cache::AnnotationCache,
    #[allow(dead_code)]
    pub doc_hash: u32,
    pub modified: bool,
    /// Smart alpha calculator
    pub smart_alpha: SmartAlpha,
    /// 笔迹包围盒的四叉树索引，服务于视口剔除与橡皮命中测试。
    pub spatial: Quadtree,
    /// 笔迹集合是否已变更、索引需要重建。变更帧退回全量遍历，下一帧重建。
    spatial_dirty: bool,
}

impl AnnotationSystem {
    pub fn new(doc_hash: u32, cache_dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&cache_dir);

        let recovered = cache::AnnotationCache::scan_and_load(&cache_dir, doc_hash);
        let (strokes, cache) = match recovered {
            Some(data) => {
                eprintln!("[annotation] Recovered {} strokes from cache", data.len());
                (
                    data,
                    cache::AnnotationCache::new_with_existing(cache_dir, doc_hash, 0),
                )
            }
            None => (Vec::new(), cache::AnnotationCache::new(cache_dir, doc_hash)),
        };

        Self {
            strokes,
            current: None,
            tool: ToolType::Pen,
            color: [0, 0, 0, 255],
            thickness: 2.5,
            input_processor: input::AnnotationInput::new(),
            renderer: renderer::AnnotationRenderer::default(),
            toolbar: toolbar::AnnotationToolbar::default(),
            cache,
            doc_hash,
            modified: false,
            smart_alpha: SmartAlpha::default(),
            spatial: Quadtree::default(),
            // 缓存恢复出来的笔迹尚未入索引，首帧先建一次。
            spatial_dirty: true,
        }
    }

    /// 整体替换笔迹集合（例如从备课端导入当前页批注）。
    ///
    /// 必须走此方法而非直接写 `strokes` 字段，否则空间索引不会置脏，
    /// 会导致剔除与橡皮命中基于过期下标。
    pub fn set_strokes(&mut self, strokes: Vec<InkStroke>) {
        self.strokes = strokes;
        self.current = None;
        self.spatial_dirty = true;
    }

    /// 外部直接改动 `strokes` 后手动置脏索引。
    pub fn mark_spatial_dirty(&mut self) {
        self.spatial_dirty = true;
    }

    /// Called every frame. Returns toolbar actions for the app to process.
    pub fn update(
        &mut self,
        ctx: &egui::Context,
        screen_rect: egui::Rect,
        page_current: usize,
        page_total: usize,
    ) -> ToolbarAction {
        // 1. Toolbar (returns actions + mutates tool/color/thickness)
        let mut changed = false;
        let action = self.toolbar.update(
            ctx,
            &mut self.tool,
            &mut self.color,
            &mut self.thickness,
            page_current,
            page_total,
            &mut changed,
            &self.smart_alpha,
        );

        // 2. 空间索引维护：只在笔迹集合变更后重建（O(n)），不是每帧都做。
        if self.spatial_dirty {
            self.spatial.rebuild(&self.strokes);
            self.spatial_dirty = false;
        }

        // 3. Input processing（橡皮借助索引只对候选笔迹做距离测试）
        let (completed, erased) = self.input_processor.process(
            ctx,
            screen_rect,
            &mut self.current,
            &mut self.strokes,
            &self.tool,
            &self.color,
            self.thickness,
            &self.spatial,
        );

        if let Some(stroke) = completed {
            self.strokes.push(stroke);
            self.modified = true;
            self.spatial_dirty = true;
            self.cache.mark_pending();
            // Merge nearby strokes every ~20 additions (amortised O(n))
            if self.strokes.len().is_multiple_of(20) {
                stroke::merge_adjacent_strokes(&mut self.strokes);
            }
        }
        if erased > 0 {
            self.modified = true;
            self.spatial_dirty = true;
            self.cache.mark_pending();
        }

        // 4. Render
        // 本帧刚发生变更时下标已失效，退回全量遍历；索引在下一帧开头重建。
        let visible: Option<Vec<usize>> = if self.spatial_dirty {
            None
        } else {
            Some(self.spatial.query(screen_rect))
        };
        self.renderer.render(
            ctx,
            &self.strokes,
            self.current.as_ref(),
            visible.as_deref(),
        );

        // 4b. Eraser cursor preview
        self.renderer
            .render_cursor_preview(ctx, &self.tool, self.thickness);

        // 5. Auto-cache flush
        if self.modified && self.cache.should_flush(&self.strokes) {
            if let Err(e) = self.cache.flush(&self.strokes) {
                eprintln!("[annotation] Cache flush failed: {}", e);
            } else {
                self.modified = false;
            }
        }

        action
    }

    pub fn save_patch(&self, doc_path: &Path) -> Result<(), String> {
        if self.strokes.is_empty() {
            return Err("No annotations to save".to_string());
        }

        let mut encoded =
            bincode::serialize(&self.strokes).map_err(|e| format!("Serialize failed: {}", e))?;

        let crc = crc32fast::hash(&encoded);
        encoded.extend_from_slice(&crc.to_le_bytes());

        let patch_path = doc_path.with_extension("drfp");
        std::fs::write(&patch_path, &encoded).map_err(|e| format!("Write failed: {}", e))?;

        eprintln!(
            "[annotation] Saved {} strokes to {:?}",
            self.strokes.len(),
            patch_path
        );
        Ok(())
    }

    /// 把当前页板书批注导出到任意路径（与 `save_patch` 同格式，但路径自定义）。
    /// 仅生成独立文件，绝不修改课件本身，符合学生作答快照防篡改红线。
    pub fn export_to(&self, path: &PathBuf) -> Result<(), String> {
        if self.strokes.is_empty() {
            return Err("当前页暂无板书批注".to_string());
        }
        let mut encoded =
            bincode::serialize(&self.strokes).map_err(|e| format!("Serialize failed: {}", e))?;
        let crc = crc32fast::hash(&encoded);
        encoded.extend_from_slice(&crc.to_le_bytes());
        std::fs::write(path, &encoded).map_err(|e| format!("Write failed: {}", e))?;
        Ok(())
    }

    pub fn clear(&mut self) {
        self.strokes.clear();
        self.current = None;
        self.modified = true;
        self.spatial_dirty = true;
        self.cache.cleanup();
    }

    /// Analyse the courseware page and update smart-alpha recommendations.
    /// Call after loading a document / changing page.
    pub fn analyze_current_page(
        &mut self,
        page_index: usize,
        bg_color: &[u8; 3],
        element_count: usize,
        canvas_width: f32,
        canvas_height: f32,
    ) {
        self.smart_alpha.analyze_page(
            page_index,
            bg_color,
            element_count,
            canvas_width,
            canvas_height,
        );
    }

    pub fn shutdown(&mut self) {
        if !self.strokes.is_empty() {
            let _ = self.cache.flush(&self.strokes);
        }
    }
}
