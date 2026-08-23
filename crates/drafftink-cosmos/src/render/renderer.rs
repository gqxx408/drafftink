//! 渲染器实现
//!
//! 基于 egui Painter 的 3D 渲染器，使用 2D 投影方式绘制 3D 三角形。
//! 底层通过 egui 的 wgpu 后端获得 GPU 加速。
//!
//! # 性能优化（CTO 指令）
//!
//! - **批量渲染**: 所有实体三角形合并为单个 Mesh，一次 `painter.add()` 调用
//! - **全局深度排序**: 跨实体的画家算法，避免逐实体排序开销
//! - **预分配缓冲区**: 复用 Vec 避免每帧分配

use std::cmp::Ordering;

use egui::{Color32, Mesh, Painter, Pos2, Rect, Stroke};
use egui::epaint::Vertex;
use nalgebra::{Matrix4, Point3, Vector3};

use crate::ecs::Material;
use crate::geometry::MeshData;

use super::camera::{project_point, OrbitCamera};

// ---------------------------------------------------------------------------
// 批量渲染数据结构
// ---------------------------------------------------------------------------

/// 单个待渲染三角形（屏幕空间，已着色）
struct BatchedTriangle {
    /// 屏幕空间顶点（3 个）
    screen_pos: [Pos2; 3],
    /// 深度值（NDC z，用于全局排序）
    depth: f32,
    /// 最终颜色（已计算光照）
    color: Color32,
}

/// 批量渲染收集器
///
/// 收集所有实体的三角形，统一进行全局深度排序后构建单个 Mesh。
/// 避免每实体一次 `painter.add()` 的多次 Draw Call 开销。
pub struct RenderBatch {
    triangles: Vec<BatchedTriangle>,
    /// 预分配的顶点缓冲区（避免重复创建）
    vertex_buf: Vec<Point3<f32>>,
}

impl RenderBatch {
    /// 创建空的批量渲染收集器
    pub fn new() -> Self {
        Self {
            triangles: Vec::with_capacity(4096),
            vertex_buf: Vec::with_capacity(4096),
        }
    }

    /// 清空批量收集器，保留已分配容量
    pub fn clear(&mut self) {
        self.triangles.clear();
        self.vertex_buf.clear();
    }

    /// 当前三角形数量
    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }
}

impl Default for RenderBatch {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// SceneRenderer
// ---------------------------------------------------------------------------

/// 基于 egui Painter 的 3D 场景渲染器
///
/// 核心思路：
/// - 将 3D 三角形投影到 2D 屏幕空间
/// - 使用 egui::Mesh 绘制填充三角形
/// - 支持方向光 + 兰伯特漫反射光照
/// - 全局深度排序（画家算法）
pub struct SceneRenderer {
    camera: OrbitCamera,
    /// 方向光方向（世界空间，已归一化）
    light_dir: Vector3<f32>,
    /// 环境光强度（0-1）
    ambient: f32,
}

impl SceneRenderer {
    /// 创建新的场景渲染器
    ///
    /// 默认光从右上方（1, 1, 1）方向入射，环境光强度 0.3。
    pub fn new(camera: OrbitCamera) -> Self {
        let light_dir = Vector3::new(1.0, 1.0, 1.0).normalize();
        Self {
            camera,
            light_dir,
            ambient: 0.3,
        }
    }

    /// 获取可变相机引用
    pub fn camera_mut(&mut self) -> &mut OrbitCamera {
        &mut self.camera
    }

    /// 获取相机引用
    pub fn camera(&self) -> &OrbitCamera {
        &self.camera
    }

    // -----------------------------------------------------------------------
    // 批量渲染（性能优化主路径）
    // -----------------------------------------------------------------------

    /// 将单个实体的网格三角形收集到批量缓冲区中。
    ///
    /// 此方法不绘制任何内容，只做 CPU 侧的投影、剔除、光照计算，
    /// 将结果三角形存入 `batch`。
    ///
    /// # 参数
    /// - `batch`: 批量收集器（可变引用）
    /// - `mesh`: 3D 网格数据
    /// - `model_matrix`: 模型矩阵
    /// - `material`: 材质
    /// - `screen_width`, `screen_height`: 视口尺寸
    pub fn collect_entity_triangles(
        &self,
        batch: &mut RenderBatch,
        mesh: &MeshData,
        model_matrix: &Matrix4<f32>,
        material: &Material,
        screen_width: f32,
        screen_height: f32,
    ) {
        let vp = self.camera.view_projection();
        let mvp = vp * model_matrix;

        // 预计算世界空间顶点（复用 batch 的 vertex_buf）
        let vert_count = mesh.vertices.len();
        batch.vertex_buf.clear();
        batch.vertex_buf.reserve(vert_count);
        for v in &mesh.vertices {
            batch.vertex_buf.push(model_matrix.transform_point(v));
        }

        let num_tris = mesh.indices.len() / 3;

        // 预计算材质颜色基础值
        let base_r = (material.albedo[0] * 255.0) as i32;
        let base_g = (material.albedo[1] * 255.0) as i32;
        let base_b = (material.albedo[2] * 255.0) as i32;

        for tri_idx in 0..num_tris {
            let i0 = mesh.indices[tri_idx * 3] as usize;
            let i1 = mesh.indices[tri_idx * 3 + 1] as usize;
            let i2 = mesh.indices[tri_idx * 3 + 2] as usize;

            let v0 = &mesh.vertices[i0];
            let v1 = &mesh.vertices[i1];
            let v2 = &mesh.vertices[i2];

            // 裁剪空间坐标
            let clip0 = mvp * v0.to_homogeneous();
            let clip1 = mvp * v1.to_homogeneous();
            let clip2 = mvp * v2.to_homogeneous();

            // 快速剔除：任意顶点在相机后面
            if clip0.w <= 0.0 || clip1.w <= 0.0 || clip2.w <= 0.0 {
                continue;
            }

            // 世界空间顶点
            let w0 = batch.vertex_buf[i0];
            let w1 = batch.vertex_buf[i1];
            let w2 = batch.vertex_buf[i2];

            // 计算三角形法线（世界空间）
            let edge1 = w1 - w0;
            let edge2 = w2 - w0;
            let normal = edge1.cross(&edge2);
            let normal_len = normal.norm();
            if normal_len < 1e-6 {
                continue; // 退化三角形
            }
            let normal = normal / normal_len;

            // 背面剔除
            let tri_center = Point3::new(
                (w0.x + w1.x + w2.x) / 3.0,
                (w0.y + w1.y + w2.y) / 3.0,
                (w0.z + w1.z + w2.z) / 3.0,
            );
            let view_dir = (self.camera.eye_position() - tri_center).normalize();
            if normal.dot(&view_dir) <= 0.0 {
                continue;
            }

            // 深度（NDC z）
            let center_clip = mvp * tri_center.to_homogeneous();
            let depth = center_clip.z / center_clip.w;

            // 兰伯特光照
            let diffuse = normal.dot(&self.light_dir).max(0.0);
            let intensity = self.ambient + (1.0 - self.ambient) * diffuse;

            let r = (base_r as f32 * intensity).clamp(0.0, 255.0) as u8;
            let g = (base_g as f32 * intensity).clamp(0.0, 255.0) as u8;
            let b = (base_b as f32 * intensity).clamp(0.0, 255.0) as u8;
            let color = Color32::from_rgb(r, g, b);

            // 投影到屏幕空间
            let p0 = project_point(&vp, &w0, screen_width, screen_height);
            let p1 = project_point(&vp, &w1, screen_width, screen_height);
            let p2 = project_point(&vp, &w2, screen_width, screen_height);

            if let (Some(p0), Some(p1), Some(p2)) = (p0, p1, p2) {
                batch.triangles.push(BatchedTriangle {
                    screen_pos: [
                        Pos2::new(p0.x, p0.y),
                        Pos2::new(p1.x, p1.y),
                        Pos2::new(p2.x, p2.y),
                    ],
                    depth,
                    color,
                });
            }
        }
    }

    /// 完成批量渲染：全局深度排序 + 构建单个 Mesh + 提交绘制。
    ///
    /// 这是性能优化的核心：所有实体的三角形在此处一次性提交给 egui，
    /// egui_wgpu 将在 GPU 端用一次 Draw Call 完成所有三角形光栅化。
    ///
    /// # 参数
    /// - `batch`: 批量收集器（消费所有权）
    /// - `painter`: egui Painter
    /// - `rect`: 视口矩形
    pub fn finish_batch(&self, mut batch: RenderBatch, painter: &Painter, rect: Rect) {
        if batch.triangles.is_empty() {
            return;
        }

        // ── 全局深度排序：远的先画（画家算法）──
        batch.triangles.sort_by(|a, b| {
            b.depth
                .partial_cmp(&a.depth)
                .unwrap_or(Ordering::Equal)
        });

        // ── 构建单个 egui::Mesh（一次 Draw Call）──
        let offset_x = rect.min.x;
        let offset_y = rect.min.y;
        let mut egui_mesh = Mesh::default();
        egui_mesh.vertices.reserve(batch.triangles.len() * 3);
        egui_mesh.indices.reserve(batch.triangles.len() * 3);

        for tri in &batch.triangles {
            let base_idx = egui_mesh.vertices.len() as u32;

            egui_mesh.vertices.push(Vertex {
                pos: Pos2::new(offset_x + tri.screen_pos[0].x, offset_y + tri.screen_pos[0].y),
                uv: Pos2::ZERO,
                color: tri.color,
            });
            egui_mesh.vertices.push(Vertex {
                pos: Pos2::new(offset_x + tri.screen_pos[1].x, offset_y + tri.screen_pos[1].y),
                uv: Pos2::ZERO,
                color: tri.color,
            });
            egui_mesh.vertices.push(Vertex {
                pos: Pos2::new(offset_x + tri.screen_pos[2].x, offset_y + tri.screen_pos[2].y),
                uv: Pos2::ZERO,
                color: tri.color,
            });

            egui_mesh.indices.push(base_idx);
            egui_mesh.indices.push(base_idx + 1);
            egui_mesh.indices.push(base_idx + 2);
        }

        // ── 一次提交，GPU 端处理 ──
        painter.add(egui_mesh);
    }

    // -----------------------------------------------------------------------
    // 单实体渲染（保留向后兼容）
    // -----------------------------------------------------------------------

    /// 渲染单个 3D 网格（独立渲染，非批量模式）。
    ///
    /// 对单个实体做完整的投影-剔除-光照-排序-绘制流程。
    /// 如需渲染多个实体，推荐使用批量渲染以获得更好的性能。
    pub fn render_mesh(
        &self,
        painter: &Painter,
        rect: Rect,
        mesh: &MeshData,
        model_matrix: &Matrix4<f32>,
        material: &Material,
    ) {
        let mut batch = RenderBatch::new();
        self.collect_entity_triangles(
            &mut batch,
            mesh,
            model_matrix,
            material,
            rect.width(),
            rect.height(),
        );
        self.finish_batch(batch, painter, rect);
    }

    // -----------------------------------------------------------------------
    // 辅助绘制方法
    // -----------------------------------------------------------------------

    /// 绘制 3D 线段
    ///
    /// 用于绘制轨道线、坐标轴、辅助线等。
    /// 如果线段端点在视锥体外，则不绘制（简单实现，不做裁剪）。
    pub fn render_line(
        &self,
        painter: &Painter,
        rect: Rect,
        start: &Point3<f32>,
        end: &Point3<f32>,
        color: Color32,
        width: f32,
    ) {
        let vp = self.camera.view_projection();
        let screen_width = rect.width();
        let screen_height = rect.height();

        let p0 = project_point(&vp, start, screen_width, screen_height);
        let p1 = project_point(&vp, end, screen_width, screen_height);

        if let (Some(p0), Some(p1)) = (p0, p1) {
            let pos0 = Pos2::new(rect.min.x + p0.x, rect.min.y + p0.y);
            let pos1 = Pos2::new(rect.min.x + p1.x, rect.min.y + p1.y);
            painter.line_segment([pos0, pos1], Stroke::new(width, color));
        }
    }

    /// 绘制始终面向相机的文本标签（Billboard）
    ///
    /// 将 3D 位置投影到屏幕空间后，用 painter.text() 绘制文本。
    /// 文本自动居中对齐到投影点。
    ///
    /// 返回标签是否在视野内（是否成功绘制）。
    pub fn render_billboard_label(
        &self,
        painter: &Painter,
        rect: Rect,
        position: &Point3<f32>,
        text: &str,
        color: Color32,
    ) -> bool {
        let vp = self.camera.view_projection();
        let screen_width = rect.width();
        let screen_height = rect.height();

        if let Some(p) = project_point(&vp, position, screen_width, screen_height) {
            let pos = Pos2::new(rect.min.x + p.x, rect.min.y + p.y);
            painter.text(
                pos,
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(12.0),
                color,
            );
            true
        } else {
            false
        }
    }
}