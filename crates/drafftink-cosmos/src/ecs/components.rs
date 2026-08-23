//! ECS 组件定义

use nalgebra::{Matrix4, Point3, UnitQuaternion, Vector3};
use serde::{Deserialize, Serialize};

/// 变换组件：位置、旋转、缩放
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transform {
    /// 位置
    pub position: Vector3<f32>,
    /// 旋转（单位四元数）
    pub rotation: UnitQuaternion<f32>,
    /// 均匀缩放
    pub scale: f32,
}

impl Transform {
    /// 计算模型矩阵（缩放 * 旋转 * 平移）
    pub fn matrix(&self) -> Matrix4<f32> {
        let translation = Matrix4::new_translation(&self.position);
        let rotation = self.rotation.to_homogeneous();
        let scale = Matrix4::new_scaling(self.scale);
        translation * rotation * scale
    }
}

impl Default for Transform {
    /// 默认变换：原点，无旋转，缩放 1.0
    fn default() -> Self {
        Self {
            position: Vector3::zeros(),
            rotation: UnitQuaternion::identity(),
            scale: 1.0,
        }
    }
}

/// 网格句柄组件：指向资源缓存中的网格
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeshHandle {
    /// 网格资源 ID
    pub mesh_id: u64,
}

/// 材质组件
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Material {
    /// 基础颜色（RGB）
    pub albedo: [f32; 3],
    /// 自发光颜色（用于太阳等自发光天体）
    pub emissive: [f32; 3],
    /// 粗糙度（0-1）
    pub roughness: f32,
    /// 金属度（0-1）
    pub metallic: f32,
    /// 纹理句柄（可选）
    pub texture_id: Option<u64>,
}

impl Default for Material {
    /// 默认材质：白色、不发光、粗糙度 0.5、金属度 0
    fn default() -> Self {
        Self {
            albedo: [1.0, 1.0, 1.0],
            emissive: [0.0, 0.0, 0.0],
            roughness: 0.5,
            metallic: 0.0,
            texture_id: None,
        }
    }
}

/// 轨道组件：描述天体的公转轨道参数
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Orbit {
    /// 半长轴（轨道大小）
    pub semi_major_axis: f32,
    /// 偏心率（0 = 圆形，<1 = 椭圆）
    pub eccentricity: f32,
    /// 倾角（弧度），相对于参考平面
    pub inclination: f32,
    /// 公转周期（秒/圈）
    pub orbital_period: f32,
    /// 当前角度（弧度），平近点角或真近点角
    pub current_angle: f32,
    /// 升交点经度（弧度）
    pub ascending_node: f32,
    /// 近心点幅角（弧度）
    pub arg_of_perihelion: f32,
}

/// 自转组件：描述天体的自转
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rotation {
    /// 角速度（弧度/秒）
    pub angular_velocity: f32,
    /// 自转轴
    pub axis: Vector3<f32>,
}

impl Default for Rotation {
    /// 默认自转：y 轴，零角速度
    fn default() -> Self {
        Self {
            angular_velocity: 0.0,
            axis: Vector3::y(),
        }
    }
}

/// 标注组件：用于在 3D 空间中显示文本标签
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Label {
    /// 标注文本
    pub text: String,
    /// 相对于实体位置的偏移
    pub offset: Vector3<f32>,
    /// 文本颜色（RGB）
    pub color: [f32; 3],
    /// 是否可见
    pub visible: bool,
}

/// 行星信息组件：用于 UI 显示行星详细信息
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlanetInfo {
    /// 行星名称
    pub name: String,
    /// 行星描述
    pub description: String,
    /// 直径（公里）
    pub diameter_km: f32,
    /// 质量（千克）
    pub mass_kg: f32,
}

/// 获取实体的世界坐标点（辅助函数）
pub fn transform_to_point(transform: &Transform) -> Point3<f32> {
    Point3::from(transform.position)
}
