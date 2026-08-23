---
title: ENBX 解析技术报告与架构决策
date: 2026-08-14
status: accepted
---

# 授课端 .enbx 解析技术报告

> 调查范围：`crates/drafftink-desktop/`（含 Teach 模式）。
> **前置澄清**：工作区里**没有 `crates/drafftink-teach/`**——Teach 模式是 `drafftink-desktop` 内部的一个 `AppMode`（`app.rs:36/138/195`），UI 在 `src/ui/teach.rs`。

## 0. 关键背景：工作区里其实有 3+ 套 .enbx 解析器

| # | Crate | 引擎 | 产出模型 | 被谁消费 |
|---|-------|------|----------|----------|
| 1 | **`drafftink-enbx`** | quick-xml 0.31 流式 | `EnbxFile` → `ElementData` | **`drafftink-desktop`（授课/备课）** |
| 2 | `plugins/format_enbx`（cdylib） | 自有 parser | `CoursewareDoc` | `drafftink-display`、`tools/enbx_viewer` |
| 3 | `crates/enbx_importer` | quick-xml | `CoursewareDoc` | （带 zip-bomb / ZipSlip 防护 + 动画解析） |

本报告聚焦 #1（授课端实际走的链路），#2/#3 在 §6 横向对比中说明。

## 1. 解析入口 (Entry Point)

链路从上到下只有一条，Teach 与 Prepare 共用：

```
prepare.rs:56   按钮 "导入 .enbx"
  → prepare.rs:224  handle_import_enbx(app)          // 文件对话框回调
    → enbx/mod.rs:33  import_enbx(path)             // 桌面集成层
      → drafftink_enbx::parse_enbx(path)            // 真正的解析入口
        → parser.rs:247  pub fn parse_enbx(path)     // ZIP 解包 + XML 解析
```

- 处理函数名：`parse_enbx`（`parser.rs:247`）；桌面薄封装 `import_enbx`（`enbx/mod.rs:33`）。
- Teach 模式（`teach.rs`）本身不重复实现解析，而是复用 `import_enbx` 得到的 `Vec<ElementData>` 再上屏。

## 2. 解析引擎 (Engine)

- **第三方库**：`quick-xml = "0.31"`（features = `["serialize"]`），使用流式 `Reader`（`parser.rs:12-13`），前向只读、不把整棵 XML 树驻留内存。
- **没有自定义 FormatEnbx 插件**：desktop 直接把 `drafftink-enbx` 当普通 crate 依赖（`desktop/Cargo.toml:15`）。`format_enbx` 是 **cdylib 插件**（`plugins/format_enbx/`），由 `drafftink-display` / `enbx_viewer` 经 `libloading` 动态加载——**和授课端 desktop 是两套无关实现**。
- 辅助：`zip` 解包；EMU→px `emu_to_px`（`parser.rs:21`）；Y-down→Y-up 用 `flip_y`（`mapper.rs:46`）；未知标签用 `XmlValue` 透明保存以保 round-trip（`parser.rs:39-51`）。

## 3. 数据模型 (Data Model)

解析产物（均定义在 `parser.rs`）：

```rust
// parser.rs:225
pub struct EnbxFile {
    pub slides: Vec<EnbxSlide>,
    pub metadata: EnbxMetadata,
    pub resources: HashMap<String, Vec<u8>>, // id → 原始字节
}

// parser.rs:199
pub struct EnbxSlide {
    pub elements: Vec<EnbxElement>,
    pub background: Option<String>,
    pub size: (f64, f64),
}

// parser.rs:170 —— 只有 5 个变体
pub enum EnbxElement {
    Text(EnbxText),
    Image(EnbxImage),
    Shape(EnbxShape),
    Path(EnbxPath),
    Group(EnbxGroup),
    Unknown(XmlValue), // 未知元素类型，透明保存
}
```

经 `map_element_from_enbx`（`mapper.rs:381`）映射到桌面内部模型 **`drafftink_core::element::ElementData`**（枚举 Text / Image / Shape / Path / SvgShape / Formula / MindMap / Quiz / Cosmos / Geometry，见 `mapper.rs:164-175`）。

主要字段示例：

- `EnbxText { x, y, width, height, content, font_size, font_color, bold, italic }`（`parser.rs:71`）
- `EnbxImage { resource_id, opacity }`
- `EnbxShape { shape_type, fill_color, stroke_color, stroke_width }`
- `EnbxGroup { elements: Vec<EnbxElement> }`

## 4. 元素覆盖率 (Element Coverage)

`parser.rs:460-495` 的标签分派只有 5 个分支：

```rust
"text"                  => EnbxElement::Text(...)
"image"|"picture"|"pic" => EnbxElement::Image(...)
"shape"                 => EnbxElement::Shape(...)
path/freeline/ink       => EnbxElement::Path(...)    // parser.rs:897
"group"                 => EnbxElement::Group(...)
_ => EnbxElement::Unknown(xv)                       // parser.rs:493 ← 兜底
```

而 `map_element_from_enbx`（**导入方向**，授课端实际走的路径）只映射 5 类且 `Unknown ⇒ None` 直接丢弃（`mapper.rs:381-390`）；`Group` 仅映射成包围盒 `ShapeElement`，**子元素全部丢失**（`mapper.rs:387,497`）。

### 对照 migrator V5 成果

| 元素 | 授课端 `drafftink-enbx` | migrator V5 |
|---|---|---|
| Text | ✅ | ✅ |
| Image / Picture | ✅ | ✅ `WbImage` |
| Shape | ✅ | ✅ `WbShape` |
| Path / 自由线 | ✅ | （形状降级） |
| Video | ❌ `Unknown`→丢弃 | ✅ V3 |
| 3D `Cylinder` / `Cone` | ❌ | ✅ V4 |
| Activity / Classify | ❌ | ✅ V4 |
| **Topic（思维导图）** | ❌ **`Unknown`→丢弃** | ✅ V5 |
| Group | ⚠️ 仅包围盒 | — |

> **重点结论**：授课端解析器**完全不解析 Topic 与 Activity**。它们落入 `Unknown(XmlValue)` 后，在 `map_element_from_enbx` 被返回 `None` 静默丢弃。老师用桌面端打开含思维导图 / 课堂活动的希沃课件时，这些元素会**无声消失**，且没有任何警告日志。

## 5. 渲染消费 (Rendering)

- **渲染方式：egui 原语，而非自定义 wgpu pipeline。** desktop 用 eframe + wgpu，但元素是用 `ui.painter()` 直接画的：`teach.rs:160/171/201`（`rect_filled` / `text` / `line_segment`）、`prepare.rs:107-142`、`grade.rs:208+` 同理。wgpu 在这里只是 egui 的光栅化后端。
- **模型 → 屏幕**：`import_enbx` 把每个 slide 的 elements 拍平成单个 `Vec<ElementData>`（`enbx/mod.rs:38-44`），各 view 遍历 `app.elements` / `app.slides`，按 `app.selected_slide` 索引区分页面，用 painter 绘制。坐标在 1280×720 / 1920×1080 视口下映射。
- 注意：`map_element_from_enbx` 产出的是 `ElementData`，desktop 并未消费 migrator 的 `WhiteboardDoc`——两套渲染 / 模型无交集。

## 6. 横向对比 & 关键发现

### (a) migrator 与 desktop 的解析链路是「解耦且重复」的

- desktop 直接调 `drafftink_enbx::parse_enbx`（同一 crate）。
- **migrator 的 `Cargo.toml` 显示零依赖（仅 serde / serde_json），根本不依赖 `drafftink-enbx`！** 它的 `enbx_model.rs` 是**自有的** `EnbxParsed` / `EnbxElement` 模型，且 migrator **不解析 XML 本身**（无 quick-xml、无 ZIP、无文件读取），只做内存模型 → `WhiteboardDoc` 的转换（从上游喂入的已解析模型）。换言之，**migrator 当前无法自行读取真实 .enbx 文件**。`from_enbx(parsed, …)` 接收的是已构造好的 `EnbxParsed`。
- 两者各自维护一套 `EnbxElement` 模型，字段约定不一致（f32 vs f64、`color` 表示等），无共享类型。

### (b) 还有第三 / 第四套实现

`plugins/format_enbx` 与 `crates/enbx_importer` 又是完全独立的解析器，且产出**另一个内部模型 `CoursewareDoc`**（`drafftink_core::model`）。

**总结：工作区至少有 3 套完整 .enbx 解析实现 + 2 套内部模型（`ElementData` / `CoursewareDoc`），彼此不共享代码。**

## Architecture Decision Record (ADR-001)

**议题**：migrator 是否应作为 .enbx 解析的唯一中枢（Single Source of Truth）？

**现状问题**

1. 碎片化：3 套解析器、2 套模型，持续分裂。
2. 覆盖倒挂：migrator V4 / V5 已支持 Video / 3D / Activity / **Topic**，但老师真正打开课件的 `drafftink-enbx` 把这些元素当 `Unknown` 静默丢弃。
3. migrator 自身零依赖、不读文件、模型与 `enbx` 不一致，且不能独立吃进 .enbx。

**决策建议：不要把 migrator 设为解析 SSOT。**

- **解析层 SSOT = `drafftink-enbx`**（它已被 desktop 使用、能读 ZIP / XML、模型已是 `ElementData`）。应立即把 migrator V4 / V5 的元素覆盖（Topic / Activity / 3D / Video）反向移植进 `drafftink-enbx` 的 `EnbxElement` 枚举与 `map_element_from_enbx`，消除「老师打开课件丢元素」的静默缺陷。
- **migrator 保持「纯转换 / 降级专门器」角色**：ENBX→drftx 的模型映射，消费 `drafftink-enbx`（或统一的 `enbx_core`）解析出的模型，而不是自己解析文件。这契合其零依赖设计约束，也避免了循环依赖。
- **中期**：合并 `format_enbx` / `enbx_importer` / `drafftink-enbx` 三套实现为一个 `enbx_core` crate，统一到单一内部模型（`ElementData` 或 `CoursewareDoc` 二选一并收敛），并复用 `enbx_importer` 已有的 zip-bomb / ZipSlip 防护。

**理由**：以「老师能否正确打开现有希沃课件」为第一优先级，`drafftink-enbx` 是唯一已接入渲染、且具备文件读取能力的解析器；migrator 的设计目标是转换而非解析，强行让其当中枢会破坏零依赖约束并加剧三套实现的分裂。

---

> 本报告由 drafftink-migrator V5 完成后代码考古生成，驱动 ADR-001 反向移植工作。
