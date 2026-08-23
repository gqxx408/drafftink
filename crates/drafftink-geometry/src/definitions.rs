//! 核心数据定义 — Definition-driven Geometry
//!
//! 所有几何元素存储 *数学定义* 和 *依赖引用*，而非直接坐标。
//! 求解器按拓扑顺序解析依赖图，生成具体点位。
//!
//! # 核心理念
//! - `PointDef::Free { pos }` — 自由点，直接存储坐标
//! - `PointDef::Midpoint { a, b }` — 中点，依赖两个引用点
//! - `PointDef::OnLine { line, t }` — 线上点，依赖一条线 + 参数 t
//! - `PointDef::OnCircle { circle, angle }` — 圆上点，依赖圆 + 角度
//! - `PointDef::Intersection { a, b }` — 两线交点
//!
//! # 2D 图形扩展
//! 多边形、圆弧、扇形、椭圆、圆环、贝塞尔曲线、角度/长度标注
//!
//! # 3D 图形扩展
//! 立方体、长方体、棱柱、棱锥、球体、圆柱、圆锥、圆台、正多面体

use nalgebra::{Vector2, Vector3};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ── 2D / 3D 点类型 ──────────────────────────────────────────────

/// 2D 点（已求解的具体坐标）
pub type Point2D = Vector2<f32>;

/// 3D 点（已求解的具体坐标）
pub type Point3D = Vector3<f32>;

// ── 点定义（definition-driven）──────────────────────────────────

/// 点的数学定义
///
/// 每个变体描述了点的 *构造方式*，而非直接坐标。
/// 依赖的其他点/线/圆通过 Uuid 引用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PointDef {
    /// 自由点 — 用户直接拖拽定位
    Free { pos: Point2D },

    /// 中点 — 两参考点的算术平均
    Midpoint { a: Uuid, b: Uuid },

    /// 线上点 — 参数 t ∈ [0,1] 表示在线段上的位置
    OnLine { line: Uuid, t: f32 },

    /// 圆上点 — 角度（弧度）表示在圆上的位置
    OnCircle { circle: Uuid, angle: f32 },

    /// 两线交点
    Intersection {
        line_a: Uuid,
        line_b: Uuid,
    },

    /// 线圆交点（选择第一个或第二个交点）
    LineCircleIntersection {
        line: Uuid,
        circle: Uuid,
        which: bool, // true = 第一个交点, false = 第二个
    },

    /// 3D 自由点（升维支持）
    Free3D { pos: Point3D },
}

impl PointDef {
    /// 返回此点定义依赖的所有元素 Uuid
    pub fn dependencies(&self) -> Vec<Uuid> {
        match self {
            PointDef::Free { .. } | PointDef::Free3D { .. } => Vec::new(),
            PointDef::Midpoint { a, b } => vec![*a, *b],
            PointDef::OnLine { line, .. } => vec![*line],
            PointDef::OnCircle { circle, .. } => vec![*circle],
            PointDef::Intersection { line_a, line_b } => vec![*line_a, *line_b],
            PointDef::LineCircleIntersection { line, circle, .. } => vec![*line, *circle],
        }
    }

    /// 判断是否为 3D 点
    pub fn is_3d(&self) -> bool {
        matches!(self, PointDef::Free3D { .. })
    }
}

// ── 线定义 ──────────────────────────────────────────────────────

/// 线段定义 — 引用起点和终点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineDef {
    /// 唯一标识
    pub id: Uuid,
    /// 起点（引用一个 PointDef）
    pub start: Uuid,
    /// 终点（引用一个 PointDef）
    pub end: Uuid,
}

// ── 圆定义 ──────────────────────────────────────────────────────

/// 圆定义 — 引用圆心点 + 半径
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircleDef {
    /// 唯一标识
    pub id: Uuid,
    /// 圆心（引用一个 PointDef）
    pub center: Uuid,
    /// 半径
    pub radius: f32,
}

// ── 2D 图形扩展定义 ─────────────────────────────────────────────

/// 多边形定义 — 任意 N 边形（顶点列表驱动）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolygonDef {
    /// 唯一标识
    pub id: Uuid,
    /// 顶点列表（引用 PointDef）
    pub vertices: Vec<Uuid>,
}

/// 正多边形定义 — 中心 + 半径 + 边数 + 旋转角
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegularPolygonDef {
    /// 唯一标识
    pub id: Uuid,
    /// 中心点（引用 PointDef）
    pub center: Uuid,
    /// 外接圆半径
    pub radius: f32,
    /// 边数（≥3）
    pub sides: u32,
    /// 旋转角度（弧度）
    pub rotation: f32,
}

/// 圆弧定义 — 圆心 + 半径 + 起止角
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArcDef {
    /// 唯一标识
    pub id: Uuid,
    /// 圆心（引用 PointDef）
    pub center: Uuid,
    /// 半径
    pub radius: f32,
    /// 起始角度（弧度）
    pub start_angle: f32,
    /// 终止角度（弧度）
    pub end_angle: f32,
}

/// 扇形定义 — 圆心 + 半径 + 起止角
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectorDef {
    /// 唯一标识
    pub id: Uuid,
    /// 圆心（引用 PointDef）
    pub center: Uuid,
    /// 半径
    pub radius: f32,
    /// 起始角度（弧度）
    pub start_angle: f32,
    /// 终止角度（弧度）
    pub end_angle: f32,
}

/// 椭圆定义 — 中心 + 长半轴 + 短半轴 + 旋转角
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EllipseDef {
    /// 唯一标识
    pub id: Uuid,
    /// 中心点（引用 PointDef）
    pub center: Uuid,
    /// 长半轴
    pub semi_a: f32,
    /// 短半轴
    pub semi_b: f32,
    /// 旋转角度（弧度）
    pub rotation: f32,
}

/// 圆环定义 — 内外双半径
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnulusDef {
    /// 唯一标识
    pub id: Uuid,
    /// 中心点（引用 PointDef）
    pub center: Uuid,
    /// 内半径
    pub inner_radius: f32,
    /// 外半径
    pub outer_radius: f32,
}

/// 贝塞尔曲线定义 — 二阶/三阶，控制点可拖拽
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BezierDef {
    /// 唯一标识
    pub id: Uuid,
    /// 控制点列表（2 = 二阶，3 = 三阶）
    /// 第一个和最后一个点是端点，中间是控制点
    pub control_points: Vec<Uuid>,
}

/// 角度标注定义 — 弧 + 数值
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AngleMarkDef {
    /// 唯一标识
    pub id: Uuid,
    /// 角顶点（引用 PointDef）
    pub vertex: Uuid,
    /// 第一条边上的点
    pub point_a: Uuid,
    /// 第二条边上的点
    pub point_b: Uuid,
}

/// 长度标注定义 — 线段 + 文本
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LengthMarkDef {
    /// 唯一标识
    pub id: Uuid,
    /// 起点（引用 PointDef）
    pub start: Uuid,
    /// 终点（引用 PointDef）
    pub end: Uuid,
}

/// 三角形类型 — 等边、等腰、直角
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum TriangleType {
    /// 任意三角形（无约束）
    #[default]
    Scalene,
    /// 等边三角形 — 三边等长
    Equilateral,
    /// 等腰三角形 — 两边等长
    Isosceles,
    /// 直角三角形 — 含一个 90° 角
    RightAngled,
}

/// 三角形定义 — 三个顶点 + 类型约束
///
/// 支持拖拽顶点动态变形。对于等边/等腰/直角类型，
/// 拖拽一个顶点时其余顶点按约束自动调整。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriangleDef {
    /// 唯一标识
    pub id: Uuid,
    /// 顶点 A（引用 PointDef）
    pub vertex_a: Uuid,
    /// 顶点 B（引用 PointDef）
    pub vertex_b: Uuid,
    /// 顶点 C（引用 PointDef）
    pub vertex_c: Uuid,
    /// 三角形类型约束
    pub triangle_type: TriangleType,
}

/// 坐标系网格定义 — 可选显示
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridDef {
    /// 唯一标识
    pub id: Uuid,
    /// 网格原点（引用 PointDef，通常为自由点 (0,0)）
    pub origin: Uuid,
    /// 网格间距（世界坐标单位）
    pub spacing: f32,
    /// 是否显示主刻度
    pub show_major: bool,
    /// 主刻度间隔（每隔多少格画一条粗线）
    pub major_every: u32,
    /// 是否显示坐标轴标签
    pub show_labels: bool,
}

// ── 3D 基本体定义 ───────────────────────────────────────────────

/// 立方体定义 — 中心点 + 边长
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CubeDef {
    /// 唯一标识
    pub id: Uuid,
    /// 中心（引用一个 3D PointDef）
    pub center: Uuid,
    /// 边长
    pub size: f32,
}

/// 长方体定义 — 中心 + 长 + 宽 + 高
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuboidDef {
    /// 唯一标识
    pub id: Uuid,
    /// 中心（引用一个 3D PointDef）
    pub center: Uuid,
    /// 长（X 方向）
    pub width: f32,
    /// 高（Y 方向）
    pub height: f32,
    /// 深（Z 方向）
    pub depth: f32,
}

/// 球体定义 — 中心点 + 半径
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SphereDef {
    /// 唯一标识
    pub id: Uuid,
    /// 中心（引用一个 3D PointDef）
    pub center: Uuid,
    /// 半径
    pub radius: f32,
}

/// 圆柱定义 — 底面圆心 + 顶面圆心 + 半径
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CylinderDef {
    /// 唯一标识
    pub id: Uuid,
    /// 底面圆心
    pub bottom_center: Uuid,
    /// 顶面圆心
    pub top_center: Uuid,
    /// 半径
    pub radius: f32,
}

/// 圆锥定义 — 底面圆心 + 顶点 + 底面半径
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConeDef {
    /// 唯一标识
    pub id: Uuid,
    /// 底面圆心
    pub base_center: Uuid,
    /// 顶点
    pub apex: Uuid,
    /// 底面半径
    pub radius: f32,
}

/// 圆台定义 — 底面圆心 + 顶面圆心 + 底半径 + 顶半径
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrustumDef {
    /// 唯一标识
    pub id: Uuid,
    /// 底面圆心
    pub bottom_center: Uuid,
    /// 顶面圆心
    pub top_center: Uuid,
    /// 底面半径
    pub bottom_radius: f32,
    /// 顶面半径
    pub top_radius: f32,
}

/// 棱柱定义 — 底面圆心 + 顶面圆心 + 半径 + 边数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrismDef {
    /// 唯一标识
    pub id: Uuid,
    /// 底面圆心
    pub base_center: Uuid,
    /// 顶面圆心
    pub top_center: Uuid,
    /// 外接圆半径
    pub radius: f32,
    /// 边数（≥3）
    pub sides: u32,
}

/// 棱锥定义 — 底面圆心 + 顶点 + 半径 + 边数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PyramidDef {
    /// 唯一标识
    pub id: Uuid,
    /// 底面圆心
    pub base_center: Uuid,
    /// 顶点
    pub apex: Uuid,
    /// 外接圆半径
    pub radius: f32,
    /// 边数（≥3）
    pub sides: u32,
}

/// 正多面体类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum PolyhedronType {
    /// 正四面体（4 面）
    Tetrahedron,
    /// 正六面体（即立方体，6 面）
    Hexahedron,
    /// 正八面体（8 面）
    Octahedron,
    /// 正十二面体（12 面）
    Dodecahedron,
    /// 正二十面体（20 面）
    Icosahedron,
}

/// 正多面体定义 — 中心 + 大小 + 类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegularPolyhedronDef {
    /// 唯一标识
    pub id: Uuid,
    /// 中心（引用一个 3D PointDef）
    pub center: Uuid,
    /// 大小（外接球半径）
    pub size: f32,
    /// 多面体类型
    pub poly_type: PolyhedronType,
}

// ── 几何文档 ────────────────────────────────────────────────────

/// 完整的几何文档 — 可序列化的整体状态
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GeometryDoc {
    /// 所有点定义
    pub points: HashMap<Uuid, PointDef>,
    /// 所有线定义
    pub lines: HashMap<Uuid, LineDef>,
    /// 所有圆定义
    pub circles: HashMap<Uuid, CircleDef>,
    // ── 2D 图形扩展 ──
    /// 多边形
    pub polygons: HashMap<Uuid, PolygonDef>,
    /// 正多边形
    pub regular_polygons: HashMap<Uuid, RegularPolygonDef>,
    /// 圆弧
    pub arcs: HashMap<Uuid, ArcDef>,
    /// 扇形
    pub sectors: HashMap<Uuid, SectorDef>,
    /// 椭圆
    pub ellipses: HashMap<Uuid, EllipseDef>,
    /// 圆环
    pub annuli: HashMap<Uuid, AnnulusDef>,
    /// 贝塞尔曲线
    pub beziers: HashMap<Uuid, BezierDef>,
    /// 角度标注
    pub angle_marks: HashMap<Uuid, AngleMarkDef>,
    /// 长度标注
    pub length_marks: HashMap<Uuid, LengthMarkDef>,
    /// 三角形
    pub triangles: HashMap<Uuid, TriangleDef>,
    /// 坐标系网格
    pub grids: HashMap<Uuid, GridDef>,
    // ── 3D 基本体 ──
    /// 立方体
    pub cubes: HashMap<Uuid, CubeDef>,
    /// 长方体
    pub cuboids: HashMap<Uuid, CuboidDef>,
    /// 球体
    pub spheres: HashMap<Uuid, SphereDef>,
    /// 圆柱
    pub cylinders: HashMap<Uuid, CylinderDef>,
    /// 圆锥
    pub cones: HashMap<Uuid, ConeDef>,
    /// 圆台
    pub frustums: HashMap<Uuid, FrustumDef>,
    /// 棱柱
    pub prisms: HashMap<Uuid, PrismDef>,
    /// 棱锥
    pub pyramids: HashMap<Uuid, PyramidDef>,
    /// 正多面体
    pub regular_polyhedra: HashMap<Uuid, RegularPolyhedronDef>,
}

impl GeometryDoc {
    /// 创建空文档
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加自由点，返回其 Uuid
    pub fn add_free_point(&mut self, pos: Point2D) -> Uuid {
        let id = Uuid::new_v4();
        self.points.insert(id, PointDef::Free { pos });
        id
    }

    /// 添加 3D 自由点
    pub fn add_free_point_3d(&mut self, pos: Point3D) -> Uuid {
        let id = Uuid::new_v4();
        self.points.insert(id, PointDef::Free3D { pos });
        id
    }

    /// 添加中点
    pub fn add_midpoint(&mut self, a: Uuid, b: Uuid) -> Uuid {
        let id = Uuid::new_v4();
        self.points.insert(id, PointDef::Midpoint { a, b });
        id
    }

    /// 添加线段
    pub fn add_line(&mut self, start: Uuid, end: Uuid) -> Uuid {
        let id = Uuid::new_v4();
        self.lines.insert(id, LineDef { id, start, end });
        id
    }

    /// 添加圆
    pub fn add_circle(&mut self, center: Uuid, radius: f32) -> Uuid {
        let id = Uuid::new_v4();
        self.circles.insert(id, CircleDef { id, center, radius });
        id
    }

    // ── 2D 图形添加方法 ──

    /// 添加多边形
    pub fn add_polygon(&mut self, vertices: Vec<Uuid>) -> Uuid {
        let id = Uuid::new_v4();
        self.polygons.insert(id, PolygonDef { id, vertices });
        id
    }

    /// 添加正多边形
    pub fn add_regular_polygon(
        &mut self,
        center: Uuid,
        radius: f32,
        sides: u32,
        rotation: f32,
    ) -> Uuid {
        let id = Uuid::new_v4();
        self.regular_polygons.insert(
            id,
            RegularPolygonDef {
                id,
                center,
                radius,
                sides,
                rotation,
            },
        );
        id
    }

    /// 添加圆弧
    pub fn add_arc(
        &mut self,
        center: Uuid,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
    ) -> Uuid {
        let id = Uuid::new_v4();
        self.arcs.insert(
            id,
            ArcDef {
                id,
                center,
                radius,
                start_angle,
                end_angle,
            },
        );
        id
    }

    /// 添加扇形
    pub fn add_sector(
        &mut self,
        center: Uuid,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
    ) -> Uuid {
        let id = Uuid::new_v4();
        self.sectors.insert(
            id,
            SectorDef {
                id,
                center,
                radius,
                start_angle,
                end_angle,
            },
        );
        id
    }

    /// 添加椭圆
    pub fn add_ellipse(
        &mut self,
        center: Uuid,
        semi_a: f32,
        semi_b: f32,
        rotation: f32,
    ) -> Uuid {
        let id = Uuid::new_v4();
        self.ellipses.insert(
            id,
            EllipseDef {
                id,
                center,
                semi_a,
                semi_b,
                rotation,
            },
        );
        id
    }

    /// 添加圆环
    pub fn add_annulus(&mut self, center: Uuid, inner_radius: f32, outer_radius: f32) -> Uuid {
        let id = Uuid::new_v4();
        self.annuli.insert(
            id,
            AnnulusDef {
                id,
                center,
                inner_radius,
                outer_radius,
            },
        );
        id
    }

    /// 添加贝塞尔曲线
    pub fn add_bezier(&mut self, control_points: Vec<Uuid>) -> Uuid {
        let id = Uuid::new_v4();
        self.beziers.insert(id, BezierDef { id, control_points });
        id
    }

    /// 添加角度标注
    pub fn add_angle_mark(&mut self, vertex: Uuid, point_a: Uuid, point_b: Uuid) -> Uuid {
        let id = Uuid::new_v4();
        self.angle_marks.insert(
            id,
            AngleMarkDef {
                id,
                vertex,
                point_a,
                point_b,
            },
        );
        id
    }

    /// 添加长度标注
    pub fn add_length_mark(&mut self, start: Uuid, end: Uuid) -> Uuid {
        let id = Uuid::new_v4();
        self.length_marks.insert(id, LengthMarkDef { id, start, end });
        id
    }

    /// 添加三角形
    pub fn add_triangle(
        &mut self,
        vertex_a: Uuid,
        vertex_b: Uuid,
        vertex_c: Uuid,
        triangle_type: TriangleType,
    ) -> Uuid {
        let id = Uuid::new_v4();
        self.triangles.insert(
            id,
            TriangleDef {
                id,
                vertex_a,
                vertex_b,
                vertex_c,
                triangle_type,
            },
        );
        id
    }

    /// 添加坐标系网格
    pub fn add_grid(
        &mut self,
        origin: Uuid,
        spacing: f32,
        show_major: bool,
        major_every: u32,
        show_labels: bool,
    ) -> Uuid {
        let id = Uuid::new_v4();
        self.grids.insert(
            id,
            GridDef {
                id,
                origin,
                spacing,
                show_major,
                major_every,
                show_labels,
            },
        );
        id
    }

    // ── 3D 基本体添加方法 ──

    /// 添加立方体
    pub fn add_cube(&mut self, center: Uuid, size: f32) -> Uuid {
        let id = Uuid::new_v4();
        self.cubes.insert(id, CubeDef { id, center, size });
        id
    }

    /// 添加长方体
    pub fn add_cuboid(&mut self, center: Uuid, width: f32, height: f32, depth: f32) -> Uuid {
        let id = Uuid::new_v4();
        self.cuboids.insert(
            id,
            CuboidDef {
                id,
                center,
                width,
                height,
                depth,
            },
        );
        id
    }

    /// 添加球体
    pub fn add_sphere(&mut self, center: Uuid, radius: f32) -> Uuid {
        let id = Uuid::new_v4();
        self.spheres.insert(id, SphereDef { id, center, radius });
        id
    }

    /// 添加圆柱
    pub fn add_cylinder(
        &mut self,
        bottom_center: Uuid,
        top_center: Uuid,
        radius: f32,
    ) -> Uuid {
        let id = Uuid::new_v4();
        self.cylinders.insert(
            id,
            CylinderDef {
                id,
                bottom_center,
                top_center,
                radius,
            },
        );
        id
    }

    /// 添加圆锥
    pub fn add_cone(&mut self, base_center: Uuid, apex: Uuid, radius: f32) -> Uuid {
        let id = Uuid::new_v4();
        self.cones.insert(
            id,
            ConeDef {
                id,
                base_center,
                apex,
                radius,
            },
        );
        id
    }

    /// 添加圆台
    pub fn add_frustum(
        &mut self,
        bottom_center: Uuid,
        top_center: Uuid,
        bottom_radius: f32,
        top_radius: f32,
    ) -> Uuid {
        let id = Uuid::new_v4();
        self.frustums.insert(
            id,
            FrustumDef {
                id,
                bottom_center,
                top_center,
                bottom_radius,
                top_radius,
            },
        );
        id
    }

    /// 添加棱柱
    pub fn add_prism(
        &mut self,
        base_center: Uuid,
        top_center: Uuid,
        radius: f32,
        sides: u32,
    ) -> Uuid {
        let id = Uuid::new_v4();
        self.prisms.insert(
            id,
            PrismDef {
                id,
                base_center,
                top_center,
                radius,
                sides,
            },
        );
        id
    }

    /// 添加棱锥
    pub fn add_pyramid(
        &mut self,
        base_center: Uuid,
        apex: Uuid,
        radius: f32,
        sides: u32,
    ) -> Uuid {
        let id = Uuid::new_v4();
        self.pyramids.insert(
            id,
            PyramidDef {
                id,
                base_center,
                apex,
                radius,
                sides,
            },
        );
        id
    }

    /// 添加正多面体
    pub fn add_regular_polyhedron(
        &mut self,
        center: Uuid,
        size: f32,
        poly_type: PolyhedronType,
    ) -> Uuid {
        let id = Uuid::new_v4();
        self.regular_polyhedra.insert(
            id,
            RegularPolyhedronDef {
                id,
                center,
                size,
                poly_type,
            },
        );
        id
    }

    /// 更新自由点位置
    pub fn update_free_point(&mut self, id: Uuid, new_pos: Point2D) {
        if let Some(PointDef::Free { pos }) = self.points.get_mut(&id) {
            *pos = new_pos;
        }
    }

    /// 更新 3D 自由点位置
    pub fn update_free_point_3d(&mut self, id: Uuid, new_pos: Point3D) {
        if let Some(PointDef::Free3D { pos }) = self.points.get_mut(&id) {
            *pos = new_pos;
        }
    }

    /// 删除元素（同时清理引用它的其他元素）
    pub fn remove_element(&mut self, id: Uuid) {
        self.points.remove(&id);
        self.lines.remove(&id);
        self.circles.remove(&id);
        self.polygons.remove(&id);
        self.regular_polygons.remove(&id);
        self.arcs.remove(&id);
        self.sectors.remove(&id);
        self.ellipses.remove(&id);
        self.annuli.remove(&id);
        self.beziers.remove(&id);
        self.angle_marks.remove(&id);
        self.length_marks.remove(&id);
        self.triangles.remove(&id);
        self.grids.remove(&id);
        self.cubes.remove(&id);
        self.cuboids.remove(&id);
        self.spheres.remove(&id);
        self.cylinders.remove(&id);
        self.cones.remove(&id);
        self.frustums.remove(&id);
        self.prisms.remove(&id);
        self.pyramids.remove(&id);
        self.regular_polyhedra.remove(&id);

        // 清理引用了被删除元素的线/圆
        self.lines.retain(|_, l| l.start != id && l.end != id);
        self.circles.retain(|_, c| c.center != id);
        // 清理多边形中引用被删除点的条目
        self.polygons.retain(|_, p| !p.vertices.contains(&id));
        self.regular_polygons.retain(|_, p| p.center != id);
        self.arcs.retain(|_, a| a.center != id);
        self.sectors.retain(|_, s| s.center != id);
        self.ellipses.retain(|_, e| e.center != id);
        self.annuli.retain(|_, a| a.center != id);
        self.beziers.retain(|_, b| !b.control_points.contains(&id));
        self.angle_marks
            .retain(|_, a| a.vertex != id && a.point_a != id && a.point_b != id);
        self.length_marks
            .retain(|_, l| l.start != id && l.end != id);
        // 三角形：任一顶点被删则删除
        self.triangles.retain(|_, t| {
            t.vertex_a != id && t.vertex_b != id && t.vertex_c != id
        });
        // 网格：原点被删则删除
        self.grids.retain(|_, g| g.origin != id);
        // 3D
        self.cubes.retain(|_, c| c.center != id);
        self.cuboids.retain(|_, c| c.center != id);
        self.spheres.retain(|_, s| s.center != id);
        self.cones
            .retain(|_, c| c.base_center != id && c.apex != id);
        self.cylinders
            .retain(|_, c| c.bottom_center != id && c.top_center != id);
        self.frustums
            .retain(|_, f| f.bottom_center != id && f.top_center != id);
        self.prisms
            .retain(|_, p| p.base_center != id && p.top_center != id);
        self.pyramids
            .retain(|_, p| p.base_center != id && p.apex != id);
        self.regular_polyhedra.retain(|_, p| p.center != id);
    }

    /// 获取所有点 ID 列表
    pub fn point_ids(&self) -> Vec<Uuid> {
        self.points.keys().copied().collect()
    }

    /// 获取所有线 ID 列表
    pub fn line_ids(&self) -> Vec<Uuid> {
        self.lines.keys().copied().collect()
    }

    /// 获取所有圆 ID 列表
    pub fn circle_ids(&self) -> Vec<Uuid> {
        self.circles.keys().copied().collect()
    }

    /// 统计 2D 图形总数
    pub fn count_2d_shapes(&self) -> usize {
        self.lines.len()
            + self.circles.len()
            + self.polygons.len()
            + self.regular_polygons.len()
            + self.arcs.len()
            + self.sectors.len()
            + self.ellipses.len()
            + self.annuli.len()
            + self.beziers.len()
            + self.angle_marks.len()
            + self.length_marks.len()
            + self.triangles.len()
            + self.grids.len()
    }

    /// 统计 3D 图形总数
    pub fn count_3d_shapes(&self) -> usize {
        self.cubes.len()
            + self.cuboids.len()
            + self.spheres.len()
            + self.cylinders.len()
            + self.cones.len()
            + self.frustums.len()
            + self.prisms.len()
            + self.pyramids.len()
            + self.regular_polyhedra.len()
    }
}

// ── 测试 ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_free_point() {
        let mut doc = GeometryDoc::new();
        let id = doc.add_free_point(Point2D::new(10.0, 20.0));
        assert_eq!(doc.points.len(), 1);

        match &doc.points[&id] {
            PointDef::Free { pos } => {
                assert_eq!(pos.x, 10.0);
                assert_eq!(pos.y, 20.0);
            }
            _ => panic!("Expected Free point"),
        }
    }

    #[test]
    fn test_line_references() {
        let mut doc = GeometryDoc::new();
        let p1 = doc.add_free_point(Point2D::new(0.0, 0.0));
        let p2 = doc.add_free_point(Point2D::new(10.0, 0.0));
        let line_id = doc.add_line(p1, p2);

        let line = &doc.lines[&line_id];
        assert_eq!(line.start, p1);
        assert_eq!(line.end, p2);
    }

    #[test]
    fn test_midpoint_dependencies() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let def = PointDef::Midpoint { a, b };
        let deps = def.dependencies();
        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&a));
        assert!(deps.contains(&b));
    }

    #[test]
    fn test_remove_cascades() {
        let mut doc = GeometryDoc::new();
        let p1 = doc.add_free_point(Point2D::new(0.0, 0.0));
        let p2 = doc.add_free_point(Point2D::new(10.0, 0.0));
        let _line = doc.add_line(p1, p2);

        doc.remove_element(p1);
        assert_eq!(doc.lines.len(), 0);
        assert_eq!(doc.points.len(), 1);
    }

    #[test]
    fn test_circle_def() {
        let mut doc = GeometryDoc::new();
        let center = doc.add_free_point(Point2D::new(5.0, 5.0));
        let circle_id = doc.add_circle(center, 3.0);

        let circle = &doc.circles[&circle_id];
        assert_eq!(circle.center, center);
        assert_eq!(circle.radius, 3.0);
    }

    #[test]
    fn test_3d_point() {
        let mut doc = GeometryDoc::new();
        let id = doc.add_free_point_3d(Point3D::new(1.0, 2.0, 3.0));
        match &doc.points[&id] {
            PointDef::Free3D { pos } => {
                assert_eq!(pos.x, 1.0);
                assert_eq!(pos.y, 2.0);
                assert_eq!(pos.z, 3.0);
            }
            _ => panic!("Expected Free3D point"),
        }
    }

    #[test]
    fn test_cube_and_sphere() {
        let mut doc = GeometryDoc::new();
        let center = doc.add_free_point_3d(Point3D::new(0.0, 0.0, 0.0));
        let cube_id = doc.add_cube(center, 2.0);
        let sphere_id = doc.add_sphere(center, 1.5);

        assert_eq!(doc.cubes[&cube_id].size, 2.0);
        assert_eq!(doc.spheres[&sphere_id].radius, 1.5);
    }

    #[test]
    fn test_polygon() {
        let mut doc = GeometryDoc::new();
        let p1 = doc.add_free_point(Point2D::new(0.0, 0.0));
        let p2 = doc.add_free_point(Point2D::new(10.0, 0.0));
        let p3 = doc.add_free_point(Point2D::new(5.0, 8.0));
        let poly_id = doc.add_polygon(vec![p1, p2, p3]);

        assert_eq!(doc.polygons[&poly_id].vertices.len(), 3);
    }

    #[test]
    fn test_regular_polygon() {
        let mut doc = GeometryDoc::new();
        let center = doc.add_free_point(Point2D::new(0.0, 0.0));
        let id = doc.add_regular_polygon(center, 5.0, 6, 0.0);

        assert_eq!(doc.regular_polygons[&id].sides, 6);
        assert_eq!(doc.regular_polygons[&id].radius, 5.0);
    }

    #[test]
    fn test_arc_and_sector() {
        let mut doc = GeometryDoc::new();
        let center = doc.add_free_point(Point2D::new(0.0, 0.0));
        let arc_id = doc.add_arc(center, 3.0, 0.0, std::f32::consts::FRAC_PI_2);
        let sector_id = doc.add_sector(center, 3.0, 0.0, std::f32::consts::FRAC_PI_2);

        assert_eq!(doc.arcs[&arc_id].radius, 3.0);
        assert_eq!(doc.sectors[&sector_id].end_angle, std::f32::consts::FRAC_PI_2);
    }

    #[test]
    fn test_ellipse() {
        let mut doc = GeometryDoc::new();
        let center = doc.add_free_point(Point2D::new(0.0, 0.0));
        let id = doc.add_ellipse(center, 5.0, 3.0, 0.0);

        assert_eq!(doc.ellipses[&id].semi_a, 5.0);
        assert_eq!(doc.ellipses[&id].semi_b, 3.0);
    }

    #[test]
    fn test_annulus() {
        let mut doc = GeometryDoc::new();
        let center = doc.add_free_point(Point2D::new(0.0, 0.0));
        let id = doc.add_annulus(center, 2.0, 5.0);

        assert_eq!(doc.annuli[&id].inner_radius, 2.0);
        assert_eq!(doc.annuli[&id].outer_radius, 5.0);
    }

    #[test]
    fn test_bezier() {
        let mut doc = GeometryDoc::new();
        let p1 = doc.add_free_point(Point2D::new(0.0, 0.0));
        let p2 = doc.add_free_point(Point2D::new(5.0, 10.0));
        let p3 = doc.add_free_point(Point2D::new(10.0, 0.0));
        let id = doc.add_bezier(vec![p1, p2, p3]);

        assert_eq!(doc.beziers[&id].control_points.len(), 3);
    }

    #[test]
    fn test_cuboid() {
        let mut doc = GeometryDoc::new();
        let center = doc.add_free_point_3d(Point3D::new(0.0, 0.0, 0.0));
        let id = doc.add_cuboid(center, 3.0, 2.0, 1.0);

        assert_eq!(doc.cuboids[&id].width, 3.0);
        assert_eq!(doc.cuboids[&id].height, 2.0);
    }

    #[test]
    fn test_frustum() {
        let mut doc = GeometryDoc::new();
        let b = doc.add_free_point_3d(Point3D::new(0.0, 0.0, 0.0));
        let t = doc.add_free_point_3d(Point3D::new(0.0, 3.0, 0.0));
        let id = doc.add_frustum(b, t, 2.0, 1.0);

        assert_eq!(doc.frustums[&id].bottom_radius, 2.0);
        assert_eq!(doc.frustums[&id].top_radius, 1.0);
    }

    #[test]
    fn test_prism_and_pyramid() {
        let mut doc = GeometryDoc::new();
        let b = doc.add_free_point_3d(Point3D::new(0.0, 0.0, 0.0));
        let t = doc.add_free_point_3d(Point3D::new(0.0, 3.0, 0.0));
        let prism_id = doc.add_prism(b, t, 2.0, 6);
        let apex = doc.add_free_point_3d(Point3D::new(0.0, 4.0, 0.0));
        let pyr_id = doc.add_pyramid(b, apex, 2.0, 4);

        assert_eq!(doc.prisms[&prism_id].sides, 6);
        assert_eq!(doc.pyramids[&pyr_id].sides, 4);
    }

    #[test]
    fn test_regular_polyhedron() {
        let mut doc = GeometryDoc::new();
        let center = doc.add_free_point_3d(Point3D::new(0.0, 0.0, 0.0));
        let id = doc.add_regular_polyhedron(center, 2.0, PolyhedronType::Icosahedron);

        assert_eq!(
            doc.regular_polyhedra[&id].poly_type,
            PolyhedronType::Icosahedron
        );
    }

    #[test]
    fn test_remove_polygon_cascade() {
        let mut doc = GeometryDoc::new();
        let p1 = doc.add_free_point(Point2D::new(0.0, 0.0));
        let p2 = doc.add_free_point(Point2D::new(10.0, 0.0));
        let p3 = doc.add_free_point(Point2D::new(5.0, 8.0));
        doc.add_polygon(vec![p1, p2, p3]);

        doc.remove_element(p1);
        assert_eq!(doc.polygons.len(), 0);
    }

    #[test]
    fn test_count_shapes() {
        let mut doc = GeometryDoc::new();
        let p1 = doc.add_free_point(Point2D::new(0.0, 0.0));
        let p2 = doc.add_free_point(Point2D::new(10.0, 0.0));
        doc.add_line(p1, p2);
        doc.add_circle(p1, 3.0);
        doc.add_regular_polygon(p1, 5.0, 6, 0.0);

        assert_eq!(doc.count_2d_shapes(), 3);
        assert_eq!(doc.count_3d_shapes(), 0);
    }

    #[test]
    fn test_triangle() {
        let mut doc = GeometryDoc::new();
        let a = doc.add_free_point(Point2D::new(0.0, 0.0));
        let b = doc.add_free_point(Point2D::new(4.0, 0.0));
        let c = doc.add_free_point(Point2D::new(2.0, 3.0));
        let id = doc.add_triangle(a, b, c, TriangleType::Scalene);

        assert_eq!(doc.triangles[&id].vertex_a, a);
        assert_eq!(doc.triangles[&id].vertex_b, b);
        assert_eq!(doc.triangles[&id].vertex_c, c);
        assert_eq!(doc.triangles[&id].triangle_type, TriangleType::Scalene);
    }

    #[test]
    fn test_triangle_equilateral() {
        let mut doc = GeometryDoc::new();
        let a = doc.add_free_point(Point2D::new(0.0, 0.0));
        let b = doc.add_free_point(Point2D::new(2.0, 0.0));
        let c = doc.add_free_point(Point2D::new(1.0, 1.732));
        let id = doc.add_triangle(a, b, c, TriangleType::Equilateral);

        assert_eq!(doc.triangles[&id].triangle_type, TriangleType::Equilateral);
    }

    #[test]
    fn test_grid() {
        let mut doc = GeometryDoc::new();
        let origin = doc.add_free_point(Point2D::new(0.0, 0.0));
        let id = doc.add_grid(origin, 50.0, true, 5, true);

        assert_eq!(doc.grids[&id].spacing, 50.0);
        assert!(doc.grids[&id].show_major);
        assert_eq!(doc.grids[&id].major_every, 5);
        assert!(doc.grids[&id].show_labels);
    }

    #[test]
    fn test_remove_triangle_cascade() {
        let mut doc = GeometryDoc::new();
        let a = doc.add_free_point(Point2D::new(0.0, 0.0));
        let b = doc.add_free_point(Point2D::new(4.0, 0.0));
        let c = doc.add_free_point(Point2D::new(2.0, 3.0));
        doc.add_triangle(a, b, c, TriangleType::Scalene);

        doc.remove_element(a);
        assert_eq!(doc.triangles.len(), 0);
    }

    #[test]
    fn test_remove_grid_cascade() {
        let mut doc = GeometryDoc::new();
        let origin = doc.add_free_point(Point2D::new(0.0, 0.0));
        doc.add_grid(origin, 50.0, true, 5, true);

        doc.remove_element(origin);
        assert_eq!(doc.grids.len(), 0);
    }

    #[test]
    fn test_triangle_grid_serde() {
        let mut doc = GeometryDoc::new();
        let a = doc.add_free_point(Point2D::new(0.0, 0.0));
        let b = doc.add_free_point(Point2D::new(4.0, 0.0));
        let c = doc.add_free_point(Point2D::new(2.0, 3.0));
        doc.add_triangle(a, b, c, TriangleType::Isosceles);
        doc.add_grid(a, 50.0, true, 5, false);

        let json = serde_json::to_string(&doc).unwrap();
        let restored: GeometryDoc = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.triangles.len(), 1);
        assert_eq!(restored.grids.len(), 1);
        assert_eq!(
            restored.triangles.values().next().unwrap().triangle_type,
            TriangleType::Isosceles
        );
        assert!(!restored.grids.values().next().unwrap().show_labels);
    }
}
