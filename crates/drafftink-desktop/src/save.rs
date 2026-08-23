//! ENBX 课件导出（保存）。
//!
//! 将当前画布的全部元素（文档层 `Element` + 宿主层叠加层 形状 / 图片 / 视频 / 音频）
//! 序列化为 Seewo `.enbx` 格式（完整 ZIP：
//! `Board.xml` + `Document.xml` + `Reference.xml` + `Slide_N.xml` +
//! `SaveInfoMetadataFile.xml` + `[Content_Types].xml` + `thumbnail.png` +
//! `Resources/<hash>.<ext>` + `SlideThumbnails/Slide_N.png`）。
//!
//! 设计要点：本模块只负责「把已收集的数据装配成 ENBX」，不触碰 `IntegratedApp`
//! 的私有字段——宿主通过 `IntegratedApp::save_bundle()`（在 `app.rs` 内实现，拥有
//! 完整字段访问权）把画布快照打包成 [`SaveBundle`] 这类纯数据结构后传入。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::Local;
use drafftink_core::element::ElementData;
use drafftink_core::model::Element;
use drafftink_enbx::parser::{EnbxAudio, EnbxImage, EnbxShape, EnbxVideo};
use drafftink_enbx::{map_element_to_enbx, EnbxElement, EnbxSlide};
use image::{ImageBuffer, Rgba};

use crate::app::IntegratedApp;

// ── 纯数据快照（由 `IntegratedApp::save_bundle` 填充） ────────────────────────

/// 单页可序列化快照。
pub(crate) struct PageElements {
    pub(crate) doc_elements: Vec<Element>,
    pub(crate) shapes: Vec<ShapeDesc>,
    pub(crate) images: Vec<ImageDesc>,
    pub(crate) videos: Vec<VideoDesc>,
    pub(crate) audios: Vec<AudioDesc>,
}

/// 画布完整快照。
pub(crate) struct SaveBundle {
    pub(crate) pages: Vec<PageElements>,
    pub(crate) page_size: [f32; 2],
    pub(crate) background: [u8; 4],
}

pub(crate) struct ShapeDesc {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) h: f64,
    pub(crate) shape_type: String,
    pub(crate) fill: String,
    pub(crate) stroke: String,
}

pub(crate) struct ImageDesc {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) h: f64,
    pub(crate) path: PathBuf,
    pub(crate) opacity: f64,
}

pub(crate) struct VideoDesc {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) h: f64,
    pub(crate) resource_id: String,
    pub(crate) is_loop: bool,
    pub(crate) is_auto_play: bool,
    pub(crate) volume: f64,
}

pub(crate) struct AudioDesc {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) h: f64,
    pub(crate) resource_id: String,
    pub(crate) is_loop: bool,
    pub(crate) duration_ms: u64,
}

// ── 导出入口 ────────────────────────────────────────────────────────────────

/// 应用版本（写入 `Document.xml` 的 `AppVersion` / `CreatedAppVersion`）。
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 将当前画布序列化为完整 `.enbx` 文件。
///
/// 复用 `drafftink_enbx::generate_enbx_with_resources` 写 `Reference.xml` +
/// `Resources/` + `Slide_N.xml`（与解析器 `parse_enbx` 对称），再通过 `extra_files`
/// 参数追加 `Board.xml` / `Document.xml` / `SaveInfoMetadataFile.xml` /
/// `[Content_Types].xml` / `thumbnail.png` / `SlideThumbnails/Slide_N.png`。
pub(crate) fn save_enbx(app: &IntegratedApp, output_path: &Path) -> Result<()> {
    let bundle = app.save_bundle();
    let mut resources: HashMap<String, Vec<u8>> = HashMap::new();
    let mut slides: Vec<EnbxSlide> = Vec::with_capacity(bundle.pages.len());
    let mut page_ids: Vec<String> = Vec::with_capacity(bundle.pages.len());

    for page in &bundle.pages {
        let mut elements: Vec<EnbxElement> = Vec::new();
        let page_id = uuid::Uuid::new_v4().to_string();
        page_ids.push(page_id);

        // 1) 文档层元素（Shape / Text / Image / Path / SvgShape / Video / Audio）。
        for elem in &page.doc_elements {
            if let Some(e) = map_element_to_enbx(&ElementData::from_legacy(elem.clone())) {
                elements.push(e);
            }
        }

        // 2) 形状叠加层。
        for s in &page.shapes {
            elements.push(EnbxElement::Shape(EnbxShape {
                x: s.x,
                y: s.y,
                width: s.w,
                height: s.h,
                shape_type: s.shape_type.clone(),
                fill_color: s.fill.clone(),
                stroke_color: s.stroke.clone(),
                stroke_width: 3.0,
                geometry_type: None,
                path_data: None,
                line_type: None,
                arrow_head: None,
                arrow_tail: None,
                adjusts: Vec::new(),
            }));
        }

        // 3) 图片叠加层（本地文件内嵌进 Resources/）。
        for im in &page.images {
            let rid = embed_file(&im.path, &mut resources);
            elements.push(EnbxElement::Image(EnbxImage {
                x: im.x,
                y: im.y,
                width: im.w,
                height: im.h,
                resource_id: rid,
                opacity: im.opacity,
            }));
        }

        // 4) 视频叠加层（本地 file:// 视频内嵌进 Resources/；内嵌 hex id 保留原样）。
        for v in &page.videos {
            let rid = embed_resource_id(&v.resource_id, &mut resources);
            elements.push(EnbxElement::Video(EnbxVideo {
                resource_id: rid,
                x: v.x,
                y: v.y,
                width: v.w,
                height: v.h,
                is_loop: v.is_loop,
                is_auto_play: v.is_auto_play,
                volume: v.volume,
                thumbnail_id: None,
            }));
        }

        // 5) 音频叠加层（本地 file:// 音频内嵌进 Resources/；镜像视频嵌入逻辑）。
        for a in &page.audios {
            let rid = embed_resource_id(&a.resource_id, &mut resources);
            elements.push(EnbxElement::Audio(EnbxAudio {
                resource_id: rid,
                x: a.x,
                y: a.y,
                width: a.w,
                height: a.h,
                is_loop: a.is_loop,
                is_auto_play: false, // 宿主层未持久化；解析时默认 false
                volume: 1.0,         // 宿主层未持久化；解析时默认 1.0
                duration_ms: a.duration_ms,
            }));
        }

        slides.push(EnbxSlide {
            size: (bundle.page_size[0] as f64, bundle.page_size[1] as f64),
            background: Some(argb_from_bg(bundle.background)),
            elements,
        });
    }

    // ── 构造完整 ENBX 的固定文件（Board / Document / SaveInfo / [Content_Types] / 缩略图） ──
    let mut extra_files: HashMap<String, Vec<u8>> = HashMap::new();
    extra_files.insert("Board.xml".to_string(), build_board_xml(&page_ids));
    extra_files.insert(
        "Document.xml".to_string(),
        build_document_xml(bundle.pages.len()),
    );
    extra_files.insert(
        "SaveInfoMetadataFile.xml".to_string(),
        build_save_info_metadata_xml(),
    );

    // 收集所有引用的扩展名（用于 [Content_Types].xml）。
    let mut extensions: Vec<String> = vec!["xml".to_string()];
    for name in resources.keys() {
        if let Some(ext) = std::path::Path::new(name).extension().and_then(|e| e.to_str()) {
            let ext = ext.to_lowercase();
            if !extensions.contains(&ext) {
                extensions.push(ext);
            }
        }
    }
    extra_files.insert(
        "[Content_Types].xml".to_string(),
        build_content_types_xml(&extensions),
    );

    // 占位缩略图（蓝色 320×180 PNG；后续可优化为真实画布截图）。
    let thumb = build_placeholder_thumbnail(320, 180);
    extra_files.insert("thumbnail.png".to_string(), thumb);
    for (i, _) in bundle.pages.iter().enumerate() {
        let w = 320u32;
        let h = (180.0 * (bundle.page_size[1] / bundle.page_size[0].max(1.0))).max(1.0) as u32;
        extra_files.insert(
            format!("SlideThumbnails/Slide_{}.png", i + 1),
            build_placeholder_thumbnail(w, h.max(1)),
        );
    }

    drafftink_enbx::generator::generate_enbx_with_resources(
        &slides,
        &resources,
        &extra_files,
        output_path,
    )?;
    log::info!(
        "[enbx] 保存成功: {} ({} 页, {} 资源)",
        output_path.display(),
        slides.len(),
        resources.len()
    );
    Ok(())
}

// ── 资源内嵌 ────────────────────────────────────────────────────────────────

/// 读取本地文件并以 `md5(路径).扩展名` 命名内嵌进 `resources`。
fn embed_file(path: &Path, resources: &mut HashMap<String, Vec<u8>>) -> String {
    match std::fs::read(path) {
        Ok(bytes) => {
            let name = resource_name_for(path);
            resources.insert(name.clone(), bytes);
            name
        }
        Err(e) => {
            log::warn!(
                "[enbx] 读取图片/视频/音频资源失败，跳过内嵌: {} — {e}",
                path.display()
            );
            path.display().to_string()
        }
    }
}

/// 解析 `resource_id`：本地 `file://` 媒体内嵌；其它（hex 内嵌 id / 相对路径）原样保留。
fn embed_resource_id(resource_id: &str, resources: &mut HashMap<String, Vec<u8>>) -> String {
    if let Some(p) = resource_id.strip_prefix("file://") {
        let path = Path::new(p);
        embed_file(path, resources)
    } else {
        resource_id.to_string()
    }
}

/// `md5(路径).扩展名` —— 与既有资源命名约定一致。
fn resource_name_for(path: &Path) -> String {
    let hash = format!("{:x}", md5::compute(path.to_string_lossy().as_bytes()));
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_else(|| "dat".to_string());
    format!("{hash}.{ext}")
}

/// `[r, g, b, a]` → `AARRGGBB` 十六进制（Seewo 背景色约定）。
fn argb_from_bg(c: [u8; 4]) -> String {
    format!("{:02X}{:02X}{:02X}{:02X}", c[3], c[0], c[1], c[2])
}

// ── 完整 ENBX 固定文件生成 ─────────────────────────────────────────────────

/// 构造 `Board.xml`：画布尺寸 + 页面 ID 列表 + 默认主题。
fn build_board_xml(page_ids: &[String]) -> Vec<u8> {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    out.push_str("<Board>\n");
    out.push_str("  <SlideWidth>1280</SlideWidth>\n");
    out.push_str("  <SlideHeight>720</SlideHeight>\n");
    out.push_str("  <Slides>\n");
    for id in page_ids {
        out.push_str(&format!("    <Item>{}</Item>\n", xml_escape_text(id)));
    }
    out.push_str("  </Slides>\n");
    out.push_str("  <ThemeForBoard><ThemeId>-1</ThemeId></ThemeForBoard>\n");
    out.push_str("</Board>\n");
    out.into_bytes()
}

/// 构造 `Document.xml`：标题、作者、版本、时间戳（最小可用版，不含
/// `DocumentExtraInfo` / `CoursewareSourceTrace` 等高级字段，解析器只读核心字段）。
fn build_document_xml(page_count: usize) -> Vec<u8> {
    let now = Local::now().format("%m/%d/%Y %H:%M:%S").to_string();
    let title = format!("drafftink 课件 ({page_count} 页)");
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    out.push_str("<Document>\n");
    out.push_str(&format!("  <Name>{}</Name>\n", xml_escape_text(&title)));
    out.push_str("  <Creator>drafftink</Creator>\n");
    out.push_str("  <LastModifiedBy>drafftink</LastModifiedBy>\n");
    out.push_str(&format!("  <CreatedDateTime>{now}</CreatedDateTime>\n"));
    out.push_str(&format!("  <ModifiedDateTime>{now}</ModifiedDateTime>\n"));
    out.push_str("  <CreatedDocumentVersion>1.0</CreatedDocumentVersion>\n");
    out.push_str("  <DocumentVersion>1.0</DocumentVersion>\n");
    out.push_str(&format!(
        "  <CreatedAppVersion>{APP_VERSION}</CreatedAppVersion>\n"
    ));
    out.push_str(&format!("  <AppVersion>{APP_VERSION}</AppVersion>\n"));
    out.push_str("</Document>\n");
    out.into_bytes()
}

/// 构造 `SaveInfoMetadataFile.xml`：固定 MetadataContract 列表（与样本最小集对齐）。
fn build_save_info_metadata_xml() -> Vec<u8> {
    let names = [
        ("Board", "Unset", ""),
        ("ThemeForBoard", "Unset", ""),
        ("Slide", "Unset", ""),
        ("Picture", "Element", "Fallback"),
        ("PictureStyle", "Unset", ""),
        ("PictureMetaData", "Unset", ""),
        ("SaveInfoMetadata", "Unset", ""),
        ("ThemeForSlide", "Unset", ""),
        ("Video", "Unset", ""),
        ("ElementBehavior", "Unset", ""),
        ("Document", "Unset", ""),
        ("DocumentExtraInfo", "Unset", ""),
        ("CoursewareSourceTraceInfo", "Unset", ""),
        ("DocumentEditTrace", "Unset", ""),
        ("Reference", "Unset", ""),
        ("Relationship", "Unset", ""),
    ];
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    out.push_str("<SaveInfoMetadataFile>\n");
    out.push_str("  <MetadataContract>\n");
    for (name, kind, fallback) in names {
        out.push_str(&format!(
            "    <MetadataContract><SaveInfoName>{name}</SaveInfoName><SaveInfoFriendlyName></SaveInfoFriendlyName><SaveInfoType>{kind}</SaveInfoType><FallbackSaveInfo>{fallback}</FallbackSaveInfo></MetadataContract>\n"
        ));
    }
    out.push_str("  </MetadataContract>\n");
    out.push_str("</SaveInfoMetadataFile>\n");
    out.into_bytes()
}

/// 构造 `[Content_Types].xml`：扩展名 → ContentType 映射（Seewo 用空 ContentType）。
fn build_content_types_xml(extensions: &[String]) -> Vec<u8> {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    out.push_str("<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\n");
    for ext in extensions {
        out.push_str(&format!(
            "  <Default Extension=\"{}\" ContentType=\"\"/>\n",
            xml_escape_text(ext)
        ));
    }
    out.push_str("</Types>\n");
    out.into_bytes()
}

/// 占位缩略图（蓝底 320×180 PNG；后续可优化为真实画布截图）。
fn build_placeholder_thumbnail(w: u32, h: u32) -> Vec<u8> {
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_pixel(w, h, Rgba([60, 100, 160, 255]));
    let mut buf: Vec<u8> = Vec::new();
    if let Err(e) = img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png) {
        log::warn!("[enbx] 缩略图生成失败: {e}（写入空 PNG）");
        return Vec::new();
    }
    buf
}

/// XML 文本转义（属性/文本通用）。
fn xml_escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
