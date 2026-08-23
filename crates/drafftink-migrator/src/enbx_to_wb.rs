//! ENBX → drftx 核心转换逻辑 (V3：支持 SVG Path 提取 + 多媒体资源)。
//!
//! 设计约束：
//! - 无 IWB API：直接构建 `WhiteboardDoc` 内存模型，不做国标 ZIP 打包。
//! - 零依赖（除 serde / serde_json）：XML 解析已由上游 `drafftink-enbx` 完成，本模块只做映射与降级。
//! - V1 降级策略（保持不变）：
//!   * 文本 / 图片：100% 还原 (x, y, w, h) 与内容。
//!   * 形状：提取 SVG `raw_path`；复杂/未知几何降级为 `WbShapeType::Path`。
//!   * 交互 / 动画 / 未知标签：绝不 panic，统一降级为红色虚线占位符。
//! - 多媒体资源（新增）：
//!   * `<Picture>`：经 `Reference::resolve` 反查 `Resources/*.jpg` 并读取字节存入 `media` 字典。
//!   * `<Video>`：同上提取视频本体 + 可选缩略图；drftx V1 不支持播放，降级为占位符并记录日志。
//!   * 文件读取失败一律 `warn!` + 返回占位符，**绝不 panic**。
//! - 坐标校准：希沃坐标可能为负或超大，统一夹取到 `0..=1920 × 0..=1080` 视口。
//! - 颜色解析：希沃 `#AARRGGBB` → drftx `#RRGGBB`（去 Alpha）。
//! - 路径处理：兼容 Windows 反斜杠 `Resources\xxx.jpg` 与 Unix 正斜杠。

use std::collections::HashMap;
use std::path::Path;

use crate::whiteboard::{
    Canvas, MediaAsset, Metadata, MigrationNote, MigrationReport, WbElement, WbImage, WbPage,
    WbPlaceholder, WbShape, WbShapeType, WbText, WhiteboardDoc,
};
use crate::enbx_model::{
    Enbx3dShape, EnbxActivity, EnbxActivityItem, EnbxElement, EnbxParsed, EnbxPicture, EnbxTopic,
    EnbxVideo, ImageXml, Reference, ShapeXml, TextXml,
};

/// 希沃标准视口（V3 校准目标）。
const VIEW_W: f64 = 1920.0;
const VIEW_H: f64 = 1080.0;

/// 主转换入口：`EnbxParsed` → `WhiteboardDoc`（默认相对 `Resources` 资源目录）。
///
/// 委托给 [`from_enbx`]，保留完整内存模型（含 `notes` 迁移日志与 `media` 媒体字典）。
impl From<EnbxParsed> for WhiteboardDoc {
    fn from(parsed: EnbxParsed) -> Self {
        from_enbx(&parsed, Path::new("Resources"))
    }
}

/// 主转换入口（带资源目录）。
///
/// `resources_dir` 指向 ENBX 解包根目录（其下含 `Reference.xml` 与 `Resources/` 子目录）。
/// `Reference.Target` 是相对该根目录的路径（可能含 `\` 或 `/`）。
pub fn from_enbx(parsed: &EnbxParsed, resources_dir: &Path) -> WhiteboardDoc {
    convert(parsed, resources_dir)
}

/// 执行完整转换，构建 `WhiteboardDoc`。
pub fn convert(parsed: &EnbxParsed, resources_dir: &Path) -> WhiteboardDoc {
    let mut logs: Vec<String> = Vec::new();
    let mut migration_notes: Vec<MigrationNote> = Vec::new();
    let (cw, ch) = calibrate_canvas(parsed.board.width, parsed.board.height);
    let mut media: HashMap<String, MediaAsset> = HashMap::new();
    let mut pages: Vec<WbPage> = Vec::with_capacity(parsed.slides.len());

    for (i, slide) in parsed.slides.iter().enumerate() {
        let mut elements: Vec<WbElement> = Vec::with_capacity(slide.elements.len());

        for elem in &slide.elements {
            // 媒体类 / 3D / 课堂活动类元素需要额外上下文（reference / 资源目录 / media / notes），在此显式分发。
            let produced: Vec<WbElement> = match elem {
                EnbxElement::Picture(pic) => {
                    vec![convert_picture(pic, &parsed.reference, resources_dir, &mut media)]
                }
                EnbxElement::Video(vid) => vec![convert_video(
                    vid,
                    &parsed.reference,
                    resources_dir,
                    &mut media,
                    &mut logs,
                    i as u32,
                )],
                EnbxElement::Image(img) => vec![WbElement::Image(convert_image(img, &mut media))],
                EnbxElement::Cylinder(c) => {
                    logs.push(format!("[slide {i}] 3D 形状 <Cylinder> 降级为占位符"));
                    vec![convert_3d_shape("Cylinder", c, &mut migration_notes, i as u32)]
                }
                EnbxElement::Cone(c) => {
                    logs.push(format!("[slide {i}] 3D 形状 <Cone> 降级为占位符"));
                    vec![convert_3d_shape("Cone", c, &mut migration_notes, i as u32)]
                }
                EnbxElement::ActivityItem(item) => {
                    let els = convert_activity_item(
                        item,
                        &parsed.reference,
                        resources_dir,
                        &mut media,
                        &mut migration_notes,
                        i as u32,
                    );
                    logs.push(format!(
                        "[slide {i}] 课堂活动素材已转换 ({} 个子元素)",
                        els.len()
                    ));
                    els
                }
                EnbxElement::Activity(act) => {
                    logs.push(format!("[slide {i}] 课堂活动 <{}> 降级为占位符", act.key));
                    vec![convert_activity(act, &mut migration_notes, i as u32)]
                }
                EnbxElement::Topic(t) => {
                    logs.push(format!("[slide {i}] 思维导图展开为独立文本节点"));
                    convert_topic(t, &mut logs)
                }
                // 文本 / 形状 / 未知标签走通用分支（含占位符降级）。
                other => match convert_element(other) {
                    Some(WbElement::Placeholder(p)) => {
                        logs.push(format!("[slide {i}] placeholder inserted: {}", p.reason));
                        vec![WbElement::Placeholder(p)]
                    }
                    Some(o) => vec![o],
                    None => {
                        let (x, y, w, h) = calibrate(0.0, 0.0, 200.0, 100.0);
                        logs.push(format!(
                            "[slide {i}] element dropped (no recoverable geometry), inserted placeholder"
                        ));
                        vec![WbElement::Placeholder(WbPlaceholder {
                            reason: "element dropped: no recoverable geometry".to_string(),
                            x,
                            y,
                            w,
                            h,
                        })]
                    }
                },
            };

            elements.extend(produced);
        }

        let thumbnail = parsed.thumbnails.get(&i).cloned();
        pages.push(WbPage {
            index: i,
            elements,
            thumbnail,
        });
    }

    logs.push(format!(
        "migration finished: {} slide(s) converted, {} media asset(s), {} migration note(s)",
        pages.len(),
        media.len(),
        migration_notes.len()
    ));

    WhiteboardDoc {
        metadata: Metadata {
            title: parsed.board.name.clone(),
            source: "enbx".to_string(),
            generator: "drafftink-migrator/4.0".to_string(),
        },
        canvas: Canvas {
            width: cw,
            height: ch,
            background: "#FFFFFF".to_string(),
        },
        pages,
        notes: logs,
        migration_notes,
        media,
    }
}

/// 单个元素转换器（文本 / 形状 / 未知）。
///
/// 返回 `Option<WbElement>`：
/// - `Some` 表示成功映射，或已降级为占位符（V1 下未知 / 交互 / 动画元素一律转为占位符，不崩溃）。
/// - `None` 仅在元素完全无可用几何信息时返回，调用方会补一个兜底占位符。
///
/// 注意：图片 / `<Picture>` / `<Video>` 等资源类元素由调用方带 `media` 上下文单独处理，
/// 若直接以这些变体调用本函数，会返回 `None`（由调用方兜底为占位符），不会崩溃。
pub fn convert_element(elem: &EnbxElement) -> Option<WbElement> {
    match elem {
        EnbxElement::Text(t) => Some(WbElement::Text(convert_text(t))),
        EnbxElement::Shape(s) => Some(WbElement::Shape(convert_shape(s))),
        // 未知标签（含 <seewo:xxx>、动画等）：降级为红色虚线占位符。
        EnbxElement::Unknown(tag) => {
            let (x, y, w, h) = calibrate(0.0, 0.0, 240.0, 120.0);
            Some(WbElement::Placeholder(WbPlaceholder {
                reason: format!("unsupported element <{tag}> downgraded to placeholder"),
                x,
                y,
                w,
                h,
            }))
        }
        // 资源类 / 3D / 课堂活动 / 思维导图类元素不应经此路径进入（由外层 convert 显式分发）。
        EnbxElement::Image(_)
        | EnbxElement::Picture(_)
        | EnbxElement::Video(_)
        | EnbxElement::Cylinder(_)
        | EnbxElement::Cone(_)
        | EnbxElement::ActivityItem(_)
        | EnbxElement::Activity(_)
        | EnbxElement::Topic(_) => None,
    }
}

/// 文本转换：100% 还原内容 + 首个 run 的样式（字体 / 大小 / 颜色）。
pub fn convert_text(t: &TextXml) -> WbText {
    let (x, y, w, h) = calibrate(t.x, t.y, t.w, t.h);
    let (font, size, color) = match t.runs.first() {
        Some(r) => (r.font.clone(), r.size, r.color.clone()),
        None => ("sans-serif".to_string(), 24.0, "#000000".to_string()),
    };
    WbText {
        content: t.content.clone(),
        font,
        size,
        color,
        x,
        y,
        w,
        h,
    }
}

/// 内嵌图片转换：还原位置与媒体引用，并将 base64 / src 登记进 media 字典（同 id 去重）。
pub fn convert_image(img: &ImageXml, media: &mut HashMap<String, MediaAsset>) -> WbImage {
    let (x, y, w, h) = calibrate(img.x, img.y, img.w, img.h);
    let media_id = if img.src.is_empty() {
        format!(
            "media_{:016x}",
            fnv1a(img.data.as_deref().unwrap_or("").as_bytes())
        )
    } else {
        img.src.clone()
    };
    media.entry(media_id.clone()).or_insert_with(|| {
        let data = img
            .data
            .as_deref()
            .and_then(base64_decode)
            .unwrap_or_default();
        MediaAsset {
            filename: img.src.clone(),
            mime: mime_from_ext(&img.src),
            data,
        }
    });
    WbImage {
        media_id,
        src: img.src.clone(),
        x,
        y,
        w,
        h,
    }
}

/// `<Picture>` 转换：经 `Reference` 反查资源路径，读取二进制并存入 `media`，返回 `WbImage`。
///
/// 任何失败（引用未找到 / 文件读取失败）都打 `warn!` 并返回占位符，**绝不 panic**。
pub fn convert_picture(
    pic: &EnbxPicture,
    reference: &Reference,
    resources_dir: &Path,
    media: &mut HashMap<String, MediaAsset>,
) -> WbElement {
    let (x, y, w, h) = calibrate(pic.x, pic.y, pic.width, pic.height);
    match reference.resolve(&pic.source) {
        Some(mref) => {
            let full = resources_dir.join(normalize_sep(&mref.target));
            match std::fs::read(&full) {
                Ok(bytes) => {
                    media.insert(
                        mref.id.clone(),
                        MediaAsset {
                            filename: mref.filename.clone(),
                            mime: mime_from_ext(&mref.extension),
                            data: bytes,
                        },
                    );
                    WbElement::Image(WbImage {
                        media_id: mref.id.clone(),
                        src: mref.target.clone(),
                        x,
                        y,
                        w,
                        h,
                    })
                }
                Err(e) => {
                    warn(&format!("图片读取失败 {}: {e}", mref.target));
                    placeholder(&format!("图片读取失败: {}", pic.picture_name), x, y, w, h)
                }
            }
        }
        None => {
            warn(&format!("图片引用未解析: {}", pic.source));
            placeholder(&format!("图片引用未解析: {}", pic.source), x, y, w, h)
        }
    }
}

/// `<Video>` 转换：提取视频本体 + 可选缩略图字节存入 `media`，但 drftx V1 不支持播放，
/// 故降级为占位符并在 `notes` 追加一条说明。文件读取失败只 `warn!` 不崩溃。
pub fn convert_video(
    vid: &EnbxVideo,
    reference: &Reference,
    resources_dir: &Path,
    media: &mut HashMap<String, MediaAsset>,
    notes: &mut Vec<String>,
    page_index: u32,
) -> WbElement {
    let (x, y, w, h) = calibrate(vid.x, vid.y, vid.width, vid.height);

    if let Some(mref) = reference.resolve(&vid.source) {
        let full = resources_dir.join(normalize_sep(&mref.target));
        match std::fs::read(&full) {
            Ok(bytes) => {
                media.insert(
                    mref.id.clone(),
                    MediaAsset {
                        filename: mref.filename.clone(),
                        mime: mime_from_ext(&mref.extension),
                        data: bytes,
                    },
                );
            }
            Err(e) => warn(&format!("视频读取失败 {}: {e}", mref.target)),
        }

        // 可选缩略图：同样经 Reference 反查并读取。
        if let Some(thumb_source) = &vid.thumbnail {
            if let Some(tref) = reference.resolve(thumb_source) {
                let tfull = resources_dir.join(normalize_sep(&tref.target));
                match std::fs::read(&tfull) {
                    Ok(tbytes) => {
                        media.insert(
                            tref.id.clone(),
                            MediaAsset {
                                filename: tref.filename.clone(),
                                mime: mime_from_ext(&tref.extension),
                                data: tbytes,
                            },
                        );
                    }
                    Err(e) => warn(&format!("视频缩略图读取失败 {}: {e}", tref.target)),
                }
            }
        }
    } else {
        warn(&format!("视频引用未找到: {}", vid.source));
    }

    notes.push(format!(
        "[slide {page_index}] 视频已提取但 drftx V1 不支持播放: {} (loop={}, auto_play={})",
        vid.media_name, vid.is_loop, vid.is_auto_play
    ));

    WbElement::Placeholder(WbPlaceholder {
        reason: format!("视频: {}", vid.media_name),
        x,
        y,
        w,
        h,
    })
}

/// `<Cylinder>` / `<Cone>` 3D 形状转换。
///
/// drftx V1 不支持 3D 几何，统一降级为占位符，并写入一条 [`MigrationNote`]。
/// 坐标由 `f32` cast 为 `f64` 后做视口校准（与全工程一致）。
pub fn convert_3d_shape(
    tag: &str,
    shape: &Enbx3dShape,
    notes: &mut Vec<MigrationNote>,
    page_index: u32,
) -> WbElement {
    let (x, y, w, h) = calibrate(
        shape.x as f64,
        shape.y as f64,
        shape.width as f64,
        shape.height as f64,
    );
    notes.push(MigrationNote {
        page_index,
        element_type: tag.to_string(),
        detail: format!(
            "3D 形状 <{tag}> 暂不被 drftx V1 支持，已降级为占位符（3D 变换矩阵已丢弃）"
        ),
        suggestion: Some(
            "在 drftx 中用手动绘制的 2D 近似图形替代，或等待后续版本支持 3D 元素".to_string(),
        ),
    });
    placeholder(&format!("3D 形状: {tag}"), x, y, w, h)
}

/// `<ActivityItem>`：课堂活动的容器 / 素材。
///
/// 行为：
/// - 若携带 `background_source`（形如 `id://<id>`），经 `Reference` 反查并读取图片，登记进 `media` 字典，并产出 `WbImage`。
/// - 若携带文本内容（`text_content` 或 `rich_text_content`），产出 `WbText`。
///
/// 因此单个 ActivityItem 可能产出 0 / 1 / 2 个 `WbElement`（故返回 `Vec`）。
/// 任何 I/O 失败仅 `warn!`，对应子元素降级 / 缺失，**绝不 panic**。
pub fn convert_activity_item(
    item: &EnbxActivityItem,
    reference: &Reference,
    resources_dir: &Path,
    media: &mut HashMap<String, MediaAsset>,
    notes: &mut Vec<MigrationNote>,
    page_index: u32,
) -> Vec<WbElement> {
    let (x, y, w, h) = calibrate(
        item.x as f64,
        item.y as f64,
        item.width as f64,
        item.height as f64,
    );
    let mut out: Vec<WbElement> = Vec::new();

    // 背景图片：经 Reference 反查并读取字节。
    if let Some(source) = &item.background_source {
        match reference.resolve(source) {
            Some(mref) => {
                let full = resources_dir.join(normalize_sep(&mref.target));
                match std::fs::read(&full) {
                    Ok(bytes) => {
                        media.insert(
                            mref.id.clone(),
                            MediaAsset {
                                filename: mref.filename.clone(),
                                mime: mime_from_ext(&mref.extension),
                                data: bytes,
                            },
                        );
                        out.push(WbElement::Image(WbImage {
                            media_id: mref.id.clone(),
                            src: mref.target.clone(),
                            x,
                            y,
                            w,
                            h,
                        }));
                    }
                    Err(e) => {
                        warn(&format!("课堂活动素材图片读取失败 {}: {e}", mref.target));
                        notes.push(MigrationNote {
                            page_index,
                            element_type: "ActivityItem".to_string(),
                            detail: format!(
                                "ActivityItem 背景图片读取失败: {} ({})",
                                item.resource_id, e
                            ),
                            suggestion: Some("检查 ENBX 资源是否完整解包".to_string()),
                        });
                        out.push(placeholder(
                            &format!("课堂活动素材图片读取失败: {}", item.resource_id),
                            x,
                            y,
                            w,
                            h,
                        ));
                    }
                }
            }
            None => {
                warn(&format!("课堂活动素材图片引用未解析: {source}"));
                notes.push(MigrationNote {
                    page_index,
                    element_type: "ActivityItem".to_string(),
                    detail: format!(
                        "ActivityItem 背景图片引用未解析: {} ({source})",
                        item.resource_id
                    ),
                    suggestion: Some("检查 Reference.xml 是否包含该资源 id".to_string()),
                });
                out.push(placeholder(
                    &format!("课堂活动素材图片引用未解析: {source}"),
                    x,
                    y,
                    w,
                    h,
                ));
            }
        }
    }

    // 文本：优先 text_content，回退 rich_text_content。
    let text = item
        .text_content
        .clone()
        .or_else(|| item.rich_text_content.clone());
    if let Some(content) = text {
        let color = match parse_argb_color(&item.foreground_color) {
            Some(v) => format!("#{:06X}", v & 0x00FF_FFFF),
            None => "#000000".to_string(),
        };
        let (tx, ty, tw, th) = calibrate(
            x + item.text_offset_x as f64,
            y + item.text_offset_y as f64,
            w,
            h,
        );
        out.push(WbElement::Text(WbText {
            content,
            font: item.font_family.clone(),
            size: item.font_size as f64,
            color,
            x: tx,
            y: ty,
            w: tw,
            h: th,
        }));
    }

    notes.push(MigrationNote {
        page_index,
        element_type: "ActivityItem".to_string(),
        detail: format!(
            "课堂活动素材已转换: type={}, activity_id={}, 子元素数={}",
            item.resource_type,
            item.activity_id,
            out.len()
        ),
        suggestion: None,
    });

    out
}

/// `<Activity>`（如 `<Classify>`）分类课堂活动转换。
///
/// drftx V1 不支持交互式课堂活动，统一降级为占位符。若 `key == "Classify"`，
/// 会在 `detail` 中记录分类数量与首分类名（便于下游确认降级范围）。
pub fn convert_activity(
    activity: &EnbxActivity,
    notes: &mut Vec<MigrationNote>,
    page_index: u32,
) -> WbElement {
    let (x, y, w, h) = calibrate(100.0, 100.0, 600.0, 400.0);
    let classify_count = activity.classifies.len();
    let first = activity
        .classifies
        .first()
        .map(|c| c.name.clone())
        .unwrap_or_default();
    let detail = format!(
        "课堂活动 <{}> (key={}) 降级为占位符：含 {} 个分类{}",
        activity.name,
        activity.key,
        classify_count,
        if activity.key.eq_ignore_ascii_case("Classify") {
            format!("，首分类「{first}」")
        } else {
            String::new()
        }
    );
    notes.push(MigrationNote {
        page_index,
        element_type: "Activity".to_string(),
        detail,
        suggestion: Some(
            "在 drftx 中用手动图形 + 文本重建课堂活动，或等待后续版本支持交互式活动".to_string(),
        ),
    });
    placeholder(
        &format!("课堂活动: {0} - {1}", activity.key, activity.name),
        x,
        y,
        w,
        h,
    )
}

/// 解析 Topic 子节点的 `Location`（相对中心偏移），如 `"290.5,-128"` → `(290.5, -128.0)`。
///
/// 逗号分隔、容忍空白；任一分量解析失败回退 `0.0`，**绝不 panic**。
pub fn parse_location(loc: &str) -> (f64, f64) {
    let mut it = loc.split(',');
    let x = it
        .next()
        .map(|s| s.trim().parse::<f64>().unwrap_or(0.0))
        .unwrap_or(0.0);
    let y = it
        .next()
        .map(|s| s.trim().parse::<f64>().unwrap_or(0.0))
        .unwrap_or(0.0);
    (x, y)
}

/// 从 Topic 的 Title 结构（XML 片段提示）中提取 `<Text>` 纯文本内容。
///
/// 复用轻量标签扫描思想（与 `enbx_model::Reference::from_xml` 的 `tag_text` 一致）。
pub fn extract_text_from_topic_title(title_xml_hint: &str) -> String {
    let open = "<Text>";
    let close = "</Text>";
    match (title_xml_hint.find(open), title_xml_hint.find(close)) {
        (Some(s), Some(e)) if e > s + open.len() => {
            title_xml_hint[s + open.len()..e].trim().to_string()
        }
        _ => String::new(),
    }
}

/// 背景色解析：`#AARRGGBB` / `#RRGGBB` → RGB 的 `u32`（用于填充 / 背景色）。
///
/// 语义与 [`parse_argb_color`] 等价；单独命名以契合"背景色"语义。
pub fn parse_argb_color_bg(s: &str) -> Option<u32> {
    parse_argb_color(s)
}

/// `<Topic>`（思维导图 / 鱼骨图 / 组织结构图）转换。
///
/// 将 Topic 展开为多个独立文本节点：
/// - 中心节点 → 一个 `WbText`（坐标取 Topic 包围盒左上，尺寸取 `content_width/Height`）。
/// - 每个子节点 → 一个 `WbText`，绝对坐标 = Topic 中心 + `Location` 偏移。
///
/// 所有坐标经 `calibrate()` 校准。若中心与所有子节点文本均为空，返回单个占位符
/// （绝不 panic）。
pub fn convert_topic(topic: &EnbxTopic, notes: &mut Vec<String>) -> Vec<WbElement> {
    notes.push(format!(
        "Topic 类型: {}，连线: {}，子节点数: {}",
        topic.topic_type, topic.branch_type, topic.children.len()
    ));

    let all_empty = topic.center_text.trim().is_empty()
        && topic.children.iter().all(|c| c.text.trim().is_empty());
    if all_empty {
        let (x, y, w, h) = calibrate(topic.x, topic.y, topic.width, topic.height);
        return vec![WbElement::Placeholder(WbPlaceholder {
            reason: "空思维导图".to_string(),
            x,
            y,
            w,
            h,
        })];
    }

    let mut out: Vec<WbElement> = Vec::new();

    // 中心节点文本。
    if !topic.center_text.trim().is_empty() {
        let (x, y, w, h) = calibrate(
            topic.x,
            topic.y,
            topic.content_width,
            topic.content_height,
        );
        out.push(WbElement::Text(WbText {
            content: topic.center_text.clone(),
            font: "sans-serif".to_string(),
            size: topic.center_font_size,
            color: parse_argb_color(&topic.center_color)
                .map(|v| format!("#{v:06X}"))
                .unwrap_or_else(|| "#000000".to_string()),
            x,
            y,
            w,
            h,
        }));
    }

    // 子节点：绝对坐标 = Topic 中心 + Location 偏移。
    for child in &topic.children {
        if child.text.trim().is_empty() {
            continue;
        }
        let (ox, oy) = parse_location(&child.location);
        let (x, y, w, h) = calibrate(
            topic.x + ox,
            topic.y + oy,
            child.content_width,
            child.content_height,
        );
        out.push(WbElement::Text(WbText {
            content: child.text.clone(),
            font: "sans-serif".to_string(),
            size: child.font_size,
            color: parse_argb_color(&child.color)
                .map(|v| format!("#{v:06X}"))
                .unwrap_or_else(|| "#000000".to_string()),
            x,
            y,
            w,
            h,
        }));
    }

    out
}

/// 形状转换：提取 SVG Path、几何类型与样式，并分类 `shape_type`。
///
/// - 简单形状（Rectangle / Circle / Triangle 等）→ 具名 `WbShapeType` 枚举。
/// - 其他（Star / Love / 任意不规则路径）→ `WbShapeType::Path(raw_path)`。
/// - 颜色统一经 [`parse_color`] 去掉 Alpha（`#AARRGGBB` → `#RRGGBB`）。
pub fn convert_shape(s: &ShapeXml) -> WbShape {
    let (x, y, w, h) = calibrate(s.x, s.y, s.w, s.h);
    let fill = s.fill.as_deref().and_then(parse_color);
    let stroke = s.stroke.as_deref().and_then(parse_color);
    let shape_type = classify_geometry(&s.geometry_type);
    WbShape {
        x,
        y,
        w,
        h,
        raw_path: s.raw_path.clone(),
        geometry_type: s.geometry_type.clone(),
        shape_type,
        fill,
        stroke,
        stroke_width: s.stroke_width,
        opacity: s.opacity,
    }
}

/// 由 ENBX `GeometryType` 分类形状类型。
fn classify_geometry(geom: &str) -> WbShapeType {
    match geom.to_ascii_lowercase().as_str() {
        "rectangle" | "rect" => WbShapeType::Rectangle,
        "circle" => WbShapeType::Circle,
        "ellipse" | "oval" => WbShapeType::Ellipse,
        "triangle" => WbShapeType::Triangle,
        "line" => WbShapeType::Line,
        "polygon" => WbShapeType::Polygon,
        // 其余（Star / Love / 任意路径）统一走 SVG Path。
        _ => WbShapeType::Path(String::new()),
    }
}

/// 颜色解析：希沃 `#AARRGGBB` → `#RRGGBB`（去掉 Alpha）。
///
/// 已是 `#RRGGBB` 或非 `#` 前缀（如命名色 "red"）则原样返回。
pub fn parse_color(s: &str) -> Option<String> {
    let s = s.trim();
    if !s.starts_with('#') {
        return Some(s.to_string());
    }
    let hex = &s[1..];
    if hex.len() == 8 {
        // #AARRGGBB -> #RRGGBB
        return Some(format!("#{}", &hex[2..8]));
    }
    // 已是 #RRGGBB 或无法识别，原样返回。
    Some(s.to_string())
}

/// 颜色解析：把 ARGB / RGB 十六进制串解析为 `u32`（0xAARRGGBB / 0xRRGGBB）。
///
/// 支持前缀 `#` / `0x`（或无前缀），长度 6 或 8 位；非法输入返回 `None`。
/// 6 位视为 RGB，自动补 `FF` Alpha；8 位按 AARRGGBB 解释。
/// 与 [`parse_color`]（返回 `#RRGGBB` 字符串）互补：本函数用于需要数值表示的场景
/// （如将希沃 `foreground_color` 透传为数值后再转回字符串）。
pub fn parse_argb_color(s: &str) -> Option<u32> {
    let s = s.trim();
    let hex = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .or_else(|| s.strip_prefix('#'))
        .unwrap_or(s)
        .trim();
    if hex.is_empty() || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    match hex.len() {
        6 => u32::from_str_radix(hex, 16).ok(),
        // #AARRGGBB -> #RRGGBB（剥离 Alpha，返回 RGB 的 u32）。
        8 => u32::from_str_radix(hex, 16).ok().map(|v| v & 0x00FF_FFFF),
        _ => None,
    }
}

/// 生成 JSON 迁移报告（由 `WhiteboardDoc` 推导，不依赖外部状态）。
pub fn generate_report(doc: &WhiteboardDoc) -> String {
    let mut total = 0usize;
    let mut placeholders = 0usize;
    for page in &doc.pages {
        for el in &page.elements {
            total += 1;
            if matches!(el, WbElement::Placeholder(_)) {
                placeholders += 1;
            }
        }
    }
    let report = MigrationReport {
        total_elements: total,
        success_count: total.saturating_sub(placeholders),
        placeholders,
        logs: doc.notes.clone(),
    };
    serde_json::to_string_pretty(&report).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
}

// ----------------------------- 内部工具 -----------------------------

/// 夹取单个浮点：非法（NaN / Inf）时回退到 `lo`。
fn clampf(v: f64, lo: f64, hi: f64) -> f64 {
    if !v.is_finite() {
        lo
    } else {
        v.max(lo).min(hi)
    }
}

/// 坐标校准：把 (x, y, w, h) 夹取到视口内，并保证 w / h 为正。
fn calibrate(x: f64, y: f64, w: f64, h: f64) -> (f64, f64, f64, f64) {
    let x = clampf(x, 0.0, VIEW_W - 1.0);
    let y = clampf(y, 0.0, VIEW_H - 1.0);
    let w = clampf(w, 1.0, VIEW_W - x);
    let h = clampf(h, 1.0, VIEW_H - y);
    (x, y, w, h)
}

/// 画布尺寸校准：非法 / 超大时回退到标准视口。
fn calibrate_canvas(w: f64, h: f64) -> (f64, f64) {
    let w = if w.is_finite() && w > 0.0 {
        w.min(VIEW_W)
    } else {
        VIEW_W
    };
    let h = if h.is_finite() && h > 0.0 {
        h.min(VIEW_H)
    } else {
        VIEW_H
    };
    (w, h)
}

/// 64 位 FNV-1a 哈希，用于无 src 时给内嵌图片生成稳定 media_id。
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// 把 `target` 中的 Windows 反斜杠统一为 `/`，便于 `Path::join` 跨平台拼接。
fn normalize_sep(p: &str) -> String {
    p.replace('\\', "/")
}

/// 由扩展名推断 MIME 类型。
fn mime_from_ext(ext: &str) -> String {
    match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "mkv" => "video/x-matroska",
        "mp4" => "video/mp4",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        "wmv" => "video/x-ms-wmv",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    }
    .to_string()
}

/// 构造一个占位符元素（红色虚线边框语义，由序列化/渲染层体现）。
fn placeholder(reason: &str, x: f64, y: f64, w: f64, h: f64) -> WbElement {
    WbElement::Placeholder(WbPlaceholder {
        reason: reason.to_string(),
        x,
        y,
        w,
        h,
    })
}

/// 轻量 warn 日志：写入 stderr（不引入 `log` 等外部依赖，满足零新增依赖约束）。
///
/// 既满足"文件读取失败时打 warn 日志"的诉求，又保持 crate 仅依赖 serde / serde_json。
fn warn(msg: &str) {
    eprintln!("[drafftink-migrator WARN] {msg}");
}

/// 标准 Base64 编码（无填充之外的特性需求，手写零依赖实现）。
pub(crate) fn base64_encode(data: &[u8]) -> String {
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64[((n >> 18) & 63) as usize] as char);
        out.push(B64[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// 标准 Base64 解码（用于内嵌图片 base64 文本 → 字节）。忽略空白与非法字符。
pub(crate) fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    let mut out: Vec<u8> = Vec::new();
    for c in s.trim().bytes() {
        if c == b'=' {
            break;
        }
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enbx_model::{
        BoardXml, Enbx3dShape, EnbxActivity, EnbxActivityItem, EnbxClassify, EnbxClassifyItem,
        EnbxElement, EnbxParsed, EnbxTopic, EnbxTopicNode, Reference, ShapeXml, SlideXml,
    };
    use crate::whiteboard::{WbElement, WbShapeType};
    use std::collections::HashMap;

    /// Slide_5：Rectangle，验证 Path 提取 + Alpha 剥离。
    fn slide5() -> EnbxParsed {
        EnbxParsed {
            board: BoardXml {
                name: "Slide_5".into(),
                width: 1920.0,
                height: 1080.0,
            },
            slides: vec![SlideXml {
                id: "s5".into(),
                elements: vec![EnbxElement::Shape(ShapeXml {
                    geometry_type: "Rectangle".into(),
                    raw_path: "M0,0 L100,0 L100,50 L0,50 Z".into(),
                    x: 10.0,
                    y: 20.0,
                    w: 100.0,
                    h: 50.0,
                    fill: Some("#FF00FF00".into()),
                    stroke: Some("#FFFF0000".into()),
                    stroke_width: 2.0,
                    opacity: 1.0,
                })],
            }],
            thumbnails: HashMap::new(),
            reference: Reference::default(),
        }
    }

    /// Slide_12：Star + Love，验证非简单形状走 Path。
    fn slide12() -> EnbxParsed {
        EnbxParsed {
            board: BoardXml {
                name: "Slide_12".into(),
                width: 1920.0,
                height: 1080.0,
            },
            slides: vec![SlideXml {
                id: "s12".into(),
                elements: vec![
                    EnbxElement::Shape(ShapeXml {
                        geometry_type: "Star".into(),
                        raw_path:
                            "M50,0 L61,35 L98,35 L68,57 L79,91 L50,70 L21,91 L32,57 L2,35 L39,35 Z"
                                .into(),
                        x: 0.0,
                        y: 0.0,
                        w: 100.0,
                        h: 100.0,
                        fill: Some("#FFFFD700".into()),
                        stroke: None,
                        stroke_width: 0.0,
                        opacity: 1.0,
                    }),
                    EnbxElement::Shape(ShapeXml {
                        geometry_type: "Love".into(),
                        raw_path:
                            "M50,30 C50,10 20,10 20,35 C20,60 50,80 50,80 C50,80 80,60 80,35 C80,10 50,10 50,30 Z"
                                .into(),
                        x: 200.0,
                        y: 0.0,
                        w: 100.0,
                        h: 80.0,
                        fill: Some("#FFFF69B4".into()),
                        stroke: None,
                        stroke_width: 0.0,
                        opacity: 0.9,
                    }),
                ],
            }],
            thumbnails: HashMap::new(),
            reference: Reference::default(),
        }
    }

    #[test]
    fn slide5_rectangle_extracts_path_and_strips_alpha() {
        let doc = convert(&slide5(), Path::new("Resources"));
        let shape = match &doc.pages[0].elements[0] {
            WbElement::Shape(s) => s,
            _ => panic!("expected shape"),
        };
        assert_eq!(shape.raw_path, "M0,0 L100,0 L100,50 L0,50 Z");
        assert_eq!(shape.geometry_type, "Rectangle");
        assert_eq!(shape.shape_type, WbShapeType::Rectangle);
        assert_eq!(shape.fill.as_deref(), Some("#00FF00"));
        assert_eq!(shape.stroke.as_deref(), Some("#FF0000"));
        assert_eq!(shape.stroke_width, 2.0);
        assert_eq!(shape.opacity, 1.0);
    }

    #[test]
    fn slide12_star_and_love_use_path() {
        let doc = convert(&slide12(), Path::new("Resources"));
        let els = &doc.pages[0].elements;
        assert_eq!(els.len(), 2);
        for el in els {
            match el {
                WbElement::Shape(s) => {
                    assert!(!s.raw_path.is_empty(), "raw_path must be extracted");
                    assert!(matches!(s.shape_type, WbShapeType::Path(_)));
                }
                _ => panic!("expected shape"),
            }
        }
        let star = match &els[0] {
            WbElement::Shape(s) => s,
            _ => unreachable!(),
        };
        assert_eq!(star.fill.as_deref(), Some("#FFD700"));
        let love = match &els[1] {
            WbElement::Shape(s) => s,
            _ => unreachable!(),
        };
        assert_eq!(love.fill.as_deref(), Some("#FF69B4"));
        assert_eq!(love.opacity, 0.9);
    }

    #[test]
    fn parse_color_strips_alpha() {
        assert_eq!(parse_color("#FF00FF00").as_deref(), Some("#00FF00"));
        assert_eq!(parse_color("#123456").as_deref(), Some("#123456"));
        assert_eq!(parse_color("red").as_deref(), Some("red"));
    }

    #[test]
    fn base64_roundtrip() {
        let data = b"\x00\x10\xff\xab\xcd\xef";
        let enc = base64_encode(data);
        let dec = base64_decode(&enc).expect("decode");
        assert_eq!(dec, data);
    }

    #[test]
    fn drftx_serialization_prefers_raw_path() {
        let doc = convert(&slide12(), Path::new("Resources"));
        let json = crate::wb_to_drftx::to_drftx(&doc);
        let v: serde_json::Value =
            serde_json::from_str(&json).expect("drftx output must be valid JSON");
        let shapes = &v["pages"][0]["elements"];
        assert_eq!(shapes[0]["type"], "shape");
        assert!(
            shapes[0]["path"].as_str().unwrap().starts_with("M50,0"),
            "raw_path must be serialized as path"
        );
    }

    // ----- V3：多媒体资源测试 -----

    /// 构造一个**唯一**的临时 ENBX 解包根目录，写入 `Resources/<file>`。
    ///
    /// 每个测试传入不同的 `label`，避免并行执行时共享同一目录导致清理相互干扰。
    fn make_resources_root(label: &str, files: &[(&str, &[u8])]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("dftx_test_{}_{}", std::process::id(), label));
        let resources = root.join("Resources");
        std::fs::create_dir_all(&resources).unwrap();
        for (name, bytes) in files {
            std::fs::write(resources.join(name), bytes).unwrap();
        }
        root
    }

    #[test]
    fn picture_loads_from_reference_and_media() {
        let root = make_resources_root("pic", &[("img1.jpg", b"fake-jpeg-bytes")]);
        let reference = Reference::from_xml(
            r#"<SaveInfoMetadataFile><MetadataContract><Relationship><Id>img1</Id><Target>Resources\img1.jpg</Target><Hash>abc123</Hash></Relationship></MetadataContract></SaveInfoMetadataFile>"#,
        )
        .expect("reference parse");
        assert_eq!(reference.relationships.len(), 1);
        assert!(reference.resolve("id://img1").is_some());

        let parsed = EnbxParsed {
            board: BoardXml {
                name: "P".into(),
                width: 1920.0,
                height: 1080.0,
            },
            slides: vec![SlideXml {
                id: "s".into(),
                elements: vec![EnbxElement::Picture(EnbxPicture {
                    source: "id://img1".into(),
                    picture_name: "shot.jpg".into(),
                    x: 227.5,
                    y: 90.0,
                    width: 825.0,
                    height: 540.0,
                    display_region: Some("0,0,1440,2200".into()),
                })],
            }],
            thumbnails: HashMap::new(),
            reference,
        };

        let doc = convert(&parsed, &root);
        match &doc.pages[0].elements[0] {
            WbElement::Image(img) => assert_eq!(img.media_id, "img1"),
            other => panic!("expected image, got {other:?}"),
        }
        assert_eq!(doc.media.len(), 1);
        let asset = doc.media.get("img1").expect("media entry");
        assert_eq!(asset.filename, "img1.jpg");
        assert_eq!(asset.mime, "image/jpeg");
        assert_eq!(asset.data, b"fake-jpeg-bytes");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn picture_missing_reference_downgrades_to_placeholder() {
        let root = make_resources_root("miss", &[]);
        let parsed = EnbxParsed {
            board: BoardXml {
                name: "P".into(),
                width: 1920.0,
                height: 1080.0,
            },
            slides: vec![SlideXml {
                id: "s".into(),
                elements: vec![EnbxElement::Picture(EnbxPicture {
                    source: "id://nope".into(),
                    picture_name: "x.jpg".into(),
                    x: 10.0,
                    y: 10.0,
                    width: 100.0,
                    height: 100.0,
                    display_region: None,
                })],
            }],
            thumbnails: HashMap::new(),
            reference: Reference::default(),
        };
        let doc = convert(&parsed, &root);
        assert!(matches!(doc.pages[0].elements[0], WbElement::Placeholder(_)));
        assert!(doc.media.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn video_downgrades_to_placeholder_and_logs() {
        let root = make_resources_root("vid", &[
            ("vid1.mkv", b"fake-mkv"),
            ("thumb1.png", b"fake-png"),
        ]);
        let reference = Reference::from_xml(
            r#"<SaveInfoMetadataFile><MetadataContract>
              <Relationship><Id>vid1</Id><Target>Resources\vid1.mkv</Target><Hash>h1</Hash></Relationship>
              <Relationship><Id>thumb1</Id><Target>Resources\thumb1.png</Target><Hash>h2</Hash></Relationship>
            </MetadataContract></SaveInfoMetadataFile>"#,
        )
        .unwrap();

        let parsed = EnbxParsed {
            board: BoardXml {
                name: "V".into(),
                width: 1920.0,
                height: 1080.0,
            },
            slides: vec![SlideXml {
                id: "s".into(),
                elements: vec![EnbxElement::Video(EnbxVideo {
                    source: "id://vid1".into(),
                    media_name: "0001-0250.mkv".into(),
                    thumbnail: Some("id://thumb1".into()),
                    x: 100.0,
                    y: 100.0,
                    width: 640.0,
                    height: 360.0,
                    is_loop: false,
                    is_auto_play: false,
                })],
            }],
            thumbnails: HashMap::new(),
            reference,
        };

        let doc = convert(&parsed, &root);
        match &doc.pages[0].elements[0] {
            WbElement::Placeholder(p) => assert!(p.reason.contains("视频"), "reason={}", p.reason),
            other => panic!("expected placeholder, got {other:?}"),
        }
        // 一条视频相关的迁移说明日志。
        assert!(
            doc.notes.iter().any(|n| n.contains("视频")),
            "notes={:?}",
            doc.notes
        );
        // 视频本体与缩略图均已提取进 media。
        assert!(doc.media.contains_key("vid1"));
        assert!(doc.media.contains_key("thumb1"));
        assert_eq!(doc.media.get("thumb1").unwrap().mime, "image/png");

        let _ = std::fs::remove_dir_all(&root);
    }

    // ----- V4：3D 形状 / 课堂活动 / 课堂活动素材 -----

    #[test]
    fn cylinder_downgrades_to_placeholder() {
        let parsed = EnbxParsed {
            board: BoardXml {
                name: "C".into(),
                width: 1920.0,
                height: 1080.0,
            },
            slides: vec![SlideXml {
                id: "s".into(),
                elements: vec![EnbxElement::Cylinder(Enbx3dShape {
                    x: 100.0f32,
                    y: 100.0f32,
                    width: 200.0f32,
                    height: 200.0f32,
                    transform: Some("1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1".into()),
                })],
            }],
            thumbnails: HashMap::new(),
            reference: Reference::default(),
        };
        let doc = convert(&parsed, Path::new("Resources"));
        match &doc.pages[0].elements[0] {
            WbElement::Placeholder(p) => assert!(p.reason.contains("3D 形状"), "reason={}", p.reason),
            other => panic!("expected placeholder, got {other:?}"),
        }
        // 结构化迁移说明应记录该降级。
        assert!(
            doc.migration_notes
                .iter()
                .any(|n| n.element_type == "Cylinder"),
            "migration_notes={:?}",
            doc.migration_notes
        );
    }

    #[test]
    fn activity_downgrades_to_placeholder_and_logs_classify() {
        let parsed = EnbxParsed {
            board: BoardXml {
                name: "A".into(),
                width: 1920.0,
                height: 1080.0,
            },
            slides: vec![SlideXml {
                id: "s".into(),
                elements: vec![EnbxElement::Activity(EnbxActivity {
                    id: "act1".into(),
                    key: "Classify".into(),
                    name: "词语分类".into(),
                    description: "将词语拖入正确分类".into(),
                    thumbnail_abs_path: None,
                    classifies: vec![EnbxClassify {
                        id: "c1".into(),
                        name: "城市名称".into(),
                        items: vec![
                            EnbxClassifyItem {
                                id: "i1".into(),
                                name: "北京".into(),
                            },
                            EnbxClassifyItem {
                                id: "i2".into(),
                                name: "上海".into(),
                            },
                        ],
                    }],
                })],
            }],
            thumbnails: HashMap::new(),
            reference: Reference::default(),
        };
        let doc = convert(&parsed, Path::new("Resources"));
        match &doc.pages[0].elements[0] {
            WbElement::Placeholder(p) => assert!(p.reason.contains("课堂活动"), "reason={}", p.reason),
            other => panic!("expected placeholder, got {other:?}"),
        }
        // 结构化日志应含 "Classify"。
        assert!(
            doc.migration_notes.iter().any(|n| n.element_type == "Activity"
                && n.detail.contains("Classify")),
            "migration_notes={:?}",
            doc.migration_notes
        );
    }

    #[test]
    fn activity_item_loads_image_into_media() {
        let root = make_resources_root("actitem", &[("bg1.png", b"fake-png-bytes")]);
        let reference = Reference::from_xml(
            r#"<SaveInfoMetadataFile><MetadataContract><Relationship><Id>bg1</Id><Target>Resources\bg1.png</Target><Hash>h</Hash></Relationship></MetadataContract></SaveInfoMetadataFile>"#,
        )
        .expect("reference parse");

        let parsed = EnbxParsed {
            board: BoardXml {
                name: "AI".into(),
                width: 1920.0,
                height: 1080.0,
            },
            slides: vec![SlideXml {
                id: "s".into(),
                elements: vec![EnbxElement::ActivityItem(EnbxActivityItem {
                    resource_type: "Container".into(),
                    activity_id: "act1".into(),
                    resource_id: "res1".into(),
                    background_source: Some("id://bg1".into()),
                    text_content: Some("标题文本".into()),
                    rich_text_content: None,
                    font_size: 24.0f32,
                    font_weight: "Bold".into(),
                    font_style: "Normal".into(),
                    font_family: "Microsoft YaHei".into(),
                    foreground_color: "#FFFF0000".into(),
                    background_color: "#FF00FF00".into(),
                    text_offset_x: 10.0f32,
                    text_offset_y: 10.0f32,
                    text_editor_width: 200.0f32,
                    x: 150.0f32,
                    y: 150.0f32,
                    width: 300.0f32,
                    height: 200.0f32,
                })],
            }],
            thumbnails: HashMap::new(),
            reference,
        };

        let doc = convert(&parsed, &root);
        // 背景图片 + 文本 => 应产出 2 个子元素。
        assert_eq!(
            doc.pages[0].elements.len(),
            2,
            "elements={:?}",
            doc.pages[0].elements
        );
        // media 应新增 1 条。
        assert_eq!(doc.media.len(), 1);
        assert!(doc.media.contains_key("bg1"));
        assert_eq!(doc.media.get("bg1").unwrap().mime, "image/png");
        // 文本颜色应来自 foreground_color 的 RGB 部分。
        match &doc.pages[0].elements[1] {
            WbElement::Text(t) => assert_eq!(t.color, "#FF0000", "color={}", t.color),
            other => panic!("expected text, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&root);
    }

    // ----- V5：Topic（思维导图 / 鱼骨图 / 组织结构图） -----

    #[test]
    fn topic_location_parsing() {
        assert_eq!(parse_location("290.5,-128"), (290.5, -128.0));
        assert_eq!(parse_location("-322,141"), (-322.0, 141.0));
        // 容忍空白与缺失分量。
        assert_eq!(parse_location("10, 20"), (10.0, 20.0));
        assert_eq!(parse_location("bad"), (0.0, 0.0));
        assert_eq!(parse_location(""), (0.0, 0.0));
    }

    #[test]
    fn topic_mindmap_expands_to_center_and_children() {
        let topic = EnbxTopic {
            topic_type: "MindMap".into(),
            branch_type: "Ellipse".into(),
            skin_type: "BlueSkin".into(),
            center_text: "中心主题".into(),
            center_font_size: 32.0,
            center_color: "#FF3F5482".into(),
            center_bg_color: "#FFB1CEFF".into(),
            x: 512.0,
            y: 360.0,
            width: 290.5,
            height: 128.0,
            content_width: 108.0,
            content_height: 46.0,
            children: vec![
                EnbxTopicNode {
                    text: "子节点A".into(),
                    font_size: 20.0,
                    color: "#FF000000".into(),
                    bg_color: "#FFE5EEFF".into(),
                    location: "290.5,-128".into(),
                    content_width: 108.0,
                    content_height: 46.0,
                },
                EnbxTopicNode {
                    text: "子节点B".into(),
                    font_size: 20.0,
                    color: "#FF000000".into(),
                    bg_color: "#FFE5EEFF".into(),
                    location: "-322,141".into(),
                    content_width: 108.0,
                    content_height: 46.0,
                },
            ],
        };

        let mut notes: Vec<String> = Vec::new();
        let els = convert_topic(&topic, &mut notes);

        // 1 中心 + 2 子节点。
        assert_eq!(els.len(), 3, "期望 3 个文本节点, got {els:?}");

        // 首元素为中心节点。
        let center = match &els[0] {
            WbElement::Text(t) => t,
            other => panic!("expected center text, got {other:?}"),
        };
        assert_eq!(center.content, "中心主题");

        // 子节点坐标 = Topic 左上 + Location 偏移（视口内 calibrate 为恒等）。
        let (ox0, oy0) = parse_location(&topic.children[0].location);
        let child0 = match &els[1] {
            WbElement::Text(t) => t,
            other => panic!("expected child0 text, got {other:?}"),
        };
        assert_eq!(child0.x, topic.x + ox0, "child0.x");
        assert_eq!(child0.y, topic.y + oy0, "child0.y");

        let (ox1, oy1) = parse_location(&topic.children[1].location);
        let child1 = match &els[2] {
            WbElement::Text(t) => t,
            other => panic!("expected child1 text, got {other:?}"),
        };
        assert_eq!(child1.x, topic.x + ox1, "child1.x");
        assert_eq!(child1.y, topic.y + oy1, "child1.y");

        // notes 含摘要。
        assert!(
            notes.iter().any(|n| n.contains("Topic 类型: MindMap")),
            "notes={notes:?}"
        );
    }

    #[test]
    fn topic_empty_returns_placeholder() {
        let topic = EnbxTopic {
            topic_type: "MindMap".into(),
            branch_type: "Ellipse".into(),
            skin_type: "BlueSkin".into(),
            center_text: String::new(),
            center_font_size: 0.0,
            center_color: String::new(),
            center_bg_color: String::new(),
            x: 512.0,
            y: 360.0,
            width: 290.5,
            height: 128.0,
            content_width: 108.0,
            content_height: 46.0,
            children: vec![EnbxTopicNode {
                text: String::new(),
                font_size: 0.0,
                color: String::new(),
                bg_color: String::new(),
                location: "0,0".into(),
                content_width: 10.0,
                content_height: 10.0,
            }],
        };
        let mut notes: Vec<String> = Vec::new();
        let els = convert_topic(&topic, &mut notes);
        assert_eq!(els.len(), 1);
        match &els[0] {
            WbElement::Placeholder(p) => assert_eq!(p.reason, "空思维导图"),
            other => panic!("expected placeholder, got {other:?}"),
        }
    }
}
