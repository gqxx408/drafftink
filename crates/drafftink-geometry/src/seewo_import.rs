//! 希沃 EasiNote XML 导入 — 解析 Slide_x.xml 中的 3D 几何体
//!
//! # 数据格式
//! 希沃使用以下字段存储 3D 对象：
//! - `Size`: 局部缩放 (sx, sy, sz)，如 "1,2.825,1"
//! - `Transform3D`: 4x4 变换矩阵（行优先，逗号分隔），如 "1,0,0,0,0,1,0,0,..."
//! - `X`, `Y`: 2D 屏幕坐标偏移
//! - `Width`, `Height`: 2D 包围盒尺寸
//! - `Rotation`: 2D 旋转角度（度）
//! - `EdgeThickness`: 边线粗细
//! - `EdgeBrush.ColorBrush`: 边线颜色（ARGB hex）
//! - `Surfaces`: 表面列表（希沃默认全透明 #00FFFFFF）
//! - `Edges`: 边线列表（希沃默认黑色 #FF000000）
//!
//! # 渲染策略
//! - 兼容模式：纯线框，黑线白底，完美复刻希沃
//! - 默认模式：实心 Lambert 光照 + 黑色边线叠加

use anyhow::{Context, Result};
use nalgebra::{Matrix4, Vector3};
use quick_xml::events::Event;
use quick_xml::Reader;

use crate::definitions::Point3D;
use crate::primitives3d::{self, Mesh3D};

// ── 颜色解析 ────────────────────────────────────────────────────

/// 解析 ARGB hex 颜色字符串（如 "#FF000000" → (255, 0, 0, 0)）
pub fn parse_argb_hex(hex: &str) -> (u8, u8, u8, u8) {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 8 {
        let a = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
        let r = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
        let g = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
        let b = u8::from_str_radix(&hex[6..8], 16).unwrap_or(0);
        (a, r, g, b)
    } else if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
        (255, r, g, b)
    } else {
        (255, 0, 0, 0)
    }
}

// ── 矩阵解析 ────────────────────────────────────────────────────

/// 解析 Transform3D 字符串为 nalgebra::Matrix4<f32>
///
/// 希沃格式：行优先，逗号分隔，16 个浮点数
/// "1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1" → 单位矩阵
pub fn parse_transform_3d(s: &str) -> Result<Matrix4<f32>> {
    let values: Vec<f32> = s
        .split(',')
        .map(|v| v.trim().parse::<f32>())
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("Transform3D 解析失败")?;

    if values.len() != 16 {
        anyhow::bail!(
            "Transform3D 需要 16 个值，实际 {}",
            values.len()
        );
    }

    // 行优先填充
    Ok(Matrix4::new(
        values[0], values[1], values[2], values[3],
        values[4], values[5], values[6], values[7],
        values[8], values[9], values[10], values[11],
        values[12], values[13], values[14], values[15],
    ))
}

/// 解析 Size 字符串为 (sx, sy, sz)
pub fn parse_size(s: &str) -> Result<(f32, f32, f32)> {
    let parts: Vec<f32> = s
        .split(',')
        .map(|v| v.trim().parse::<f32>())
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("Size 解析失败")?;

    if parts.len() != 3 {
        anyhow::bail!("Size 需要 3 个值，实际 {}", parts.len());
    }

    Ok((parts[0], parts[1], parts[2]))
}

// ── 希沃 3D 对象结构体 ─────────────────────────────────────────

/// 希沃圆柱体（严格对应 XML 字段）
#[derive(Debug, Clone)]
pub struct SeewoCylinder {
    /// 唯一标识
    pub id: String,
    /// 局部缩放 (sx, sy, sz)
    pub size: (f32, f32, f32),
    /// 4x4 变换矩阵
    pub transform: Matrix4<f32>,
    /// 2D 屏幕 X 坐标
    pub x: f32,
    /// 2D 屏幕 Y 坐标
    pub y: f32,
    /// 2D 包围盒宽度
    pub width: f32,
    /// 2D 包围盒高度
    pub height: f32,
    /// 2D 旋转角度（度）
    pub rotation: f32,
    /// 边线粗细
    pub edge_thickness: f32,
    /// 边线颜色 ARGB
    pub edge_color: (u8, u8, u8, u8),
}

/// 希沃圆锥体（严格对应 XML 字段）
#[derive(Debug, Clone)]
pub struct SeewoCone {
    /// 唯一标识
    pub id: String,
    /// 局部缩放 (sx, sy, sz)
    pub size: (f32, f32, f32),
    /// 4x4 变换矩阵
    pub transform: Matrix4<f32>,
    /// 2D 屏幕 X 坐标
    pub x: f32,
    /// 2D 屏幕 Y 坐标
    pub y: f32,
    /// 2D 包围盒宽度
    pub width: f32,
    /// 2D 包围盒高度
    pub height: f32,
    /// 2D 旋转角度（度）
    pub rotation: f32,
    /// 边线粗细
    pub edge_thickness: f32,
    /// 边线颜色 ARGB
    pub edge_color: (u8, u8, u8, u8),
}

/// 从希沃 Slide XML 解析出的所有 3D 对象
#[derive(Debug, Default)]
pub struct SeewoSlide3D {
    /// 圆柱体列表
    pub cylinders: Vec<SeewoCylinder>,
    /// 圆锥体列表
    pub cones: Vec<SeewoCone>,
}

// ── XML 解析 ────────────────────────────────────────────────────

/// 解析希沃 Slide XML，提取所有 3D 几何体
pub fn parse_slide_xml(xml: &str) -> Result<SeewoSlide3D> {
    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);

    let mut result = SeewoSlide3D::default();
    let mut buf = Vec::new();

    // 当前解析状态
    let mut current_element: Option<String> = None; // "Cylinder" | "Cone"
    let mut current_text = String::new();

    // 临时字段收集
    let mut size = (1.0_f32, 1.0, 1.0);
    let mut transform = Matrix4::identity();
    let mut x = 0.0_f32;
    let mut y = 0.0_f32;
    let mut width = 0.0_f32;
    let mut height = 0.0_f32;
    let mut rotation = 0.0_f32;
    let mut edge_thickness = 2.0_f32;
    let mut edge_color: (u8, u8, u8, u8) = (255, 0, 0, 0);
    let mut id = String::new();
    let mut in_edge_brush = false;
    let mut in_stroke = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match name.as_str() {
                    "Cylinder" | "Cone" => {
                        current_element = Some(name.clone());
                        // 重置临时字段
                        size = (1.0, 1.0, 1.0);
                        transform = Matrix4::identity();
                        x = 0.0;
                        y = 0.0;
                        width = 0.0;
                        height = 0.0;
                        rotation = 0.0;
                        edge_thickness = 2.0;
                        edge_color = (255, 0, 0, 0);
                        id = String::new();
                    }
                    "EdgeBrush" => {
                        in_edge_brush = true;
                    }
                    "Stroke" => {
                        in_stroke = true;
                    }
                    _ => {}
                }
                current_text.clear();
            }
            Ok(Event::Text(ref e)) => {
                if current_element.is_some() {
                    current_text.push_str(&e.unescape().unwrap_or_default());
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let text = current_text.trim().to_string();

                match name.as_str() {
                    "Cylinder" => {
                        result.cylinders.push(SeewoCylinder {
                            id: id.clone(),
                            size,
                            transform,
                            x,
                            y,
                            width,
                            height,
                            rotation,
                            edge_thickness,
                            edge_color,
                        });
                        current_element = None;
                    }
                    "Cone" => {
                        result.cones.push(SeewoCone {
                            id: id.clone(),
                            size,
                            transform,
                            x,
                            y,
                            width,
                            height,
                            rotation,
                            edge_thickness,
                            edge_color,
                        });
                        current_element = None;
                    }
                    "Size" => {
                        if let Ok(s) = parse_size(&text) {
                            size = s;
                        }
                    }
                    "Transform3D" => {
                        if let Ok(t) = parse_transform_3d(&text) {
                            transform = t;
                        }
                    }
                    "X" => {
                        x = text.parse().unwrap_or(0.0);
                    }
                    "Y" => {
                        y = text.parse().unwrap_or(0.0);
                    }
                    "Width" if current_element.is_some() => {
                        width = text.parse().unwrap_or(0.0);
                    }
                    "Height" if current_element.is_some() => {
                        height = text.parse().unwrap_or(0.0);
                    }
                    "Rotation" => {
                        rotation = text.parse().unwrap_or(0.0);
                    }
                    "EdgeThickness" => {
                        edge_thickness = text.parse().unwrap_or(2.0);
                    }
                    "ColorBrush" if in_edge_brush || in_stroke => {
                        edge_color = parse_argb_hex(&text);
                    }
                    "Id" if current_element.is_some() => {
                        id = text.clone();
                    }
                    "EdgeBrush" => {
                        in_edge_brush = false;
                    }
                    "Stroke" => {
                        in_stroke = false;
                    }
                    _ => {}
                }
                current_text.clear();
            }
            Ok(Event::Empty(_)) => {}
            Ok(Event::Eof) => break,
            Err(e) => anyhow::bail!("XML 解析错误: {e}"),
            _ => {}
        }
        buf.clear();
    }

    Ok(result)
}

// ── 网格生成（应用希沃变换）────────────────────────────────────

/// 希沃导入的网格数据包
pub struct SeewoMeshData {
    /// 变换后的网格
    pub mesh: Mesh3D,
    /// 边线颜色 ARGB
    pub edge_color: (u8, u8, u8, u8),
    /// 边线粗细
    pub edge_thickness: f32,
    /// 希沃屏幕 X 坐标
    pub screen_x: f32,
    /// 希沃屏幕 Y 坐标
    pub screen_y: f32,
}

/// Y 轴校正矩阵 — 翻转 Y 轴以适配屏幕坐标系
///
/// 希沃的 Y 轴向下（屏幕坐标系），我们的 3D 引擎 Y 轴向上。
/// 在 World Matrix 中翻转 Y 轴，使几何体在引擎中正确朝向。
pub fn y_axis_correction_matrix() -> Matrix4<f32> {
    Matrix4::new(
        1.0,  0.0, 0.0, 0.0,
        0.0, -1.0, 0.0, 0.0,
        0.0,  0.0, 1.0, 0.0,
        0.0,  0.0, 0.0, 1.0,
    )
}

/// 将希沃的 Size（局部缩放）合并到 Transform3D，生成 World Matrix
///
/// 顺序：Transform3D × Scale × Y_flip
/// - Scale：应用局部缩放
/// - Y_flip：翻转 Y 轴以适配引擎坐标系
/// - Transform3D：最终世界变换
pub fn compute_world_matrix(size: (f32, f32, f32), transform: &Matrix4<f32>) -> Matrix4<f32> {
    let scale = Matrix4::new_nonuniform_scaling(&Vector3::new(size.0, size.1, size.2));
    let y_flip = y_axis_correction_matrix();
    transform * scale * y_flip
}

/// 从希沃 Slide 中收集所有 3D 网格
///
/// 返回 `SeewoMeshData` 列表，包含网格、边线属性和屏幕坐标。
/// 调用方可以用 screen_x, screen_y 作为屏幕偏移定位物体。
pub fn collect_all_meshes(slide: &SeewoSlide3D) -> Vec<SeewoMeshData> {
    let mut result = Vec::new();
    for cyl in &slide.cylinders {
        result.push(SeewoMeshData {
            mesh: cylinder_to_mesh(cyl),
            edge_color: cyl.edge_color,
            edge_thickness: cyl.edge_thickness,
            screen_x: cyl.x,
            screen_y: cyl.y,
        });
    }
    for cone in &slide.cones {
        result.push(SeewoMeshData {
            mesh: cone_to_mesh(cone),
            edge_color: cone.edge_color,
            edge_thickness: cone.edge_thickness,
            screen_x: cone.x,
            screen_y: cone.y,
        });
    }
    result
}

/// 从希沃圆柱体生成网格（应用世界矩阵）
///
/// 基础圆柱半径 0.5、高度 1.0（单位圆柱），
/// 通过 Size 缩放后半径 = 0.5 * sx、高度 = 1.0 * sy。
pub fn cylinder_to_mesh(cyl: &SeewoCylinder) -> Mesh3D {
    let radius = 0.5; // 单位圆柱半径
    let height = 1.0; // 单位圆柱高度
    let segments = 32;

    let bottom = Point3D::new(0.0, -height * 0.5, 0.0);
    let top = Point3D::new(0.0, height * 0.5, 0.0);

    let mut mesh = primitives3d::generate_cylinder(bottom, top, radius, segments);

    // 应用 World Matrix（Transform3D × Scale）
    let world = compute_world_matrix(cyl.size, &cyl.transform);
    transform_mesh(&mut mesh, &world);

    mesh
}

/// 从希沃圆锥体生成网格（应用世界矩阵）
///
/// 基础圆锥半径 0.5、高度 1.0（单位圆锥）。
pub fn cone_to_mesh(cone: &SeewoCone) -> Mesh3D {
    let radius = 0.5;
    let height = 1.0;
    let segments = 32;

    let base = Point3D::new(0.0, -height * 0.5, 0.0);
    let apex = Point3D::new(0.0, height * 0.5, 0.0);

    let mut mesh = primitives3d::generate_cone(base, apex, radius, segments);

    let world = compute_world_matrix(cone.size, &cone.transform);
    transform_mesh(&mut mesh, &world);

    mesh
}

/// 对网格的所有顶点应用 4x4 变换矩阵
fn transform_mesh(mesh: &mut Mesh3D, matrix: &Matrix4<f32>) {
    for v in &mut mesh.vertices {
        let p = matrix.transform_point(&nalgebra::Point3::from(*v));
        *v = Vector3::new(p.x, p.y, p.z);
    }
}

// ── 测试 ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_argb_hex() {
        let (a, r, g, b) = parse_argb_hex("#FF000000");
        assert_eq!((a, r, g, b), (255, 0, 0, 0));

        let (a, r, g, b) = parse_argb_hex("#00FFFFFF");
        assert_eq!((a, r, g, b), (0, 255, 255, 255));

        let (a, r, g, b) = parse_argb_hex("#FFFFFFFF");
        assert_eq!((a, r, g, b), (255, 255, 255, 255));
    }

    #[test]
    fn test_parse_transform_3d_identity() {
        let m = parse_transform_3d("1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1").unwrap();
        assert_eq!(m, Matrix4::identity());
    }

    #[test]
    fn test_parse_transform_3d_translation() {
        let m = parse_transform_3d("1,0,0,5,0,1,0,3,0,0,1,0,0,0,0,1").unwrap();
        assert_eq!(m[12], 5.0);
        assert_eq!(m[13], 3.0);
    }

    #[test]
    fn test_parse_size() {
        let (sx, sy, sz) = parse_size("1,2.825,1").unwrap();
        assert!((sx - 1.0).abs() < 1e-6);
        assert!((sy - 2.825).abs() < 1e-6);
        assert!((sz - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_parse_slide_xml_cylinder() {
        let xml = r#"<Slide>
  <Elements>
    <Cylinder>
      <Size>1,2.825,1</Size>
      <Transform3D>1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1</Transform3D>
      <EdgeThickness>2</EdgeThickness>
      <EdgeBrush>
        <ColorBrush>#FF000000</ColorBrush>
      </EdgeBrush>
      <Id>af9bd2a4c35543dcb91e26d68343b97a</Id>
      <X>192.03</X>
      <Y>102.91</Y>
      <Width>273.42</Width>
      <Height>452.78</Height>
      <Rotation>0</Rotation>
    </Cylinder>
  </Elements>
</Slide>"#;

        let slide = parse_slide_xml(xml).unwrap();
        assert_eq!(slide.cylinders.len(), 1);
        assert_eq!(slide.cones.len(), 0);

        let cyl = &slide.cylinders[0];
        assert!((cyl.size.1 - 2.825).abs() < 1e-3);
        assert!((cyl.x - 192.03).abs() < 1e-3);
        assert!((cyl.y - 102.91).abs() < 1e-3);
        assert_eq!(cyl.edge_color, (255, 0, 0, 0));
        assert_eq!(cyl.edge_thickness, 2.0);
    }

    #[test]
    fn test_parse_slide_xml_cone() {
        let xml = r#"<Slide>
  <Elements>
    <Cone>
      <Size>1,2.374,1</Size>
      <Transform3D>1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1</Transform3D>
      <EdgeThickness>2</EdgeThickness>
      <EdgeBrush>
        <ColorBrush>#FF000000</ColorBrush>
      </EdgeBrush>
      <Id>f422c531bbf14262a6b3ad433cb611e7</Id>
      <X>345.04</X>
      <Y>139.90</Y>
      <Width>267.17</Width>
      <Height>342.79</Height>
      <Rotation>180</Rotation>
    </Cone>
  </Elements>
</Slide>"#;

        let slide = parse_slide_xml(xml).unwrap();
        assert_eq!(slide.cones.len(), 1);

        let cone = &slide.cones[0];
        assert!((cone.size.1 - 2.374).abs() < 1e-3);
        assert!((cone.rotation - 180.0).abs() < 1e-3);
    }

    #[test]
    fn test_cylinder_to_mesh() {
        let cyl = SeewoCylinder {
            id: "test".into(),
            size: (1.0, 2.0, 1.0),
            transform: Matrix4::identity(),
            x: 100.0,
            y: 100.0,
            width: 200.0,
            height: 400.0,
            rotation: 0.0,
            edge_thickness: 2.0,
            edge_color: (255, 0, 0, 0),
        };

        let mesh = cylinder_to_mesh(&cyl);
        assert!(mesh.vertices.len() > 8); // 32 segments × 2 rings + 2 centers
        assert!(mesh.edges.len() > 12);
    }

    #[test]
    fn test_cone_to_mesh() {
        let cone = SeewoCone {
            id: "test".into(),
            size: (1.0, 2.0, 1.0),
            transform: Matrix4::identity(),
            x: 100.0,
            y: 100.0,
            width: 200.0,
            height: 300.0,
            rotation: 0.0,
            edge_thickness: 2.0,
            edge_color: (255, 0, 0, 0),
        };

        let mesh = cone_to_mesh(&cone);
        assert!(mesh.vertices.len() > 4);
    }

    #[test]
    fn test_world_matrix_with_scale() {
        let size = (2.0, 3.0, 1.0);
        let transform = Matrix4::identity();
        let world = compute_world_matrix(size, &transform);

        // 应用到点 (1, 0, 0) 应得 (2, 0, 0)（Y=0 不受翻转影响）
        let p = world.transform_point(&nalgebra::Point3::new(1.0, 0.0, 0.0));
        assert!((p.x - 2.0).abs() < 1e-6);
        assert!((p.y - 0.0).abs() < 1e-6);

        // 验证 Y 轴翻转：点 (0, 1, 0) 应得 (0, -3, 0)
        let p2 = world.transform_point(&nalgebra::Point3::new(0.0, 1.0, 0.0));
        assert!((p2.x - 0.0).abs() < 1e-6);
        assert!((p2.y - (-3.0)).abs() < 1e-6);
    }

    #[test]
    fn test_y_axis_correction() {
        let m = y_axis_correction_matrix();
        let p = m.transform_point(&nalgebra::Point3::new(1.0, 2.0, 3.0));
        assert!((p.x - 1.0).abs() < 1e-6);
        assert!((p.y - (-2.0)).abs() < 1e-6);
        assert!((p.z - 3.0).abs() < 1e-6);
    }

    #[test]
    fn test_parse_both_shapes() {
        let xml = r#"<Slide>
  <Elements>
    <Cylinder>
      <Size>1,2,1</Size>
      <Transform3D>1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1</Transform3D>
      <EdgeThickness>2</EdgeThickness>
      <EdgeBrush><ColorBrush>#FF000000</ColorBrush></EdgeBrush>
      <Id>c1</Id>
      <X>100</X><Y>100</Y><Width>200</Width><Height>300</Height><Rotation>0</Rotation>
    </Cylinder>
    <Cone>
      <Size>1,1.5,1</Size>
      <Transform3D>1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1</Transform3D>
      <EdgeThickness>2</EdgeThickness>
      <EdgeBrush><ColorBrush>#FF000000</ColorBrush></EdgeBrush>
      <Id>c2</Id>
      <X>400</X><Y>100</Y><Width>200</Width><Height>200</Height><Rotation>0</Rotation>
    </Cone>
  </Elements>
</Slide>"#;

        let slide = parse_slide_xml(xml).unwrap();
        assert_eq!(slide.cylinders.len(), 1);
        assert_eq!(slide.cones.len(), 1);
    }
}
