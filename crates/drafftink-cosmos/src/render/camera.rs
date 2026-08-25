//! 相机系统：视角、投影矩阵等

use nalgebra::{Matrix4, Point2, Point3, Vector2, Vector3};

/// 轨道相机：围绕目标点旋转的透视相机
#[derive(Debug, Clone)]
pub struct OrbitCamera {
    /// 观察目标点
    pub target: Point3<f32>,
    /// 距离目标的距离
    pub distance: f32,
    /// 偏航角（弧度）
    pub yaw: f32,
    /// 俯仰角（弧度）
    pub pitch: f32,
    /// 垂直视场角（弧度）
    pub fov: f32,
    /// 近裁剪面
    pub near: f32,
    /// 远裁剪面
    pub far: f32,
    /// 宽高比
    pub aspect: f32,
}

impl OrbitCamera {
    /// 创建默认轨道相机
    ///
    /// 默认参数：target 原点，distance 10，yaw 0，pitch 30°，
    /// fov 60°，near 0.1，far 1000，aspect 1
    pub fn new() -> Self {
        Self {
            target: Point3::origin(),
            distance: 10.0,
            yaw: 0.0,
            pitch: std::f32::consts::PI / 6.0, // 30°
            fov: std::f32::consts::PI / 3.0,   // 60°
            near: 0.1,
            far: 1000.0,
            aspect: 1.0,
        }
    }

    /// 计算相机位置（球坐标转笛卡尔坐标）
    ///
    /// 使用 yaw 和 pitch 在以 target 为中心的球面上计算相机位置。
    /// yaw = 0, pitch = 0 时相机位于 +z 方向。
    pub fn eye_position(&self) -> Point3<f32> {
        let cos_pitch = self.pitch.cos();
        let sin_pitch = self.pitch.sin();
        let cos_yaw = self.yaw.cos();
        let sin_yaw = self.yaw.sin();

        let x = self.distance * cos_pitch * sin_yaw;
        let y = self.distance * sin_pitch;
        let z = self.distance * cos_pitch * cos_yaw;

        self.target + Vector3::new(x, y, z)
    }

    /// 计算视图矩阵（右手坐标系 look_at）
    pub fn view_matrix(&self) -> Matrix4<f32> {
        let eye = self.eye_position();
        let up = Vector3::y();
        Matrix4::look_at_rh(&eye, &self.target, &up)
    }

    /// 计算透视投影矩阵
    ///
    /// 使用 nalgebra 的 Perspective3，遵循 OpenGL 右手坐标系约定：
    /// - NDC z 范围为 [-1, 1]
    /// - 近裁剪面 z = -near，远裁剪面 z = -far（视图空间）
    pub fn projection_matrix(&self) -> Matrix4<f32> {
        let perspective = nalgebra::Perspective3::new(self.aspect, self.fov, self.near, self.far);
        perspective.as_matrix().clone()
    }

    /// 计算视图投影矩阵（VP = P * V）
    pub fn view_projection(&self) -> Matrix4<f32> {
        self.projection_matrix() * self.view_matrix()
    }

    /// 轨道旋转：调整 yaw 和 pitch
    ///
    /// pitch 被限制在 ±89° 以内，避免万向节锁。
    pub fn orbit(&mut self, delta_yaw: f32, delta_pitch: f32) {
        self.yaw += delta_yaw;
        self.pitch += delta_pitch;
        let limit = 89.0_f32.to_radians();
        self.pitch = self.pitch.clamp(-limit, limit);
    }

    /// 缩放：调整相机到目标的距离
    ///
    /// distance 被限制在 [0.1, 10000.0] 范围内。
    pub fn zoom(&mut self, factor: f32) {
        self.distance *= factor;
        self.distance = self.distance.clamp(0.1, 10000.0);
    }

    /// 平移目标点（在相机的右/上方向上移动）
    ///
    /// `delta.x` 为水平平移（右为正），`delta.y` 为垂直平移（上为正）。
    /// 平移量会根据当前距离和视场角自动缩放。
    pub fn pan(&mut self, delta: Vector2<f32>) {
        let eye = self.eye_position();
        let forward = (self.target - eye).normalize();
        let right = forward.cross(&Vector3::y()).normalize();
        let up = right.cross(&forward).normalize();

        // 平移量根据距离和 fov 缩放：在屏幕上移动相同比例的距离
        let scale = self.distance * (self.fov * 0.5).tan() * 2.0;

        let pan_x = right * delta.x * scale * self.aspect;
        let pan_y = up * delta.y * scale;

        self.target -= pan_x + pan_y;
    }
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// 3D -> 2D 投影辅助函数
// ---------------------------------------------------------------------------

/// 将 3D 点投影到 2D 屏幕坐标
///
/// - `vp`：视图投影矩阵（行向量 * 矩阵的乘法顺序）
/// - `point`：世界空间中的点
/// - `screen_width` / `screen_height`：屏幕尺寸（像素）
///
/// 返回屏幕坐标（左上角为原点，y 轴向下）。
/// 如果点在视锥体之外（w <= 0 或 NDC 超出 [-1,1] 范围）返回 None。
pub fn project_point(
    vp: &Matrix4<f32>,
    point: &Point3<f32>,
    screen_width: f32,
    screen_height: f32,
) -> Option<Point2<f32>> {
    // 裁剪空间坐标：clip = vp * point
    let clip = vp * point.to_homogeneous();

    // w <= 0 表示点在相机后面或在近裁剪面之后
    let w = clip.w;
    if w <= 0.0 {
        return None;
    }

    // 透视除法：得到 NDC（归一化设备坐标），范围 [-1, 1]
    let ndc_x = clip.x / w;
    let ndc_y = clip.y / w;
    let ndc_z = clip.z / w;

    // 视锥体剔除：检查 NDC 是否在 [-1, 1] 范围内
    if ndc_x < -1.0 || ndc_x > 1.0 || ndc_y < -1.0 || ndc_y > 1.0 || ndc_z < -1.0 || ndc_z > 1.0 {
        return None;
    }

    // NDC -> 屏幕坐标
    // NDC x: [-1, 1] -> [0, screen_width]
    // NDC y: [-1, 1] -> [screen_height, 0]（翻转，因为屏幕 y 轴向下）
    let screen_x = (ndc_x + 1.0) * 0.5 * screen_width;
    let screen_y = (1.0 - ndc_y) * 0.5 * screen_height;

    Some(Point2::new(screen_x, screen_y))
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn test_default_camera() {
        let cam = OrbitCamera::new();
        assert!(approx_eq(cam.distance, 10.0, 1e-5));
        assert!(approx_eq(cam.yaw, 0.0, 1e-5));
        assert!(approx_eq(cam.pitch, std::f32::consts::PI / 6.0, 1e-5));
        assert!(approx_eq(cam.fov, std::f32::consts::PI / 3.0, 1e-5));
        assert!(approx_eq(cam.aspect, 1.0, 1e-5));
    }

    #[test]
    fn test_eye_position_zero_angles() {
        let cam = OrbitCamera {
            yaw: 0.0,
            pitch: 0.0,
            distance: 10.0,
            target: Point3::origin(),
            ..OrbitCamera::new()
        };
        let eye = cam.eye_position();
        // yaw=0, pitch=0 时相机在 +z 方向
        assert!(approx_eq(eye.x, 0.0, 1e-5));
        assert!(approx_eq(eye.y, 0.0, 1e-5));
        assert!(approx_eq(eye.z, 10.0, 1e-5));
    }

    #[test]
    fn test_orbit_pitch_clamp() {
        let mut cam = OrbitCamera::new();
        cam.orbit(0.0, 10.0); // 超大 pitch
        let limit = 89.0_f32.to_radians();
        assert!(cam.pitch <= limit);
        assert!(cam.pitch >= -limit);

        cam.orbit(0.0, -20.0); // 超大负 pitch
        assert!(cam.pitch <= limit);
        assert!(cam.pitch >= -limit);
    }

    #[test]
    fn test_zoom_clamp() {
        let mut cam = OrbitCamera::new();
        cam.zoom(0.001); // 缩得很小
        assert!(cam.distance >= 0.1);

        cam.zoom(100000.0); // 放得很大
        assert!(cam.distance <= 10000.0);
    }

    #[test]
    fn test_project_point_in_front() {
        let cam = OrbitCamera {
            yaw: 0.0,
            pitch: 0.0,
            distance: 10.0,
            target: Point3::origin(),
            aspect: 1.0,
            ..OrbitCamera::new()
        };
        let vp = cam.view_projection();

        // 原点在视锥体中心，应该投影到屏幕中心
        let p = project_point(&vp, &Point3::origin(), 800.0, 600.0);
        assert!(p.is_some());
        let p = p.unwrap();
        assert!(approx_eq(p.x, 400.0, 2.0));
        assert!(approx_eq(p.y, 300.0, 2.0));
    }

    #[test]
    fn test_project_point_behind_camera() {
        let cam = OrbitCamera {
            yaw: 0.0,
            pitch: 0.0,
            distance: 10.0,
            target: Point3::origin(),
            ..OrbitCamera::new()
        };
        let vp = cam.view_projection();

        // 相机在 z=10，看向原点；z=20 的点在相机后面
        let p = project_point(&vp, &Point3::new(0.0, 0.0, 20.0), 800.0, 600.0);
        assert!(p.is_none());
    }

    #[test]
    fn test_view_projection_matrix() {
        let cam = OrbitCamera::new();
        let vp = cam.view_projection();
        // VP 矩阵应该是 4x4 且不是零矩阵
        assert!(vp.m11 != 0.0 || vp.m12 != 0.0);
    }
}
