//! drafftink-cosmos — 高性能 3D 宇宙/太阳系可视化引擎
//!
//! 取代希沃笨重的 WPF 3D 方案，基于 Rust + GPU 加速渲染，
//! 提供太阳系探索、行星标注、2D/3D 切换、轨道动画等功能。
//!
//! # 性能优化策略（CTO 指令）
//!
//! 1. **背景网格纹理化** — 星空和渐变背景预构建为 Mesh 缓存，视口不变不重建
//! 2. **行星列表去重绘** — 标签字符串缓存，避免每帧 `format!()` 分配
//! 3. **轨道线批处理** — 所有轨道合并为单个 Mesh，一次 `painter.add()`
//! 4. **帧率锁死** — `request_repaint_after(Duration::from_secs_f64(1.0/60.0))`

pub mod geometry;
pub mod ecs;
pub mod render;
pub mod scene;
pub mod ui;
pub mod projection;
pub mod animation;
pub mod resources;

use egui::{Color32, Rect, Vec2};
use egui::epaint::{Mesh, Vertex};
use nalgebra::{Point3, Vector2, Vector3};
use std::time::Duration;

use crate::animation::{AnimationController, ease_out_cubic};
use crate::ecs::{Orbit, Rotation, Transform, rotation_system, orbit_system};
use crate::render::{OrbitCamera, RenderBatch, SceneRenderer};
use crate::resources::ResourceCache;
use crate::scene::SolarSystemScene;
use crate::ui::{ControlPanel, ViewMode, render_labels, render_map_view};

// ─── 帧率配置 ──────────────────────────────────────────────────────────────

/// 目标帧率。设为 60 以匹配大多数显示器的刷新率。
/// egui 底层已通过 wgpu 开启 V-Sync，这里额外确保 CPU 不空转。
const TARGET_FPS: f64 = 60.0;
const FRAME_DURATION: Duration = Duration::from_nanos((1_000_000_000.0 / TARGET_FPS) as u64);

// ─── CosmosViewer — 主入口 ─────────────────────────────────────────────────

/// 宇宙查看器主入口。
///
/// 整合所有子系统：场景、渲染、UI、动画、资源缓存。
/// 对外提供单一的 `ui()` 方法，可嵌入任何 egui 应用中。
pub struct CosmosViewer {
    /// 资源缓存（网格、材质）
    pub cache: ResourceCache,
    /// 太阳系场景数据
    pub scene: SolarSystemScene,
    /// 3D 渲染器
    pub renderer: SceneRenderer,
    /// UI 控制面板
    pub controls: ControlPanel,
    /// 相机飞行动画
    camera_flight: Option<AnimationController>,
    camera_flight_start: Point3<f32>,
    camera_flight_end_target: Point3<f32>,
    camera_flight_end_distance: f32,
    /// 上一帧时间（用于 dt 计算）
    last_time: f64,
    /// 视图模式
    view_mode: ViewMode,

    // ── 性能优化：缓存 ──────────────────────────────────────────────

    /// 缓存的星空背景 Mesh（键 = 视口尺寸哈希）
    cached_background: Option<(u64, Mesh)>,
    /// 缓存的轨道线 Mesh（键 = VP 矩阵哈希 + 视口尺寸哈希）
    cached_orbits: Option<(u64, Mesh)>,
    /// 上一帧的 VP 矩阵哈希（用于检测相机变化，预留用于增量渲染优化）
    #[allow(dead_code)]
    last_vp_hash: u64,
}

impl Default for CosmosViewer {
    fn default() -> Self {
        Self::new()
    }
}

impl CosmosViewer {
    /// 创建新的太阳系查看器。
    ///
    /// 自动加载太阳系预设场景和资源。
    pub fn new() -> Self {
        let mut cache = ResourceCache::new();
        let scene = SolarSystemScene::new(&mut cache);

        let mut camera = OrbitCamera::new();
        camera.distance = 30.0;
        camera.yaw = std::f32::consts::FRAC_PI_4;
        camera.pitch = 0.4;

        let renderer = SceneRenderer::new(camera);

        Self {
            cache,
            scene,
            renderer,
            controls: ControlPanel::new(),
            camera_flight: None,
            camera_flight_start: Point3::origin(),
            camera_flight_end_target: Point3::origin(),
            camera_flight_end_distance: 10.0,
            last_time: 0.0,
            view_mode: ViewMode::Mode3D,
            cached_background: None,
            cached_orbits: None,
            last_vp_hash: 0,
        }
    }

    /// 渲染整个查看器 UI。
    ///
    /// 在 egui 的 CentralPanel 或 Window 中调用即可。
    pub fn ui(&mut self, ctx: &egui::Context) {
        // ── 帧率控制：锁死 60fps ──
        ctx.request_repaint_after(FRAME_DURATION);

        let current_time = ctx.input(|i| i.time);
        let dt = if self.last_time > 0.0 {
            (current_time - self.last_time) as f32
        } else {
            0.0
        };
        self.last_time = current_time;

        // 同步 view_mode（控制面板是单一事实来源）
        self.view_mode = self.controls.view_mode;

        // ── 模拟更新（自转 + 公转）──
        if !self.controls.paused {
            let sim_dt = dt * self.controls.simulation_speed;
            self.update_simulation(sim_dt);
        }

        // ── 相机飞行动画 ──
        if let Some(ref flight) = self.camera_flight {
            let t = flight.progress(current_time);
            let cam = self.renderer.camera_mut();

            // 位置插值（球面线性插值的简化版）
            let start_target = self.camera_flight_start;
            let end_target = self.camera_flight_end_target;
            cam.target.x = start_target.x + (end_target.x - start_target.x) * t;
            cam.target.y = start_target.y + (end_target.y - start_target.y) * t;
            cam.target.z = start_target.z + (end_target.z - start_target.z) * t;

            // 距离插值
            let start_dist = cam.distance;
            cam.distance = start_dist + (self.camera_flight_end_distance - start_dist) * t;

            if flight.is_done(current_time) {
                self.camera_flight = None;
            }
        }

        // ── 处理选中行星时的飞行 ──
        let _ = self.controls.selected_planet; // 占位，后续扩展

        // ── 主视口 ──
        egui::CentralPanel::default().show(ctx, |ui| {
            let rect = ui.max_rect();

            match self.view_mode {
                ViewMode::Mode3D => {
                    // 3D 视图
                    self.render_3d_view(ui, rect);
                }
                ViewMode::Mode2D => {
                    // 2D 地图视图
                    render_map_view(
                        ui,
                        rect,
                        &self.scene,
                        self.controls.show_labels,
                        self.controls.selected_planet,
                    );
                }
            }
        });

        // ── 控制面板（浮层）──
        self.controls.ui(ctx, &self.scene);

        // ── 处理重置视角 ──
        if self.controls.take_reset_view_request() {
            self.reset_camera();
        }

        // ── 控制面板自身的状态变化 ──
        if self.controls.show_orbits_changed() {
            self.cached_orbits = None; // 轨道显示/隐藏切换时失效缓存
        }
    }

    /// 更新物理模拟（自转 + 公转）。
    fn update_simulation(&mut self, dt: f32) {
        let n = self.scene.transforms.len();
        if n == 0 {
            return;
        }

        // ── 1. 自转系统：应用到所有实体 ──
        let rotations: Vec<Rotation> = self.scene.rotations
            .iter()
            .map(|r| r.clone().unwrap_or_else(|| Rotation::default()))
            .collect();
        rotation_system(&mut self.scene.transforms, &rotations, dt);

        // ── 2. 公转系统：只应用到有轨道的实体 ──
        let orbit_indices: Vec<usize> = self.scene.orbits
            .iter()
            .enumerate()
            .filter(|(_, o)| o.is_some())
            .map(|(i, _)| i)
            .collect();

        if !orbit_indices.is_empty() {
            let mut orbit_transforms: Vec<Transform> = orbit_indices
                .iter()
                .map(|&i| self.scene.transforms[i].clone())
                .collect();
            let mut orbits: Vec<Orbit> = orbit_indices
                .iter()
                .filter_map(|&i| self.scene.orbits[i].clone())
                .collect();

            orbit_system(&mut orbit_transforms, &mut orbits, dt);

            for (j, &i) in orbit_indices.iter().enumerate() {
                self.scene.transforms[i].position = orbit_transforms[j].position;
                if let Some(orbit) = self.scene.orbits[i].as_mut() {
                    *orbit = orbits[j].clone();
                }
            }
        }
    }

    /// 渲染 3D 视图。
    fn render_3d_view(&mut self, ui: &mut egui::Ui, rect: Rect) {
        // 更新相机宽高比
        self.renderer.camera_mut().aspect = rect.width() / rect.height().max(1.0);

        // 处理相机交互（鼠标拖拽旋转 + 滚轮缩放）
        let response = ui.interact(
            rect,
            egui::Id::new("cosmos_viewport"),
            egui::Sense::click_and_drag(),
        );

        let cam = self.renderer.camera_mut();

        // 判断鼠标按键
        let is_right_drag = response.dragged()
            && ui.input(|i| i.pointer.button_down(egui::PointerButton::Secondary));
        let is_left_drag = response.dragged() && !is_right_drag;

        // 左键拖拽旋转
        if is_left_drag {
            let delta = response.drag_delta();
            let yaw_delta = -delta.x / rect.width() * std::f32::consts::TAU;
            let pitch_delta = -delta.y / rect.height() * std::f32::consts::PI;
            cam.orbit(yaw_delta, pitch_delta);
        }

        // 滚轮缩放
        ui.input(|i| {
            let zoom_delta = i.zoom_delta();
            if (zoom_delta - 1.0).abs() > 0.001 {
                cam.zoom(1.0 / zoom_delta);
            }
        });

        // 右键拖拽平移
        if is_right_drag {
            let delta = response.drag_delta();
            let pan_delta = Vector2::new(
                -delta.x / rect.width(),
                delta.y / rect.height(),
            );
            cam.pan(pan_delta);
        }

        let painter = ui.painter_at(rect);

        // ── 优化 1: 缓存星空背景（仅视口尺寸变化时重建）──
        let bg_hash = hash_rect_size(&rect);
        if self.cached_background.as_ref().map(|(h, _)| *h != bg_hash).unwrap_or(true) {
            self.cached_background = Some((bg_hash, build_background_mesh(rect)));
        }
        if let Some((_, ref mesh)) = self.cached_background {
            painter.add(mesh.clone());
        }

        // ── 优化 3: 批量轨道线（合并为单个 Mesh，一次提交）──
        if self.controls.show_orbits {
            let vp = self.renderer.camera().view_projection();
            let vp_hash = hash_matrix4(&vp);
            let key = hash_combine(vp_hash, bg_hash);

            if self.cached_orbits.as_ref().map(|(h, _)| *h != key).unwrap_or(true) {
                self.cached_orbits = Some((key, build_orbit_mesh(
                    &vp, rect.width(), rect.height(), &self.scene.orbits,
                )));
            }
            if let Some((_, ref mesh)) = self.cached_orbits {
                if mesh.indices.len() > 0 {
                    painter.add(mesh.clone());
                }
            }
        }

        // 绘制所有实体（从远到近排序）
        self.draw_entities(&painter, rect);

        // 绘制标签
        if self.controls.show_labels {
            let labels: Vec<Option<crate::ecs::Label>> = self.scene.labels.clone();
            render_labels(
                &self.renderer,
                &painter,
                rect,
                &self.scene.transforms,
                &labels,
            );
        }

        // 性能信息（右下角）
        self.draw_perf_info(&painter, rect);
    }

    // ── 绘制所有实体（批量渲染：一次 Draw Call）─────────────────────

    fn draw_entities(&self, painter: &egui::Painter, rect: Rect) {
        let mut batch = RenderBatch::new();
        let default_material = crate::ecs::Material::default();
        let sw = rect.width();
        let sh = rect.height();

        for (i, transform) in self.scene.transforms.iter().enumerate() {
            if let Some(mesh_id) = self.scene.meshes[i] {
                if let Some(mesh) = self.cache.get_mesh(mesh_id) {
                    let material = self.scene.materials[i]
                        .and_then(|mid| self.cache.get_material(mid))
                        .unwrap_or(&default_material);
                    let model_matrix = transform.matrix();
                    self.renderer.collect_entity_triangles(
                        &mut batch, mesh, &model_matrix, material, sw, sh,
                    );
                }
            }
        }

        // ── 全局深度排序 + 一次 painter.add() → GPU 光栅化 ──
        self.renderer.finish_batch(batch, painter, rect);
    }

    /// 绘制性能信息（右下角小标签）。
    fn draw_perf_info(&self, painter: &egui::Painter, rect: Rect) {
        let entity_count = self.scene.entity_count();
        let mesh_count = self.cache.mesh_count();
        let cached = if self.cached_orbits.is_some() { " (cached)" } else { "" };

        let label = format!("🌍 {} 天体 | 📦 {} 网格{} | 🎮 批量渲染", entity_count, mesh_count, cached);
        let text_color = Color32::from_rgba_unmultiplied(200, 210, 230, 200);
        let bg_color = Color32::from_rgba_unmultiplied(0, 0, 0, 120);

        let galley = painter.layout_no_wrap(
            label,
            egui::FontId::monospace(11.0),
            text_color,
        );

        let padding = Vec2::new(8.0, 4.0);
        let label_rect = Rect::from_min_size(
            egui::pos2(
                rect.right() - galley.size().x - padding.x * 2.0 - 8.0,
                rect.bottom() - galley.size().y - padding.y * 2.0 - 8.0,
            ),
            galley.size() + padding * 2.0,
        );

        painter.rect_filled(label_rect, 4.0, bg_color);
        painter.galley(
            egui::pos2(label_rect.left() + padding.x, label_rect.top() + padding.y),
            galley,
            text_color,
        );
    }

    /// 飞向指定行星。
    pub fn fly_to_planet(&mut self, planet_index: usize, current_time: f64) {
        if planet_index >= self.scene.transforms.len() {
            return;
        }

        let target_pos = self.scene.transforms[planet_index].position;
        let scale = self.scene.transforms[planet_index].scale;

        self.camera_flight_start = Point3::from(self.renderer.camera().target);
        self.camera_flight_end_target = Point3::from(target_pos);
        self.camera_flight_end_distance = (scale * 5.0).max(1.5);

        let mut flight = AnimationController::new(1.5, ease_out_cubic);
        flight.start(current_time);
        self.camera_flight = Some(flight);
    }

    /// 重置相机到默认视角。
    pub fn reset_camera(&mut self) {
        let cam = self.renderer.camera_mut();
        cam.target = Point3::origin();
        cam.distance = 30.0;
        cam.yaw = std::f32::consts::FRAC_PI_4;
        cam.pitch = 0.4;
    }

    /// 获取当前视图模式。
    pub fn view_mode(&self) -> ViewMode {
        self.view_mode
    }

    /// 设置视图模式。
    pub fn set_view_mode(&mut self, mode: ViewMode) {
        self.view_mode = mode;
        self.controls.view_mode = mode;
    }
}

// ─── 优化 1: 星空背景 Mesh 构建 ────────────────────────────────────────────

/// 构建缓存的星空背景 Mesh（渐变 + 星星）。
/// 不再每帧调用 140 次 `painter.rect_filled` / `painter.circle_filled`。
fn build_background_mesh(rect: Rect) -> Mesh {
    let mut mesh = Mesh::default();
    let bg_top = Color32::from_rgb(5, 5, 20);
    let bg_bottom = Color32::from_rgb(10, 15, 35);

    // 渐变条纹（20 条）
    let steps = 20;
    for i in 0..steps {
        let t = i as f32 / (steps - 1) as f32;
        let y = rect.top() + t * rect.height();
        let h = rect.height() / steps as f32 + 1.0;
        let r = bg_top.r() as f32 + (bg_bottom.r() as f32 - bg_top.r() as f32) * t;
        let g = bg_top.g() as f32 + (bg_bottom.g() as f32 - bg_top.g() as f32) * t;
        let b = bg_top.b() as f32 + (bg_bottom.b() as f32 - bg_top.b() as f32) * t;
        let color = Color32::from_rgb(r as u8, g as u8, b as u8);

        let idx = mesh.vertices.len() as u32;
        mesh.vertices.extend_from_slice(&[
            Vertex { pos: egui::pos2(rect.left(), y), uv: egui::Pos2::ZERO, color },
            Vertex { pos: egui::pos2(rect.right(), y), uv: egui::Pos2::ZERO, color },
            Vertex { pos: egui::pos2(rect.right(), y + h), uv: egui::Pos2::ZERO, color },
            Vertex { pos: egui::pos2(rect.left(), y + h), uv: egui::Pos2::ZERO, color },
        ]);
        mesh.indices.extend_from_slice(&[
            idx, idx + 1, idx + 2,
            idx, idx + 2, idx + 3,
        ]);
    }

    // 伪随机星星（120 颗），使用确定性种子
    let star_count = 120;
    for i in 0..star_count {
        let x = ((i * 7919) % 10000) as f32 / 10000.0 * rect.width() + rect.left();
        let y = ((i * 6271) % 10000) as f32 / 10000.0 * rect.height() + rect.top();
        let brightness = ((i * 31) % 128 + 127) as u8;
        let size = if i % 7 == 0 { 2.0 } else { 1.0 };
        let color = Color32::from_rgb(brightness, brightness, brightness + 20);

        // 用 4 个三角形近似一个正方形星星
        let idx = mesh.vertices.len() as u32;
        mesh.vertices.extend_from_slice(&[
            Vertex { pos: egui::pos2(x - size, y - size), uv: egui::Pos2::ZERO, color },
            Vertex { pos: egui::pos2(x + size, y - size), uv: egui::Pos2::ZERO, color },
            Vertex { pos: egui::pos2(x + size, y + size), uv: egui::Pos2::ZERO, color },
            Vertex { pos: egui::pos2(x - size, y + size), uv: egui::Pos2::ZERO, color },
        ]);
        mesh.indices.extend_from_slice(&[
            idx, idx + 1, idx + 2,
            idx, idx + 2, idx + 3,
        ]);
    }

    mesh
}

// ─── 优化 3: 轨道线批量 Mesh 构建 ──────────────────────────────────────────

/// 将所有轨道线合并为一个 Mesh。
///
/// 关键优化：原来每帧调用 `painter.line_segment()` N×64 次，
/// 每次触发 egui 内部 tessellation。现在一次 `painter.add()` 完成。
fn build_orbit_mesh(
    vp: &nalgebra::Matrix4<f32>,
    sw: f32,
    sh: f32,
    orbits: &[Option<Orbit>],
) -> Mesh {
    let mut mesh = Mesh::default();
    let orbit_color = Color32::from_rgba_unmultiplied(100, 120, 160, 120);
    let samples = 64;
    let tau = std::f32::consts::TAU;

    for orbit_opt in orbits {
        let orbit = match orbit_opt {
            Some(o) => o,
            None => continue,
        };

        let mut prev_screen: Option<egui::Pos2> = None;

        for i in 0..=samples {
            let angle = i as f32 / samples as f32 * tau;

            // 椭圆上的点（在轨道平面内）
            let r = orbit.semi_major_axis * (1.0 - orbit.eccentricity.powi(2))
                / (1.0 + orbit.eccentricity * angle.cos());
            let x = r * angle.cos();
            let z = r * angle.sin();

            let mut pos = Vector3::new(x, 0.0, z);

            // 近心点幅角旋转（绕 z 轴）
            let cos_w = orbit.arg_of_perihelion.cos();
            let sin_w = orbit.arg_of_perihelion.sin();
            let x2 = pos.x * cos_w - pos.y * sin_w;
            let y2 = pos.x * sin_w + pos.y * cos_w;
            pos.x = x2;
            pos.y = y2;

            // 倾角（绕 x 轴）
            let cos_i = orbit.inclination.cos();
            let sin_i = orbit.inclination.sin();
            let y3 = pos.y * cos_i - pos.z * sin_i;
            let z3 = pos.y * sin_i + pos.z * cos_i;
            pos.y = y3;
            pos.z = z3;

            // 升交点经度（绕 y 轴）
            let cos_o = orbit.ascending_node.cos();
            let sin_o = orbit.ascending_node.sin();
            let x4 = pos.x * cos_o + pos.z * sin_o;
            let z4 = -pos.x * sin_o + pos.z * cos_o;
            pos.x = x4;
            pos.z = z4;

            let point = Point3::new(pos.x, pos.y, pos.z);

            if let Some(screen) = crate::render::project_point(vp, &point, sw, sh) {
                let screen_pos = egui::pos2(screen.x, screen.y);
                if let Some(prev) = prev_screen {
                    // 每条线段 = 2 个顶点，2 个三角形（模拟线段）
                    let idx = mesh.vertices.len() as u32;
                    let half_w = 0.5; // 线宽的一半
                    mesh.vertices.extend_from_slice(&[
                        Vertex { pos: prev, uv: egui::Pos2::ZERO, color: orbit_color },
                        Vertex { pos: screen_pos, uv: egui::Pos2::ZERO, color: orbit_color },
                        Vertex { pos: egui::pos2(prev.x + half_w, prev.y), uv: egui::Pos2::ZERO, color: orbit_color },
                        Vertex { pos: egui::pos2(screen_pos.x + half_w, screen_pos.y), uv: egui::Pos2::ZERO, color: orbit_color },
                    ]);
                    mesh.indices.extend_from_slice(&[
                        idx, idx + 1, idx + 2,
                        idx + 1, idx + 2, idx + 3,
                    ]);
                }
                prev_screen = Some(screen_pos);
            } else {
                prev_screen = None;
            }
        }
    }

    mesh
}

// ─── 哈希辅助函数 ──────────────────────────────────────────────────────────

/// 基于视口尺寸计算简单哈希，用于缓存失效检测。
fn hash_rect_size(rect: &Rect) -> u64 {
    let w = (rect.width() * 1000.0) as u64;
    let h = (rect.height() * 1000.0) as u64;
    (w << 32) | h
}

/// 矩阵 4x4 的简单哈希（用于 VP 矩阵变化检测）。
fn hash_matrix4(m: &nalgebra::Matrix4<f32>) -> u64 {
    let mut h: u64 = 0;
    for i in 0..16 {
        let v = m[i] as u32;
        h = h.wrapping_mul(31).wrapping_add(v as u64);
    }
    h
}

/// 组合两个哈希。
fn hash_combine(a: u64, b: u64) -> u64 {
    a.wrapping_mul(6364136223846793005).wrapping_add(b)
}