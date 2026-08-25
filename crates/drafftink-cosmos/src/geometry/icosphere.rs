//! 二十面体球体（icosphere）生成与细分
//!
//! 提供基于正二十面体细分的球面网格生成，以及传统 UV 球体生成。

use std::collections::HashMap;

use nalgebra::{Point3, Vector3};

/// 3D 网格数据
#[derive(Debug, Clone)]
pub struct MeshData {
    /// 顶点位置
    pub vertices: Vec<Point3<f32>>,
    /// 法线（单位向量）
    pub normals: Vec<Vector3<f32>>,
    /// 纹理坐标
    pub uvs: Vec<[f32; 2]>,
    /// 三角形索引（逆时针正面朝向）
    pub indices: Vec<u32>,
}

impl MeshData {
    /// 创建空的 MeshData
    fn new() -> Self {
        Self {
            vertices: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            indices: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Icosphere 生成
// ---------------------------------------------------------------------------

/// 基于正二十面体细分生成球体网格。
///
/// - `subdivisions = 0`：正二十面体（20 个面，12 个顶点）
/// - 每细分一次，面数 x4，顶点数约 x4
/// - 所有顶点均在单位球面上（半径 = 1）
pub fn icosphere(subdivisions: u32) -> MeshData {
    let (mut vertices, mut faces) = icosahedron();

    for _ in 0..subdivisions {
        let (new_vertices, new_faces) = subdivide(&vertices, &faces);
        vertices = new_vertices;
        faces = new_faces;
    }

    build_mesh_data(vertices, faces)
}

/// 构建正二十面体的初始顶点和面。
///
/// 使用黄金比例 phi = (1 + √5) / 2。
/// 12 个顶点为 (±1, ±phi, 0) 的循环排列，然后归一化到单位球面。
fn icosahedron() -> (Vec<Point3<f32>>, Vec<[u32; 3]>) {
    let phi = (1.0 + 5.0_f32.sqrt()) / 2.0;

    // 12 个顶点：(±1, ±phi, 0) 的循环排列
    let raw = [
        // 循环 0：(±1, ±phi, 0)
        (-1.0, phi, 0.0),
        (1.0, phi, 0.0),
        (-1.0, -phi, 0.0),
        (1.0, -phi, 0.0),
        // 循环 1：(0, ±1, ±phi)
        (0.0, -1.0, phi),
        (0.0, 1.0, phi),
        (0.0, -1.0, -phi),
        (0.0, 1.0, -phi),
        // 循环 2：(±phi, 0, ±1)
        (phi, 0.0, -1.0),
        (phi, 0.0, 1.0),
        (-phi, 0.0, -1.0),
        (-phi, 0.0, 1.0),
    ];

    // 归一化到单位球面
    let vertices: Vec<Point3<f32>> = raw
        .iter()
        .map(|&(x, y, z)| {
            let v = Vector3::new(x, y, z);
            let n = v.normalize();
            Point3::new(n.x, n.y, n.z)
        })
        .collect();

    // 20 个三角形面（索引对应上面顶点顺序）
    // 顺序保证逆时针从球外看为正面
    let faces = vec![
        // 围绕顶点 0 (-1, phi, 0) 的 5 个面
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        // 围绕顶点 3 (1, -phi, 0) 的 5 个面
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        // 其余 10 个面（连接上下两部分）
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];

    (vertices, faces)
}

/// 对当前网格进行一次细分。
///
/// 每条边的中点被投影到单位球面上。
/// 使用 HashMap 去重，确保同一条边的中点只生成一次。
fn subdivide(vertices: &[Point3<f32>], faces: &[[u32; 3]]) -> (Vec<Point3<f32>>, Vec<[u32; 3]>) {
    let mut new_vertices = vertices.to_vec();
    let mut new_faces = Vec::with_capacity(faces.len() * 4);

    // 边中点缓存：key = (较小索引, 较大索引)，value = 中点顶点索引
    let mut midpoint_cache: HashMap<(u32, u32), u32> = HashMap::new();

    for &[i0, i1, i2] in faces {
        // 获取三条边的中点索引（如不存在则创建）
        let m01 = get_or_create_midpoint(i0, i1, vertices, &mut new_vertices, &mut midpoint_cache);
        let m12 = get_or_create_midpoint(i1, i2, vertices, &mut new_vertices, &mut midpoint_cache);
        let m20 = get_or_create_midpoint(i2, i0, vertices, &mut new_vertices, &mut midpoint_cache);

        // 原三角形被分成 4 个小三角形
        // 保持逆时针朝向
        new_faces.push([i0, m01, m20]);
        new_faces.push([i1, m12, m01]);
        new_faces.push([i2, m20, m12]);
        new_faces.push([m01, m12, m20]);
    }

    (new_vertices, new_faces)
}

/// 获取边 (a, b) 的中点顶点索引；若不存在则创建并投影到单位球面。
fn get_or_create_midpoint(
    a: u32,
    b: u32,
    old_vertices: &[Point3<f32>],
    new_vertices: &mut Vec<Point3<f32>>,
    cache: &mut HashMap<(u32, u32), u32>,
) -> u32 {
    // 规范化键：小索引在前
    let key = if a < b { (a, b) } else { (b, a) };

    if let Some(&idx) = cache.get(&key) {
        return idx;
    }

    // 计算中点
    let va = old_vertices[a as usize];
    let vb = old_vertices[b as usize];
    let mid = Point3::new(
        (va.x + vb.x) * 0.5,
        (va.y + vb.y) * 0.5,
        (va.z + vb.z) * 0.5,
    );

    // 投影到单位球面
    let v = Vector3::new(mid.x, mid.y, mid.z);
    let n = v.normalize();
    let projected = Point3::new(n.x, n.y, n.z);

    let idx = new_vertices.len() as u32;
    new_vertices.push(projected);
    cache.insert(key, idx);

    idx
}

/// 从顶点列表和面索引构建 MeshData（计算法线和 UV）。
fn build_mesh_data(vertices: Vec<Point3<f32>>, faces: Vec<[u32; 3]>) -> MeshData {
    let mut mesh = MeshData::new();

    // 法线：对于单位球，法线 = 顶点位置本身（已归一化）
    let normals: Vec<Vector3<f32>> = vertices
        .iter()
        .map(|v| Vector3::new(v.x, v.y, v.z))
        .collect();

    // UV 坐标：等距圆柱投影
    let uvs: Vec<[f32; 2]> = vertices
        .iter()
        .map(|v| {
            let u = v.z.atan2(v.x) / (2.0 * std::f32::consts::PI) + 0.5;
            let v_coord = v.y.asin() / std::f32::consts::PI + 0.5;
            [u, v_coord]
        })
        .collect();

    // 索引
    let indices: Vec<u32> = faces.iter().flat_map(|&[a, b, c]| vec![a, b, c]).collect();

    mesh.vertices = vertices;
    mesh.normals = normals;
    mesh.uvs = uvs;
    mesh.indices = indices;

    mesh
}

// ---------------------------------------------------------------------------
// UV 球体生成（备选方案）
// ---------------------------------------------------------------------------

/// 生成传统 UV 球体网格。
///
/// - `segments`：经度方向分段数（至少 3）
/// - `rings`：纬度方向环数（至少 2，不包含上下极点）
/// - 所有顶点均在单位球面上（半径 = 1）
pub fn sphere_uv(segments: u32, rings: u32) -> MeshData {
    let segments = segments.max(3);
    let rings = rings.max(2);

    let mut mesh = MeshData::new();

    // 顶点数 = 2（极点） + segments * rings
    // 顶部极点索引 = 0
    // 底部极点索引 = 1 + segments * rings

    // 顶部极点
    mesh.vertices.push(Point3::new(0.0, 1.0, 0.0));
    mesh.normals.push(Vector3::new(0.0, 1.0, 0.0));
    mesh.uvs.push([0.5, 1.0]);

    // 中间环
    for ring in 0..rings {
        // v 从接近 1 到接近 0（对应纬度从北极附近到南极附近）
        let v = 1.0 - (ring as f32 + 1.0) / (rings as f32 + 1.0);
        let phi = std::f32::consts::PI * v; // 极角，0 = 北极，π = 南极
        let sin_phi = phi.sin();
        let cos_phi = phi.cos();

        for seg in 0..segments {
            let u = seg as f32 / segments as f32;
            let theta = 2.0 * std::f32::consts::PI * u; // 方位角
            let cos_theta = theta.cos();
            let sin_theta = theta.sin();

            let x = sin_phi * cos_theta;
            let y = cos_phi;
            let z = sin_phi * sin_theta;

            mesh.vertices.push(Point3::new(x, y, z));
            mesh.normals.push(Vector3::new(x, y, z));
            mesh.uvs.push([u, v]);
        }
    }

    // 底部极点
    let bottom_idx = mesh.vertices.len() as u32;
    mesh.vertices.push(Point3::new(0.0, -1.0, 0.0));
    mesh.normals.push(Vector3::new(0.0, -1.0, 0.0));
    mesh.uvs.push([0.5, 0.0]);

    // --- 构建索引 ---

    // 顶部环面：连接顶部极点与第一环
    let first_ring_start = 1u32; // 索引 0 是北极
    for seg in 0..segments {
        let current = first_ring_start + seg;
        let next = first_ring_start + (seg + 1) % segments;
        // 北极 -> 当前 -> 下一个（逆时针从外看）
        mesh.indices.push(0);
        mesh.indices.push(current);
        mesh.indices.push(next);
    }

    // 中间环面：连接相邻的环
    for ring in 0..(rings - 1) {
        let ring_start = 1 + ring * segments;
        let next_ring_start = 1 + (ring + 1) * segments;

        for seg in 0..segments {
            let current = ring_start + seg;
            let current_next = ring_start + (seg + 1) % segments;
            let next = next_ring_start + seg;
            let next_next = next_ring_start + (seg + 1) % segments;

            // 两个三角形组成一个四边形
            // 三角形 1：current -> next -> current_next
            mesh.indices.push(current);
            mesh.indices.push(next);
            mesh.indices.push(current_next);
            // 三角形 2：current_next -> next -> next_next
            mesh.indices.push(current_next);
            mesh.indices.push(next);
            mesh.indices.push(next_next);
        }
    }

    // 底部环面：连接最后一环与底部极点
    let last_ring_start = 1 + (rings - 1) * segments;
    for seg in 0..segments {
        let current = last_ring_start + seg;
        let next = last_ring_start + (seg + 1) % segments;
        // 当前 -> 南极 -> 下一个（逆时针从外看）
        mesh.indices.push(current);
        mesh.indices.push(bottom_idx);
        mesh.indices.push(next);
    }

    mesh
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_icosahedron_base() {
        let mesh = icosphere(0);
        assert_eq!(mesh.vertices.len(), 12);
        assert_eq!(mesh.indices.len() / 3, 20); // 20 个面
        assert_eq!(mesh.normals.len(), 12);
        assert_eq!(mesh.uvs.len(), 12);
    }

    #[test]
    fn test_icosphere_subdivision_1() {
        let mesh = icosphere(1);
        // 细分一次：面数 = 20 * 4 = 80
        assert_eq!(mesh.indices.len() / 3, 80);
        // 顶点数大约 12 + 30 = 42（原始 12 + 30 条边各加一个中点）
        assert_eq!(mesh.vertices.len(), 42);
    }

    #[test]
    fn test_icosphere_subdivision_2() {
        let mesh = icosphere(2);
        // 细分两次：面数 = 20 * 4^2 = 320
        assert_eq!(mesh.indices.len() / 3, 320);
    }

    #[test]
    fn test_icosphere_unit_radius() {
        let mesh = icosphere(3);
        for v in &mesh.vertices {
            let len = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
            assert!((len - 1.0).abs() < 1e-5, "顶点不在单位球面上: {}", len);
        }
    }

    #[test]
    fn test_icosphere_normals() {
        let mesh = icosphere(2);
        for (v, n) in mesh.vertices.iter().zip(mesh.normals.iter()) {
            let v_vec = Vector3::new(v.x, v.y, v.z);
            let dot = v_vec.dot(n);
            assert!((dot - 1.0).abs() < 1e-5, "法线与顶点位置不一致: {}", dot);
        }
    }

    #[test]
    fn test_icosphere_uv_range() {
        let mesh = icosphere(2);
        for uv in &mesh.uvs {
            assert!(uv[0] >= 0.0 && uv[0] <= 1.0, "u 超出范围: {}", uv[0]);
            assert!(uv[1] >= 0.0 && uv[1] <= 1.0, "v 超出范围: {}", uv[1]);
        }
    }

    #[test]
    fn test_sphere_uv_basic() {
        let mesh = sphere_uv(16, 8);
        let expected_vertices = 2 + 16 * 8; // 两极 + 中间环
        assert_eq!(mesh.vertices.len(), expected_vertices as usize);
        assert_eq!(mesh.normals.len(), expected_vertices as usize);
        assert_eq!(mesh.uvs.len(), expected_vertices as usize);
        // 面数 = 顶部 16 + 中间 7*16*2 + 底部 16 = 256
        let expected_faces = 16 + 7 * 16 * 2 + 16;
        assert_eq!(mesh.indices.len() / 3, expected_faces as usize);
    }

    #[test]
    fn test_sphere_uv_unit_radius() {
        let mesh = sphere_uv(24, 12);
        for v in &mesh.vertices {
            let len = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
            assert!((len - 1.0).abs() < 1e-5, "顶点不在单位球面上: {}", len);
        }
    }

    #[test]
    fn test_sphere_uv_minimum() {
        let mesh = sphere_uv(3, 2);
        assert_eq!(mesh.vertices.len(), 8); // 2 + 3*2
        assert!(mesh.indices.len() > 0);
    }
}
