# 校本白板 Edit 模式 — 实现方案

## 一、模块目录

```
crates/drafftink-core/src/edit/
├── mod.rs          ← AppMode, EditState, 模式切换逻辑 trait / 辅助函数
├── selection.rs    ← hit_test, 选框 (marquee), 选中集合管理
├── inspector.rs    ← 侧边栏 UI: 位置/大小/透明度/动画参数编辑 + 预览按钮

crates/drafftink-app/src/
├── app.rs          ← 集成: 新增 mode/app_mode 字段, Ctrl+E 切换,
│                      render_canvas_area 内 Edit 分支, 蚂蚁线 + handles 绘制
├── render.rs       ← 新增 draw_selection_border, draw_resize_handles
```

## 二、核心结构体

### 2.1 `drafftink-core/src/edit/mod.rs`

```rust
/// Which operating mode the app is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode { Display, Edit }

/// All Edit-mode mutable state lives here so it can be
/// dropped as a single unit when switching back to Display.
#[derive(Debug, Clone, Default)]
pub struct EditState {
    /// Currently selected element IDs (supports multi-select).
    pub selected: HashSet<Uuid>,

    /// Marquee-selection start in **screen** space.
    pub marquee_start: Option<Pos2>,

    /// Marquee-selection end in **screen** space.
    pub marquee_end: Option<Pos2>,

    /// Dash offset for the selection border animation.
    /// Incremented by 1.0 every frame in Edit mode.
    pub dash_offset: f32,

    /// Element being dragged.  None if idle.
    pub dragging: Option<DragState>,

    /// Resize handle being dragged, if any.
    pub resizing: Option<ResizeState>,

    /// Which resize handle is under the cursor (for cursor icon change).
    pub hovered_handle: Option<ResizeHandle>,

    /// True during animation preview (temporarily re-enters Display-like mode).
    pub previewing: bool,
}

pub struct DragState {
    pub element_ids: HashSet<Uuid>,    // all dragged elements (for multi-select)
    pub start_positions: HashMap<Uuid, [f32; 2]>, // world-space positions before drag
    pub drag_start_screen: Pos2,       // mouse position when drag began
}

pub struct ResizeState {
    pub element_id: Uuid,
    pub handle: ResizeHandle,
    pub start_pos: [f32; 2],
    pub start_size: [f32; 2],
    pub drag_start_screen: Pos2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeHandle {
    TopLeft, TopCenter, TopRight,
    MidLeft, MidRight,
    BottomLeft, BottomCenter, BottomRight,
}
```

### 2.2 `drafftink-core/src/edit/selection.rs`

```rust
/// Hit-test: return the *top-most* element whose world-space bounding
/// rect contains `world_pos`.  Elements are iterated in reverse z-order.
pub fn hit_test(elements: &[Element], world_pos: [f32; 2]) -> Option<Uuid> {
    elements.iter()
        .rev()  // highest z-order first
        .find(|e| e.base().visible && e.base().hit_test(world_pos))
        .map(|e| e.id())
}

/// Hit-test within a screen-space marquee rectangle.
/// Returns all element IDs whose bounding rects intersect the marquee.
pub fn marquee_test(
    elements: &[Element],
    camera: &Camera,
    marquee: &Rect,  // screen-space
) -> HashSet<Uuid> {
    elements.iter()
        .filter(|e| {
            let [l, t, r, b] = e.base().world_bounds();
            let tl = camera.world_to_screen([l, t]);
            let br = camera.world_to_screen([r, b]);
            let elem_rect = Rect::from_min_max(tl, br);
            elem_rect.intersects(*marquee)
        })
        .map(|e| e.id())
        .collect()
}
```

### 2.3 `drafftink-core/src/edit/inspector.rs`

```rust
/// Right-side inspector panel.  Operates on `&mut PageContent` and
/// `&mut EditState.selected` so that all mutations go directly into
/// the data source.
pub fn render_inspector(
    ui: &mut egui::Ui,
    page: &mut PageContent,
    edit_state: &mut EditState,
    camera: &Camera,
    page_size: [f32; 2],
    on_preview: &mut dyn FnMut(),
) {
    // If nothing selected → placeholder text
    // If one selected  → position, size, opacity, animation params
    // If multiple     → "N elements selected"
}
```

## 三、数据流

```
┌─────────────────────────────────────────────┐
│                    app.rs                    │
│                                              │
│  mode == Edit                                 │
│   ├─ render_canvas_area                       │
│   │   ├─ Edit input handler (selection/drag)  │
│   │   ├─ render::render_canvas (as normal)    │
│   │   └─ render::draw_selection_overlay       │
│   │       ├─ ant-line border (dash_offset++)  │
│   │       ├─ resize handles                   │
│   │       └─ marquee rectangle                │
│   │                                           │
│   └─ render_inspector (right panel)           │
│       └─ writes → PageContent directly        │
│                                              │
│  mode == Display                              │
│   ├─ player.update() (as before)              │
│   └─ render (as before)                       │
│                                              │
│  Ctrl+E toggle                                │
│   Display→Edit: player.shutdown()             │
│                  opacity = 1.0 for all         │
│   Edit→Display: player.init_page()            │
└─────────────────────────────────────────────┘
```

## 四、模式隔离合约

| 操作 | Display | Edit |
|---|---|---|
| AnimationPlayer | `update()` 每帧 tick | `shutdown()` — 不参与 |
| elements opacity | 动画控制 (HiddenOnStart) | 强制 1.0 |
| 点击 | 动画 Click 触发 | 元素拾取 / 拖拽 / 选框 |
| 渲染叠加 | 无 | 蚂蚁线 + handles + marquee |
| inspector | 不显示 | 右侧面板 |
| 帧率控制 | `repaint_after(16ms)` | egui 自动 |

## 五、集成到 `SeewoClassApp`

`SeewoClassApp` 新增 2 个字段：

```rust
pub struct SeewoClassApp {
    // ... existing fields ...
    pub mode: AppMode,
    pub edit_state: EditState,
}
```

`render_canvas_area` 新增 Edit 分支：

```rust
fn render_canvas_area(&mut self, ui: &mut Ui) {
    // ... existing preamble (viewport, cursor, response allocate) ...

    if self.mode == AppMode::Edit {
        self.handle_edit_input(ui, &response);
    } else if annotation_active {
        self.annotation.handle_input(&ctx, &response);
    } else {
        self.handle_canvas_input(ui, &response);
    }

    // ... render ...
    render::render_canvas(painter, &self.doc, &self.camera, &self.interaction);

    if self.mode == AppMode::Edit {
        render::draw_edit_overlay(painter, &self.doc, &self.edit_state, &self.camera);
    }
    // ...
}
```

## 六、阶段划分

| 阶段 | 内容 | 工时估 |
|---|---|---|
| Phase 1 | 模式切换 + `AppMode` + `EditState` | 0.5d |
| Phase 2 | 元素拾取 + 选框 + 蚂蚁线 + 拖拽 | 1d |
| Phase 3 | Resize handles + 缩放逻辑 | 0.5d |
| Phase 4 | Inspector 面板 (位置/大小/透明度) | 1d |
| Phase 5 | Inspector 动画参数 + 预览 | 1d |

## 七、内存影响

| 组件 | 内存 |
|---|---|
| `EditState` (HashSet + Vec) | < 1KB |
| 蚂蚁线绘制 | 0 KB (GPU 即时绘制) |
| Inspector 面板 | 0 KB (egui immediate mode) |
| **总计** | **< 10 KB** |

远低于 10MB 上限。

## 八、验收标准

- [ ] Ctrl+E 切换 < 16ms，无闪烁
- [ ] Edit 模式所有元素 opacity=1.0
- [ ] 切回 Display 模式动画按原逻辑播放
- [ ] 单选拖拽帧率 ≥60fps
- [ ] 多选拖拽帧率 ≥60fps
- [ ] 蚂蚁线流畅无卡顿
- [ ] 保存 .drft 后重开数值一致
- [ ] Inspector 修改 → 画布立即更新
- [ ] 预览动画 → 切回 Edit 模式无泄漏

---

以上为 Edit 模式设计。核心思路：**零侵入动画系统 + EditState 单一入口 + 直接操作 PageContent**。

请审阅，确认后开始 Phase 1 编码。
