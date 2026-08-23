//! 3D 立体图形 — 相机、投影、基本体
//!
//! # 升维架构
//! - 平面图形用 Point2D (x, y)
//! - 立体图形用 Point3D (x, y, z)
//!
//! # 投影
//! - 透视投影 (Perspective)：近大远小，像人眼
//! - 正交投影 (Orthographic)：无近大远小，用于工程制图
//!
//! # 交互
//! - 旋转：四元数 (UnitQuaternion) 表示，避免万向节死锁
//! - 缩放：改变相机距离
//! - 鼠标拖拽改变四元数

use nalgebra::{Matrix4, Point3, UnitQuaternion, Vector3};
use serde::{Deserialize, Serialize};

use crate::definitions::Point3D;

// ── 3D 渲染模式 ─────────────────────────────────────────────────

/// 3D 渲染模式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum RenderMode3D {
    /// 仅线框
    Wireframe,
    /// 实心不透明
    Solid,
    /// 实心 + 线框叠加
    #[default]
    SolidWireframe,
}

// ── 投影模式 ────────────────────────────────────────────────────

/// 投影模式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum ProjectionMode {
    /// 透视投影 — 近大远小
    #[default]
    Perspective,
    /// 正交投影 — 无近大远小，用于工程制图
    Orthographic,
}

// ── 3D 相机 ─────────────────────────────────────────────────────

/// 3D 轨道相机
///
/// 围绕目标点旋转，使用四元数表示旋转状态。
/// 鼠标拖拽 → 改变 yaw/pitch → 转换为四元数增量。
pub struct Camera3D {
    /// 目标点（相机围绕此点旋转）
    pub target: Point3D,
    /// 相机到目标的距离
    pub distance: f32,
    /// 旋转四元数（避免万向节死锁）
    pub rotation: UnitQuaternion<f32>,
    /// 投影模式
    pub projection: ProjectionMode,
    /// 透视投影 FOV（弧度）
    pub fov: f32,
    /// 近裁剪面
    pub near: f32,
    /// 远裁剪面
    pub far: f32,
    /// 正交投影缩放
    pub ortho_scale: f32,
}

impl Default for Camera3D {
    fn default() -> Self {
        Self {
            target: Vector3::zeros(),
            distance: 10.0,
            rotation: UnitQuaternion::identity(),
            projection: ProjectionMode::Perspective,
            fov: std::f32::consts::PI / 4.0, // 45°
            near: 0.1,
            far: 1000.0,
            ortho_scale: 10.0,
        }
    }
}

impl Camera3D {
    /// 创建新相机
    pub fn new() -> Self {
        Self::default()
    }

    /// 相机位置 = target + rotation * (0, 0, -distance)
    pub fn position(&self) -> Point3D {
        let offset = self.rotation * Vector3::new(0.0, 0.0, -self.distance);
        self.target + offset
    }

    /// 上方向 = rotation * (0, 1, 0)
    pub fn up(&self) -> Vector3<f32> {
        self.rotation * Vector3::new(0.0, 1.0, 0.0)
    }

    /// 视图矩阵
    pub fn view_matrix(&self) -> Matrix4<f32> {
        let eye = self.position();
        let target = Point3::from(self.target);
        let up = self.up();
        Matrix4::look_at_rh(&Point3::from(eye), &target, &up)
    }

    /// 投影矩阵
    pub fn projection_matrix(&self, aspect: f32) -> Matrix4<f32> {
        match self.projection {
            ProjectionMode::Perspective => {
                Matrix4::new_perspective(aspect, self.fov, self.near, self.far)
            }
            ProjectionMode::Orthographic => {
                let s = self.ortho_scale;
                let half_w = s * aspect;
                let half_h = s;
                Matrix4::new_orthographic(-half_w, half_w, -half_h, half_h, self.near, self.far)
            }
        }
    }

    /// 视图-投影矩阵 = projection × view
    pub fn view_projection_matrix(&self, aspect: f32) -> Matrix4<f32> {
        self.projection_matrix(aspect) * self.view_matrix()
    }

    /// 轨道旋转 — 鼠标拖拽
    ///
    /// dx → yaw（绕世界 Y 轴），dy → pitch（绕相机 X 轴）
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        let yaw = -dx * 0.01;
        let pitch = -dy * 0.01;

        // 绕世界 Y 轴旋转
        let yaw_rot = UnitQuaternion::from_axis_angle(&Vector3::y_axis(), yaw);
        // 绕相机本地 X 轴旋转
        let right = self.rotation * Vector3::new(1.0, 0.0, 0.0);
        let right_axis = nalgebra::Unit::new_normalize(right);
        let pitch_rot = UnitQuaternion::from_axis_angle(&right_axis, pitch);

        self.rotation = yaw_rot * self.rotation * pitch_rot;
    }

    /// 缩放 — 改变相机距离
    ///
    /// 正值（滚轮上）→ 拉近 → 距离减小
    /// 负值（滚轮下）→ 拉远 → 距离增大
    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance * (1.0 - delta * 0.001)).clamp(1.0, 500.0);
    }

    /// 重置相机
    pub fn reset(&mut self) {
        self.rotation = UnitQuaternion::identity();
        self.distance = 10.0;
    }

    /// 切换投影模式
    pub fn toggle_projection(&mut self) {
        self.projection = match self.projection {
            ProjectionMode::Perspective => ProjectionMode::Orthographic,
            ProjectionMode::Orthographic => ProjectionMode::Perspective,
        };
    }

    /// 将 3D 点投影到 2D 屏幕坐标
    ///
    /// 返回 None 表示点在裁剪面外。
    pub fn project(&self, point: Point3D, aspect: f32, screen_size: (f32, f32)) -> Option<(f32, f32)> {
        let vp = self.view_projection_matrix(aspect);
        let p = Point3::from(point);
        let projected = vp.transform_point(&p);

        // 透视除法
        if projected.z < -1.0 || projected.z > 1.0 {
            return None;
        }

        // NDC → 屏幕坐标
        let screen_x = (projected.x + 1.0) * 0.5 * screen_size.0;
        let screen_y = (1.0 - (projected.y + 1.0) * 0.5) * screen_size.1;

        Some((screen_x, screen_y))
    }
}

// ── 3D 网格 ─────────────────────────────────────────────────────

/// 3D 网格 — 顶点 + 边 + 面
#[derive(Debug, Clone)]
pub struct Mesh3D {
    /// 顶点坐标
    pub vertices: Vec<Point3D>,
    /// 边（顶点索引对）
    pub edges: Vec<(u32, u32)>,
    /// 三角面（顶点索引三元组）
    pub faces: Vec<(u32, u32, u32)>,
}

impl Mesh3D {
    /// 创建空网格
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            edges: Vec::new(),
            faces: Vec::new(),
        }
    }
}

impl Default for Mesh3D {
    fn default() -> Self {
        Self::new()
    }
}

// ── 基本体生成 ──────────────────────────────────────────────────

/// 生成立方体网格
///
/// 8 个顶点，12 条棱，12 个三角面。
pub fn generate_cube(center: Point3D, size: f32) -> Mesh3D {
    let s = size * 0.5;
    let cx = center.x;
    let cy = center.y;
    let cz = center.z;

    // 8 个顶点
    let vertices = vec![
        Vector3::new(cx - s, cy - s, cz - s), // 0: 左下后
        Vector3::new(cx + s, cy - s, cz - s), // 1: 右下后
        Vector3::new(cx + s, cy + s, cz - s), // 2: 右上后
        Vector3::new(cx - s, cy + s, cz - s), // 3: 左上后
        Vector3::new(cx - s, cy - s, cz + s), // 4: 左下前
        Vector3::new(cx + s, cy - s, cz + s), // 5: 右下前
        Vector3::new(cx + s, cy + s, cz + s), // 6: 右上前
        Vector3::new(cx - s, cy + s, cz + s), // 7: 左上前
    ];

    // 12 条棱
    let edges = vec![
        (0, 1), (1, 2), (2, 3), (3, 0), // 后面
        (4, 5), (5, 6), (6, 7), (7, 4), // 前面
        (0, 4), (1, 5), (2, 6), (3, 7), // 连接
    ];

    // 12 个三角面（6 面 × 2）
    let faces = vec![
        // 后面 (z = -s)
        (0, 2, 1), (0, 3, 2),
        // 前面 (z = +s)
        (4, 5, 6), (4, 6, 7),
        // 左面 (x = -s)
        (0, 4, 7), (0, 7, 3),
        // 右面 (x = +s)
        (1, 2, 6), (1, 6, 5),
        // 下面 (y = -s)
        (0, 1, 5), (0, 5, 4),
        // 上面 (y = +s)
        (3, 7, 6), (3, 6, 2),
    ];

    Mesh3D { vertices, edges, faces }
}

/// 生成 UV 球体网格
///
/// 使用经纬度分割生成球体。
/// 注意：对于星球级渲染，应使用 Icosphere 获得更均匀的顶点分布。
/// 此处 UV 球适用于教育几何场景。
pub fn generate_sphere(center: Point3D, radius: f32, lat_segments: usize, lon_segments: usize) -> Mesh3D {
    let mut vertices = Vec::new();
    let mut edges = Vec::new();
    let mut faces = Vec::new();

    // 生成顶点
    for lat in 0..=lat_segments {
        let theta = lat as f32 / lat_segments as f32 * std::f32::consts::PI; // 0..π
        let sin_theta = theta.sin();
        let cos_theta = theta.cos();

        for lon in 0..=lon_segments {
            let phi = lon as f32 / lon_segments as f32 * std::f32::consts::TAU; // 0..2π
            let x = center.x + radius * sin_theta * phi.cos();
            let y = center.y + radius * cos_theta;
            let z = center.z + radius * sin_theta * phi.sin();
            vertices.push(Vector3::new(x, y, z));
        }
    }

    // 生成边和面
    for lat in 0..lat_segments {
        for lon in 0..lon_segments {
            let i = (lat * (lon_segments + 1) + lon) as u32;
            let i_right = i + 1;
            let i_down = i + (lon_segments + 1) as u32;
            let i_diag = i_down + 1;

            // 水平边
            if lat > 0 {
                edges.push((i, i_right));
            }
            // 垂直边
            if lon > 0 {
                edges.push((i, i_down));
            }

            // 三角面（跳过极点退化的面）
            if lat > 0 && lat < lat_segments {
                faces.push((i, i_down, i_right));
                faces.push((i_right, i_down, i_diag));
            }
        }
    }

    Mesh3D { vertices, edges, faces }
}

/// 生成圆锥网格
pub fn generate_cone(base_center: Point3D, apex: Point3D, radius: f32, segments: usize) -> Mesh3D {
    let mut vertices = Vec::new();
    let mut edges = Vec::new();
    let mut faces = Vec::new();

    // 底面中心
    vertices.push(base_center);
    let center_idx = 0u32;

    // 底面圆周顶点
    let axis = apex - base_center;
    let height = axis.norm();
    let up = if axis.y.abs() > 0.9 {
        Vector3::new(1.0, 0.0, 0.0)
    } else {
        Vector3::new(0.0, 1.0, 0.0)
    };
    let right = axis.cross(&up).try_normalize(1e-10).unwrap_or(Vector3::new(1.0, 0.0, 0.0));
    let forward = right.cross(&axis).try_normalize(1e-10).unwrap_or(Vector3::new(0.0, 0.0, 1.0));

    for i in 0..segments {
        let angle = i as f32 / segments as f32 * std::f32::consts::TAU;
        let offset = right * (radius * angle.cos()) + forward * (radius * angle.sin());
        vertices.push(base_center + offset);
    }

    // 顶点
    vertices.push(apex);
    let apex_idx = (segments + 1) as u32;

    // 边：底面圆周 + 侧面棱
    for i in 0..segments {
        let curr = (i + 1) as u32;
        let next = ((i + 1) % segments + 1) as u32;
        edges.push((curr, next)); // 底面边
        edges.push((curr, apex_idx)); // 侧棱
    }

    // 面：底面三角扇 + 侧面三角
    for i in 0..segments {
        let curr = (i + 1) as u32;
        let next = ((i + 1) % segments + 1) as u32;
        faces.push((center_idx, next, curr)); // 底面（法线朝下）
        faces.push((curr, next, apex_idx)); // 侧面
    }

    let _ = height; // suppress unused warning
    Mesh3D { vertices, edges, faces }
}

/// 生成圆柱网格
pub fn generate_cylinder(
    bottom_center: Point3D,
    top_center: Point3D,
    radius: f32,
    segments: usize,
) -> Mesh3D {
    let mut vertices = Vec::new();
    let mut edges = Vec::new();
    let mut faces = Vec::new();

    let axis = top_center - bottom_center;
    let up = if axis.y.abs() > 0.9 {
        Vector3::new(1.0, 0.0, 0.0)
    } else {
        Vector3::new(0.0, 1.0, 0.0)
    };
    let right = axis.cross(&up).try_normalize(1e-10).unwrap_or(Vector3::new(1.0, 0.0, 0.0));
    let forward = right.cross(&axis).try_normalize(1e-10).unwrap_or(Vector3::new(0.0, 0.0, 1.0));

    // 底面顶点
    vertices.push(bottom_center); // 0: 底面中心
    for i in 0..segments {
        let angle = i as f32 / segments as f32 * std::f32::consts::TAU;
        let offset = right * (radius * angle.cos()) + forward * (radius * angle.sin());
        vertices.push(bottom_center + offset);
    }

    // 顶面顶点
    let top_center_idx = (segments + 1) as u32;
    vertices.push(top_center);
    for i in 0..segments {
        let angle = i as f32 / segments as f32 * std::f32::consts::TAU;
        let offset = right * (radius * angle.cos()) + forward * (radius * angle.sin());
        vertices.push(top_center + offset);
    }

    // 边
    for i in 0..segments {
        let b_curr = (i + 1) as u32;
        let b_next = ((i + 1) % segments + 1) as u32;
        let t_curr = (segments + 2 + i) as u32;
        let t_next = (segments + 2 + (i + 1) % segments) as u32;
        edges.push((b_curr, b_next)); // 底面边
        edges.push((t_curr, t_next)); // 顶面边
        edges.push((b_curr, t_curr)); // 侧棱
    }

    // 面
    for i in 0..segments {
        let b_curr = (i + 1) as u32;
        let b_next = ((i + 1) % segments + 1) as u32;
        let t_curr = (segments + 2 + i) as u32;
        let t_next = (segments + 2 + (i + 1) % segments) as u32;

        // 底面（法线朝下）
        faces.push((0, b_next, b_curr));
        // 顶面（法线朝上）
        faces.push((top_center_idx, t_curr, t_next));
        // 侧面
        faces.push((b_curr, b_next, t_curr));
        faces.push((b_next, t_next, t_curr));
    }

    Mesh3D { vertices, edges, faces }
}

// ── 投影渲染辅助 ────────────────────────────────────────────────

/// 投影后的 2D 边
#[derive(Debug, Clone)]
pub struct ProjectedEdge {
    pub start: (f32, f32),
    pub end: (f32, f32),
    /// 平均深度（用于深度排序）
    pub depth: f32,
}

/// 投影后的 2D 三角面（不透明，带 Lambert 光照）
#[derive(Debug, Clone)]
pub struct ProjectedFace {
    /// 三角形的三个屏幕坐标顶点
    pub vertices: [(f32, f32); 3],
    /// 平均深度（用于画家算法，远的先画）
    pub depth: f32,
    /// 经过 Lambert 光照计算后的不透明颜色 (alpha = 255)
    pub color: [u8; 4],
}

/// 将 3D 网格投影到 2D 屏幕坐标
///
/// 返回投影后的边列表，按深度排序（远的先画）。
pub fn project_mesh(
    mesh: &Mesh3D,
    camera: &Camera3D,
    aspect: f32,
    screen_size: (f32, f32),
) -> Vec<ProjectedEdge> {
    let vp = camera.view_projection_matrix(aspect);

    // 投影所有顶点
    let projected: Vec<Option<(f32, f32, f32)>> = mesh
        .vertices
        .iter()
        .map(|v| {
            let p = Point3::from(*v);
            let proj = vp.transform_point(&p);
            if proj.z < -1.0 || proj.z > 1.0 {
                None
            } else {
                let sx = (proj.x + 1.0) * 0.5 * screen_size.0;
                let sy = (1.0 - (proj.y + 1.0) * 0.5) * screen_size.1;
                Some((sx, sy, proj.z))
            }
        })
        .collect();

    // 投影边
    let mut edges: Vec<ProjectedEdge> = mesh
        .edges
        .iter()
        .filter_map(|&(a, b)| {
            let pa = projected.get(a as usize).copied().flatten()?;
            let pb = projected.get(b as usize).copied().flatten()?;
            Some(ProjectedEdge {
                start: (pa.0, pa.1),
                end: (pb.0, pb.1),
                depth: (pa.2 + pb.2) * 0.5,
            })
        })
        .collect();

    // 按深度排序（远的先画）
    edges.sort_by(|a, b| b.depth.partial_cmp(&a.depth).unwrap_or(std::cmp::Ordering::Equal));

    edges
}

/// 投影后的顶点：(屏幕坐标 or None, 世界坐标)
type ProjectedVertex = (Option<(f32, f32, f32)>, Point3D);

/// 将 3D 网格的面投影到 2D 屏幕坐标，执行背面剔除和 Lambert 光照
///
/// 返回不透明面列表（alpha = 255），按深度排序（远的先画）。
///
/// # 算法
/// 1. 投影所有顶点到屏幕坐标（保留 NDC 深度）
/// 2. 对每个三角面：
///    a. 计算面法线（世界空间）
///    b. 背面剔除：法线 · 视线方向 > 0 → 跳过
///    c. Lambert 光照：brightness = ambient + (1 - ambient) * max(0, n·L)
///    d. 调制基础颜色
/// 3. 按平均深度排序（画家算法，远的先画）
pub fn project_mesh_faces(
    mesh: &Mesh3D,
    camera: &Camera3D,
    aspect: f32,
    screen_size: (f32, f32),
    base_color: [u8; 3],
) -> Vec<ProjectedFace> {
    let vp = camera.view_projection_matrix(aspect);
    let cam_pos = camera.position();

    // 光照方向（从物体指向光源，归一化）
    let light_dir = Vector3::new(-0.4_f32, -1.0, -0.6).normalize();
    let ambient: f32 = 0.35; // 环境光

    // 投影所有顶点（保留世界坐标用于法线计算）
    let projected: Vec<ProjectedVertex> = mesh
        .vertices
        .iter()
        .map(|v| {
            let p = Point3::from(*v);
            let proj = vp.transform_point(&p);
            let screen = if proj.z < -1.0 || proj.z > 1.0 {
                None
            } else {
                let sx = (proj.x + 1.0) * 0.5 * screen_size.0;
                let sy = (1.0 - (proj.y + 1.0) * 0.5) * screen_size.1;
                Some((sx, sy, proj.z))
            };
            (screen, *v)
        })
        .collect();

    let mut faces: Vec<ProjectedFace> = mesh
        .faces
        .iter()
        .filter_map(|&(a, b, c)| {
            let (pa, wa) = projected.get(a as usize).copied()?;
            let (pb, wb) = projected.get(b as usize).copied()?;
            let (pc, wc) = projected.get(c as usize).copied()?;
            let sa = pa?;
            let sb = pb?;
            let sc = pc?;

            // 世界空间中三角面的两条边
            let edge1 = wb - wa;
            let edge2 = wc - wa;

            // 面法线（叉积）
            let normal = edge1.cross(&edge2);
            let normal_len = normal.norm();
            if normal_len < 1e-10 {
                return None; // 退化面
            }
            let normal_n = normal / normal_len;

            // 面中心
            let face_center = (wa + wb + wc) / 3.0;
            // 视线方向（从面指向相机）
            let view_dir = cam_pos - face_center;
            let view_len = view_dir.norm();
            if view_len < 1e-10 {
                return None;
            }
            let view_n = view_dir / view_len;

            // 背面剔除：法线 · 视线 < 0 → 背面，跳过
            if normal_n.dot(&view_n) < 0.0 {
                return None;
            }

            // Lambert 光照：diffuse = max(0, -n · L)（L 指向光源）
            let diffuse = (-normal_n.dot(&light_dir)).max(0.0);
            let brightness = ambient + (1.0 - ambient) * diffuse;

            // 调制基础颜色（alpha = 255，完全不透明）
            let r = (base_color[0] as f32 * brightness).clamp(0.0, 255.0) as u8;
            let g = (base_color[1] as f32 * brightness).clamp(0.0, 255.0) as u8;
            let b = (base_color[2] as f32 * brightness).clamp(0.0, 255.0) as u8;

            // 平均深度
            let avg_depth = (sa.2 + sb.2 + sc.2) / 3.0;

            Some(ProjectedFace {
                vertices: [(sa.0, sa.1), (sb.0, sb.1), (sc.0, sc.1)],
                depth: avg_depth,
                color: [r, g, b, 255],
            })
        })
        .collect();

    // 按深度排序（远的先画，画家算法）
    faces.sort_by(|a, b| b.depth.partial_cmp(&a.depth).unwrap_or(std::cmp::Ordering::Equal));

    faces
}

// ── 扩展基本体生成 ──────────────────────────────────────────────

/// 生成长方体网格
///
/// 8 个顶点，12 条棱，12 个三角面。
/// 与生成立方体相同拓扑，但允许不同维度。
pub fn generate_cuboid(center: Point3D, width: f32, height: f32, depth: f32) -> Mesh3D {
    let hw = width * 0.5;
    let hh = height * 0.5;
    let hd = depth * 0.5;
    let cx = center.x;
    let cy = center.y;
    let cz = center.z;

    // 8 个顶点
    let vertices = vec![
        Vector3::new(cx - hw, cy - hh, cz - hd), // 0: 左下后
        Vector3::new(cx + hw, cy - hh, cz - hd), // 1: 右下后
        Vector3::new(cx + hw, cy + hh, cz - hd), // 2: 右上后
        Vector3::new(cx - hw, cy + hh, cz - hd), // 3: 左上后
        Vector3::new(cx - hw, cy - hh, cz + hd), // 4: 左下前
        Vector3::new(cx + hw, cy - hh, cz + hd), // 5: 右下前
        Vector3::new(cx + hw, cy + hh, cz + hd), // 6: 右上前
        Vector3::new(cx - hw, cy + hh, cz + hd), // 7: 左上前
    ];

    // 12 条棱
    let edges = vec![
        (0, 1), (1, 2), (2, 3), (3, 0), // 后面
        (4, 5), (5, 6), (6, 7), (7, 4), // 前面
        (0, 4), (1, 5), (2, 6), (3, 7), // 连接
    ];

    // 12 个三角面（6 面 × 2）
    let faces = vec![
        // 后面 (z = -hd)
        (0, 2, 1), (0, 3, 2),
        // 前面 (z = +hd)
        (4, 5, 6), (4, 6, 7),
        // 左面 (x = -hw)
        (0, 4, 7), (0, 7, 3),
        // 右面 (x = +hw)
        (1, 2, 6), (1, 6, 5),
        // 下面 (y = -hh)
        (0, 1, 5), (0, 5, 4),
        // 上面 (y = +hh)
        (3, 7, 6), (3, 6, 2),
    ];

    Mesh3D { vertices, edges, faces }
}

/// 生成 N 边棱柱网格
///
/// 底面多边形 + 顶面多边形 + 侧面四边形（每个四边形 = 2 个三角形）。
pub fn generate_prism(
    bottom_center: Point3D,
    top_center: Point3D,
    radius: f32,
    sides: u32,
) -> Mesh3D {
    let mut vertices = Vec::new();
    let mut edges = Vec::new();
    let mut faces = Vec::new();

    let axis = top_center - bottom_center;
    let up = if axis.y.abs() > 0.9 {
        Vector3::new(1.0, 0.0, 0.0)
    } else {
        Vector3::new(0.0, 1.0, 0.0)
    };
    let right = axis
        .cross(&up)
        .try_normalize(1e-10)
        .unwrap_or(Vector3::new(1.0, 0.0, 0.0));
    let forward = right
        .cross(&axis)
        .try_normalize(1e-10)
        .unwrap_or(Vector3::new(0.0, 0.0, 1.0));

    // 顶点：底面中心(0)、底面环(1..sides)、顶面中心(sides+1)、顶面环(sides+2..)
    vertices.push(bottom_center); // 0: 底面中心
    for i in 0..sides {
        let angle = i as f32 / sides as f32 * std::f32::consts::TAU;
        let offset = right * (radius * angle.cos()) + forward * (radius * angle.sin());
        vertices.push(bottom_center + offset);
    }

    let top_center_idx = sides + 1;
    vertices.push(top_center);
    for i in 0..sides {
        let angle = i as f32 / sides as f32 * std::f32::consts::TAU;
        let offset = right * (radius * angle.cos()) + forward * (radius * angle.sin());
        vertices.push(top_center + offset);
    }

    // 边：底面环、顶面环、垂直棱
    for i in 0..sides {
        let b_curr = i + 1;
        let b_next = (i + 1) % sides + 1;
        let t_curr = sides + 2 + i;
        let t_next = sides + 2 + (i + 1) % sides;
        edges.push((b_curr, b_next)); // 底面边
        edges.push((t_curr, t_next)); // 顶面边
        edges.push((b_curr, t_curr)); // 垂直棱
    }

    // 面：底面扇、顶面扇、侧面四边形（2 个三角形）
    for i in 0..sides {
        let b_curr = i + 1;
        let b_next = (i + 1) % sides + 1;
        let t_curr = sides + 2 + i;
        let t_next = sides + 2 + (i + 1) % sides;

        // 底面（法线朝下）
        faces.push((0, b_next, b_curr));
        // 顶面（法线朝上）
        faces.push((top_center_idx, t_curr, t_next));
        // 侧面四边形 = 2 个三角形
        faces.push((b_curr, b_next, t_curr));
        faces.push((b_next, t_next, t_curr));
    }

    Mesh3D { vertices, edges, faces }
}

/// 生成 N 边棱锥网格
///
/// 底面多边形 + 朝向顶点的三角侧面。
/// 与生成圆锥类似，但底面是正多边形而非圆形。
pub fn generate_pyramid(
    base_center: Point3D,
    apex: Point3D,
    radius: f32,
    sides: u32,
) -> Mesh3D {
    let mut vertices = Vec::new();
    let mut edges = Vec::new();
    let mut faces = Vec::new();

    // 底面中心
    vertices.push(base_center);
    let center_idx = 0u32;

    // 计算底面方向轴
    let axis = apex - base_center;
    let up = if axis.y.abs() > 0.9 {
        Vector3::new(1.0, 0.0, 0.0)
    } else {
        Vector3::new(0.0, 1.0, 0.0)
    };
    let right = axis
        .cross(&up)
        .try_normalize(1e-10)
        .unwrap_or(Vector3::new(1.0, 0.0, 0.0));
    let forward = right
        .cross(&axis)
        .try_normalize(1e-10)
        .unwrap_or(Vector3::new(0.0, 0.0, 1.0));

    // 底面环顶点
    for i in 0..sides {
        let angle = i as f32 / sides as f32 * std::f32::consts::TAU;
        let offset = right * (radius * angle.cos()) + forward * (radius * angle.sin());
        vertices.push(base_center + offset);
    }

    // 顶点
    vertices.push(apex);
    let apex_idx = sides + 1;

    // 边：底面环 + 侧棱
    for i in 0..sides {
        let curr = i + 1;
        let next = (i + 1) % sides + 1;
        edges.push((curr, next)); // 底面边
        edges.push((curr, apex_idx)); // 侧棱
    }

    // 面：底面扇 + 侧面三角形
    for i in 0..sides {
        let curr = i + 1;
        let next = (i + 1) % sides + 1;
        faces.push((center_idx, next, curr)); // 底面（法线朝下）
        faces.push((curr, next, apex_idx)); // 侧面
    }

    Mesh3D { vertices, edges, faces }
}

/// 生成圆台（截头圆锥）网格
///
/// 底圆 + 顶圆 + 侧面四边形。
/// 与生成圆柱类似，但底/顶半径不同。
pub fn generate_frustum(
    bottom_center: Point3D,
    top_center: Point3D,
    bottom_radius: f32,
    top_radius: f32,
    segments: usize,
) -> Mesh3D {
    let mut vertices = Vec::new();
    let mut edges = Vec::new();
    let mut faces = Vec::new();

    let axis = top_center - bottom_center;
    let up = if axis.y.abs() > 0.9 {
        Vector3::new(1.0, 0.0, 0.0)
    } else {
        Vector3::new(0.0, 1.0, 0.0)
    };
    let right = axis
        .cross(&up)
        .try_normalize(1e-10)
        .unwrap_or(Vector3::new(1.0, 0.0, 0.0));
    let forward = right
        .cross(&axis)
        .try_normalize(1e-10)
        .unwrap_or(Vector3::new(0.0, 0.0, 1.0));

    // 底面顶点
    vertices.push(bottom_center); // 0: 底面中心
    for i in 0..segments {
        let angle = i as f32 / segments as f32 * std::f32::consts::TAU;
        let offset =
            right * (bottom_radius * angle.cos()) + forward * (bottom_radius * angle.sin());
        vertices.push(bottom_center + offset);
    }

    // 顶面顶点
    let top_center_idx = (segments + 1) as u32;
    vertices.push(top_center);
    for i in 0..segments {
        let angle = i as f32 / segments as f32 * std::f32::consts::TAU;
        let offset = right * (top_radius * angle.cos()) + forward * (top_radius * angle.sin());
        vertices.push(top_center + offset);
    }

    // 边
    for i in 0..segments {
        let b_curr = (i + 1) as u32;
        let b_next = ((i + 1) % segments + 1) as u32;
        let t_curr = (segments + 2 + i) as u32;
        let t_next = (segments + 2 + (i + 1) % segments) as u32;
        edges.push((b_curr, b_next)); // 底面边
        edges.push((t_curr, t_next)); // 顶面边
        edges.push((b_curr, t_curr)); // 侧棱
    }

    // 面
    for i in 0..segments {
        let b_curr = (i + 1) as u32;
        let b_next = ((i + 1) % segments + 1) as u32;
        let t_curr = (segments + 2 + i) as u32;
        let t_next = (segments + 2 + (i + 1) % segments) as u32;

        // 底面（法线朝下）
        faces.push((0, b_next, b_curr));
        // 顶面（法线朝上）
        faces.push((top_center_idx, t_curr, t_next));
        // 侧面四边形 = 2 个三角形
        faces.push((b_curr, b_next, t_curr));
        faces.push((b_next, t_next, t_curr));
    }

    Mesh3D { vertices, edges, faces }
}

/// 生成正多面体网格
///
/// 根据多面体类型生成：正四面体、正六面体、正八面体、正十二面体、正二十面体。
/// `size` 为外接球半径。
pub fn generate_regular_polyhedron(
    center: Point3D,
    size: f32,
    poly_type: crate::definitions::PolyhedronType,
) -> Mesh3D {
    use crate::definitions::PolyhedronType;

    match poly_type {
        PolyhedronType::Tetrahedron => {
            // 4 个顶点：(1,1,1), (-1,-1,1), (-1,1,-1), (1,-1,-1)
            // 缩放 size/√3 使外接球半径为 size
            let scale = size / 3.0_f32.sqrt();
            let raw = [
                Vector3::new(1.0, 1.0, 1.0),
                Vector3::new(-1.0, -1.0, 1.0),
                Vector3::new(-1.0, 1.0, -1.0),
                Vector3::new(1.0, -1.0, -1.0),
            ];
            let vertices: Vec<Point3D> = raw
                .iter()
                .map(|v| center + v * scale)
                .collect();

            let edges = vec![
                (0, 1), (0, 2), (0, 3),
                (1, 2), (1, 3),
                (2, 3),
            ];

            let faces = vec![
                (0, 2, 1),
                (0, 1, 3),
                (0, 3, 2),
                (1, 2, 3),
            ];

            Mesh3D { vertices, edges, faces }
        }
        PolyhedronType::Hexahedron => {
            // 正六面体 = 立方体，外接球半径 = size → 边长 = size * 2/√3
            generate_cube(center, size * 2.0 / 3.0_f32.sqrt())
        }
        PolyhedronType::Octahedron => {
            // 6 个顶点：(±1,0,0), (0,±1,0), (0,0,±1) 缩放 size
            let vertices = vec![
                center + Vector3::new(size, 0.0, 0.0),
                center + Vector3::new(-size, 0.0, 0.0),
                center + Vector3::new(0.0, size, 0.0),
                center + Vector3::new(0.0, -size, 0.0),
                center + Vector3::new(0.0, 0.0, size),
                center + Vector3::new(0.0, 0.0, -size),
            ];

            let edges = vec![
                (0, 2), (0, 3), (0, 4), (0, 5),
                (1, 2), (1, 3), (1, 4), (1, 5),
                (2, 4), (2, 5), (3, 4), (3, 5),
            ];

            let faces = vec![
                (0, 4, 2), (0, 2, 5),
                (0, 5, 3), (0, 3, 4),
                (1, 2, 4), (1, 5, 2),
                (1, 4, 3), (1, 3, 5),
            ];

            Mesh3D { vertices, edges, faces }
        }
        PolyhedronType::Icosahedron => {
            // 黄金比例
            let phi = (1.0 + 5.0_f32.sqrt()) / 2.0;
            // (0, ±1, ±φ) 的循环排列
            let raw = [
                Vector3::new(0.0, 1.0, phi),
                Vector3::new(0.0, 1.0, -phi),
                Vector3::new(0.0, -1.0, phi),
                Vector3::new(0.0, -1.0, -phi),
                Vector3::new(1.0, phi, 0.0),
                Vector3::new(1.0, -phi, 0.0),
                Vector3::new(-1.0, phi, 0.0),
                Vector3::new(-1.0, -phi, 0.0),
                Vector3::new(phi, 0.0, 1.0),
                Vector3::new(phi, 0.0, -1.0),
                Vector3::new(-phi, 0.0, 1.0),
                Vector3::new(-phi, 0.0, -1.0),
            ];
            // 归一化后缩放至外接球半径 size
            let norm = (1.0 + phi * phi).sqrt();
            let scale = size / norm;
            let vertices: Vec<Point3D> = raw
                .iter()
                .map(|v| center + v * scale)
                .collect();

            let edges = vec![
                (0, 2), (0, 4), (0, 6), (0, 8), (0, 10),
                (1, 3), (1, 4), (1, 6), (1, 9), (1, 11),
                (2, 5), (2, 7), (2, 8), (2, 10),
                (3, 5), (3, 7), (3, 9), (3, 11),
                (4, 6), (4, 8), (4, 9),
                (5, 7), (5, 8), (5, 9),
                (6, 10), (6, 11),
                (7, 10), (7, 11),
                (8, 9),
                (10, 11),
            ];

            // 20 个三角面（通过边遍历推导：每条边恰好属于 2 个面）
            let faces = vec![
                // 围绕顶点 0 的 5 个面
                (0, 2, 8), (0, 8, 4), (0, 4, 6), (0, 6, 10), (0, 10, 2),
                // 围绕顶点 1 的 5 个面
                (1, 3, 9), (1, 9, 4), (1, 4, 6), (1, 6, 11), (1, 11, 3),
                // 中部环
                (2, 8, 5), (2, 5, 7), (2, 7, 10),
                (3, 9, 5), (3, 5, 7), (3, 7, 11),
                // 底部 4 个面
                (4, 8, 9), (5, 8, 9), (6, 11, 10), (7, 11, 10),
            ];

            Mesh3D { vertices, edges, faces }
        }
        PolyhedronType::Dodecahedron => {
            // 黄金比例
            let phi = (1.0 + 5.0_f32.sqrt()) / 2.0;
            let inv_phi = 1.0 / phi;
            // 20 个顶点：
            // (±1, ±1, ±1) → 8
            // (0, ±1/φ, ±φ) 循环排列 → 4 + 4 + 4 = 12
            let raw = [
                // (±1, ±1, ±1)
                Vector3::new(1.0, 1.0, 1.0),
                Vector3::new(1.0, 1.0, -1.0),
                Vector3::new(1.0, -1.0, 1.0),
                Vector3::new(1.0, -1.0, -1.0),
                Vector3::new(-1.0, 1.0, 1.0),
                Vector3::new(-1.0, 1.0, -1.0),
                Vector3::new(-1.0, -1.0, 1.0),
                Vector3::new(-1.0, -1.0, -1.0),
                // (0, ±1/φ, ±φ) 循环排列
                Vector3::new(0.0, inv_phi, phi),
                Vector3::new(0.0, inv_phi, -phi),
                Vector3::new(0.0, -inv_phi, phi),
                Vector3::new(0.0, -inv_phi, -phi),
                // (±1/φ, ±φ, 0) 循环排列
                Vector3::new(inv_phi, phi, 0.0),
                Vector3::new(inv_phi, -phi, 0.0),
                Vector3::new(-inv_phi, phi, 0.0),
                Vector3::new(-inv_phi, -phi, 0.0),
                // (±φ, 0, ±1/φ) 循环排列
                Vector3::new(phi, 0.0, inv_phi),
                Vector3::new(phi, 0.0, -inv_phi),
                Vector3::new(-phi, 0.0, inv_phi),
                Vector3::new(-phi, 0.0, -inv_phi),
            ];
            // 缩放 size/√3 使外接球半径为 size
            let scale = size / 3.0_f32.sqrt();
            let vertices: Vec<Point3D> = raw
                .iter()
                .map(|v| center + v * scale)
                .collect();

            // 30 条边（基于标准正十二面体拓扑）
            let edges = vec![
                (0, 8), (0, 12), (0, 16),
                (1, 9), (1, 12), (1, 17),
                (2, 10), (2, 13), (2, 16),
                (3, 11), (3, 13), (3, 17),
                (4, 8), (4, 14), (4, 18),
                (5, 9), (5, 14), (5, 19),
                (6, 10), (6, 15), (6, 18),
                (7, 11), (7, 15), (7, 19),
                (8, 10), (9, 11),
                (12, 14), (13, 15),
                (16, 17), (18, 19),
            ];

            // 12 个五边形面（每个五边形拆成 3 个三角形，共 36 个三角面）
            // 五边形顶点序列通过边遍历推导：每条边恰好属于 2 个面。
            // 五边形 [v0,v1,v2,v3,v4] 扇形分解为 (v0,v1,v2),(v0,v2,v3),(v0,v3,v4)。
            let pentagons: [[u32; 5]; 12] = [
                [8, 0, 12, 14, 4],
                [12, 0, 16, 17, 1],
                [16, 0, 8, 10, 2],
                [11, 3, 13, 15, 7],
                [13, 3, 17, 16, 2],
                [17, 3, 11, 9, 1],
                [9, 5, 14, 12, 1],
                [14, 5, 19, 18, 4],
                [19, 5, 9, 11, 7],
                [10, 6, 15, 13, 2],
                [15, 6, 18, 19, 7],
                [18, 6, 10, 8, 4],
            ];

            let mut faces = Vec::new();
            for p in &pentagons {
                faces.push((p[0], p[1], p[2]));
                faces.push((p[0], p[2], p[3]));
                faces.push((p[0], p[3], p[4]));
            }

            Mesh3D { vertices, edges, faces }
        }
    }
}

// ── 测试 ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_default() {
        let cam = Camera3D::new();
        assert_eq!(cam.distance, 10.0);
        assert_eq!(cam.projection, ProjectionMode::Perspective);
    }

    #[test]
    fn test_camera_position() {
        let cam = Camera3D::new();
        let pos = cam.position();
        // 默认旋转为单位四元数，相机在 (0, 0, -10)
        assert!((pos.z - (-10.0)).abs() < 0.001);
    }

    #[test]
    fn test_camera_orbit() {
        let mut cam = Camera3D::new();
        cam.orbit(1.0, 0.0);
        // 旋转后位置应该改变
        let pos = cam.position();
        assert!(pos.x.abs() > 0.01 || pos.z.abs() > 0.01);
    }

    #[test]
    fn test_camera_zoom() {
        let mut cam = Camera3D::new();
        let initial = cam.distance;
        cam.zoom(-100.0);
        assert!(cam.distance > initial);
    }

    #[test]
    fn test_camera_toggle_projection() {
        let mut cam = Camera3D::new();
        assert_eq!(cam.projection, ProjectionMode::Perspective);
        cam.toggle_projection();
        assert_eq!(cam.projection, ProjectionMode::Orthographic);
        cam.toggle_projection();
        assert_eq!(cam.projection, ProjectionMode::Perspective);
    }

    #[test]
    fn test_generate_cube() {
        let mesh = generate_cube(Vector3::new(0.0, 0.0, 0.0), 2.0);
        assert_eq!(mesh.vertices.len(), 8);
        assert_eq!(mesh.edges.len(), 12);
        assert_eq!(mesh.faces.len(), 12);
    }

    #[test]
    fn test_generate_cube_offset() {
        let mesh = generate_cube(Vector3::new(5.0, 0.0, 0.0), 2.0);
        // 最左顶点 x = 5 - 1 = 4
        let min_x = mesh.vertices.iter().map(|v| v.x).fold(f32::INFINITY, f32::min);
        assert!((min_x - 4.0).abs() < 0.001);
    }

    #[test]
    fn test_generate_sphere() {
        let mesh = generate_sphere(Vector3::new(0.0, 0.0, 0.0), 1.0, 16, 32);
        // (17 * 33) 个顶点
        assert_eq!(mesh.vertices.len(), 17 * 33);
        assert!(!mesh.edges.is_empty());
        assert!(!mesh.faces.is_empty());
    }

    #[test]
    fn test_generate_cone() {
        let mesh = generate_cone(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 2.0, 0.0),
            1.0,
            8,
        );
        // 1(中心) + 8(底面) + 1(顶点) = 10
        assert_eq!(mesh.vertices.len(), 10);
        assert!(!mesh.edges.is_empty());
        assert!(!mesh.faces.is_empty());
    }

    #[test]
    fn test_generate_cylinder() {
        let mesh = generate_cylinder(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 2.0, 0.0),
            1.0,
            8,
        );
        // 1(底中心) + 8(底面) + 1(顶中心) + 8(顶面) = 18
        assert_eq!(mesh.vertices.len(), 18);
        assert!(!mesh.edges.is_empty());
        assert!(!mesh.faces.is_empty());
    }

    #[test]
    fn test_project_mesh() {
        let mesh = generate_cube(Vector3::new(0.0, 0.0, 0.0), 2.0);
        let camera = Camera3D::new();
        let edges = project_mesh(&mesh, &camera, 1.0, (800.0, 600.0));
        // 立方体有 12 条边，都应该可见
        assert!(!edges.is_empty());
    }

    #[test]
    fn test_project_mesh_faces_opaque() {
        let mesh = generate_cube(Vector3::new(0.0, 0.0, 0.0), 2.0);
        let camera = Camera3D::new();
        let faces = project_mesh_faces(&mesh, &camera, 1.0, (800.0, 600.0), [100, 150, 220]);

        // 背面剔除后应剩 6 个面（立方体 12 个三角面，正面可见的一半）
        assert!(!faces.is_empty(), "should have visible faces");
        assert!(
            faces.len() <= 12,
            "face count should not exceed total faces"
        );

        // 所有面的 alpha 应为 255（完全不透明）
        for face in &faces {
            assert_eq!(face.color[3], 255, "all faces must be fully opaque");
        }

        // 面应按深度排序（远的先画）
        for i in 1..faces.len() {
            assert!(
                faces[i - 1].depth >= faces[i].depth,
                "faces must be sorted far-to-near (painter's algorithm)"
            );
        }
    }

    #[test]
    fn test_project_mesh_faces_backface_culling() {
        // 将相机放在立方体前方，只能看到 3 个面
        let mesh = generate_cube(Vector3::new(0.0, 0.0, 0.0), 2.0);
        let mut camera = Camera3D::new();
        camera.distance = 5.0;
        let faces = project_mesh_faces(&mesh, &camera, 1.0, (800.0, 600.0), [120, 200, 150]);

        // 背面被剔除，面数应少于总面数 12
        assert!(
            faces.len() < 12,
            "backface culling should reduce visible faces"
        );
        assert!(!faces.is_empty(), "at least some faces should be visible");
    }

    #[test]
    fn test_generate_cuboid() {
        let mesh = generate_cuboid(Vector3::new(0.0, 0.0, 0.0), 3.0, 2.0, 1.0);
        assert_eq!(mesh.vertices.len(), 8);
        assert_eq!(mesh.edges.len(), 12);
        assert_eq!(mesh.faces.len(), 12);
    }

    #[test]
    fn test_generate_prism() {
        let mesh = generate_prism(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 3.0, 0.0),
            2.0,
            6,
        );
        // 1(bottom center) + 6(bottom ring) + 1(top center) + 6(top ring) = 14
        assert_eq!(mesh.vertices.len(), 14);
        assert!(!mesh.edges.is_empty());
        assert!(!mesh.faces.is_empty());
    }

    #[test]
    fn test_generate_pyramid() {
        let mesh = generate_pyramid(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 3.0, 0.0),
            2.0,
            4,
        );
        // 1(base center) + 4(base ring) + 1(apex) = 6
        assert_eq!(mesh.vertices.len(), 6);
        assert!(!mesh.edges.is_empty());
        assert!(!mesh.faces.is_empty());
    }

    #[test]
    fn test_generate_frustum() {
        let mesh = generate_frustum(
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 3.0, 0.0),
            2.0,
            1.0,
            8,
        );
        assert!(!mesh.vertices.is_empty());
        assert!(!mesh.faces.is_empty());
    }

    #[test]
    fn test_generate_tetrahedron() {
        let mesh = generate_regular_polyhedron(
            Vector3::new(0.0, 0.0, 0.0),
            2.0,
            crate::definitions::PolyhedronType::Tetrahedron,
        );
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.faces.len(), 4);
    }

    #[test]
    fn test_generate_octahedron() {
        let mesh = generate_regular_polyhedron(
            Vector3::new(0.0, 0.0, 0.0),
            2.0,
            crate::definitions::PolyhedronType::Octahedron,
        );
        assert_eq!(mesh.vertices.len(), 6);
        assert_eq!(mesh.faces.len(), 8);
    }

    #[test]
    fn test_generate_icosahedron() {
        let mesh = generate_regular_polyhedron(
            Vector3::new(0.0, 0.0, 0.0),
            2.0,
            crate::definitions::PolyhedronType::Icosahedron,
        );
        assert_eq!(mesh.vertices.len(), 12);
        assert_eq!(mesh.faces.len(), 20);
    }
}
