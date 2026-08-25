//! ECS 系统定义
//!
//! 所有系统均接收组件切片，相同索引对应同一实体。
//! 这是一种轻量级的并行数组 ECS 架构。

use std::f32::consts::{FRAC_PI_2, PI};

use nalgebra::{UnitQuaternion, Vector3};

use crate::ecs::components::{Orbit, Rotation, Transform};

/// 自转系统：每帧更新实体的自转
///
/// # 参数
/// - `transforms`: 变换组件数组（可变）
/// - `rotations`: 自转组件数组
/// - `dt`: 时间步长（秒）
///
/// # 注意
/// 两个数组按实体索引对齐，相同 index 对应同一实体。
/// 数组长度可以不同，系统只处理两者中较短的部分。
pub fn rotation_system(transforms: &mut [Transform], rotations: &[Rotation], dt: f32) {
    let count = transforms.len().min(rotations.len());
    for i in 0..count {
        let rot = &rotations[i];
        let transform = &mut transforms[i];

        // 绕自转轴旋转 angular_velocity * dt
        let angle = rot.angular_velocity * dt;
        if angle.abs() > 1e-8 {
            // 归一化自转轴，确保旋转轴是单位向量
            let axis = if rot.axis.norm_squared() > 1e-8 {
                rot.axis.normalize()
            } else {
                Vector3::y()
            };
            let delta_rot =
                UnitQuaternion::from_axis_angle(&nalgebra::Unit::new_normalize(axis), angle);
            transform.rotation = delta_rot * transform.rotation;
        }
    }
}

/// 轨道系统：根据开普勒轨道参数计算天体位置
///
/// 使用简化的平均角速度近似更新角度，
/// 再通过椭圆参数方程和 3D 旋转计算世界坐标。
///
/// # 参数
/// - `transforms`: 变换组件数组（可变，位置会被更新）
/// - `orbits`: 轨道组件数组（可变，current_angle 会被更新）
/// - `dt`: 时间步长（秒）
pub fn orbit_system(transforms: &mut [Transform], orbits: &mut [Orbit], dt: f32) {
    let count = transforms.len().min(orbits.len());
    for i in 0..count {
        let orbit = &mut orbits[i];
        let transform = &mut transforms[i];

        // 防止除零
        if orbit.orbital_period.abs() < 1e-8 {
            continue;
        }

        // 更新当前角度（平均角速度近似）
        // M = 2π / T * dt，M 为平近点角
        orbit.current_angle += 2.0 * PI / orbit.orbital_period * dt;

        // 将角度归一化到 [0, 2π)
        orbit.current_angle = normalize_angle(orbit.current_angle);

        // 计算轨道位置
        let pos = compute_orbit_position(orbit);
        transform.position = pos;
    }
}

/// 根据轨道参数计算世界坐标位置
///
/// 使用 3-1-3 欧拉角（Z-X-Z）将近心点坐标系下的位置
/// 变换到世界坐标系：
/// 1. 绕 z 轴旋转近心点幅角 ω (arg_of_perihelion)
/// 2. 绕 x 轴旋转轨道倾角 i (inclination)
/// 3. 绕 z 轴旋转升交点经度 Ω (ascending_node)
fn compute_orbit_position(orbit: &Orbit) -> Vector3<f32> {
    let a = orbit.semi_major_axis;
    let e = orbit.eccentricity;
    let nu = orbit.current_angle; // 真近点角（简化为当前角度）

    // 椭圆轨道半径：r = a(1-e²) / (1 + e·cos(ν))
    let r = if e < 1e-6 {
        // 圆形轨道，直接用半长轴
        a
    } else {
        a * (1.0 - e * e) / (1.0 + e * nu.cos())
    };

    // 近心点坐标系中的位置（轨道平面内，x 轴指向近心点）
    let x_peri = r * nu.cos();
    let y_peri = r * nu.sin();

    // 3-1-3 欧拉角旋转到世界坐标系
    // R = Rz(Ω) * Rx(i) * Rz(ω)
    let omega = orbit.arg_of_perihelion; // ω
    let inc = orbit.inclination; // i
    let asc_node = orbit.ascending_node; // Ω

    // 先应用近心点幅角 ω（绕 z 轴旋转）
    let (sin_omega, cos_omega) = omega.sin_cos();
    let x1 = x_peri * cos_omega - y_peri * sin_omega;
    let y1 = x_peri * sin_omega + y_peri * cos_omega;
    let z1 = 0.0;

    // 再应用轨道倾角 i（绕 x 轴旋转）
    let (sin_inc, cos_inc) = inc.sin_cos();
    let x2 = x1;
    let y2 = y1 * cos_inc - z1 * sin_inc;
    let z2 = y1 * sin_inc + z1 * cos_inc;

    // 最后应用升交点经度 Ω（绕 z 轴旋转）
    let (sin_asc, cos_asc) = asc_node.sin_cos();
    let x3 = x2 * cos_asc - y2 * sin_asc;
    let y3 = x2 * sin_asc + y2 * cos_asc;
    let z3 = z2;

    Vector3::new(x3, y3, z3)
}

/// 将角度归一化到 [0, 2π)
fn normalize_angle(angle: f32) -> f32 {
    let two_pi = 2.0 * PI;
    let mut a = angle % two_pi;
    if a < 0.0 {
        a += two_pi;
    }
    a
}

/// 轨道相机系统：根据 yaw/pitch/distance 计算相机位置和朝向
///
/// # 参数
/// - `yaw`: 偏航角（弧度），绕 y 轴旋转
/// - `pitch`: 俯仰角（弧度），绕 x 轴旋转，限制在 -89° 到 89°
/// - `distance`: 相机到目标点的距离
/// - `target`: 相机观察的目标点位置
///
/// # 返回
/// 返回 `(eye_pos, target_pos, up_vector)` 三元组：
/// - `eye_pos`: 相机位置
/// - `target_pos`: 相机观察目标点
/// - `up_vector`: 相机上方向向量
pub fn camera_orbit_system(
    yaw: f32,
    pitch: f32,
    distance: f32,
    target: Vector3<f32>,
) -> (Vector3<f32>, Vector3<f32>, Vector3<f32>) {
    // 限制 pitch 在 -89° 到 89° 之间（避免翻转）
    let pitch_limit = 89.0 * PI / 180.0;
    let pitch = pitch.clamp(-pitch_limit, pitch_limit);

    // 确保距离为正
    let distance = distance.max(0.01);

    // 球坐标转笛卡尔坐标
    // 相机位置相对于目标点的偏移
    // yaw 绕 y 轴（水平旋转），pitch 绕 x 轴（垂直旋转）
    let (sin_yaw, cos_yaw) = yaw.sin_cos();
    let (sin_pitch, cos_pitch) = pitch.sin_cos();

    let offset_x = distance * cos_pitch * sin_yaw;
    let offset_y = distance * sin_pitch;
    let offset_z = distance * cos_pitch * cos_yaw;

    let eye_pos = target + Vector3::new(offset_x, offset_y, offset_z);

    // 计算上方向向量
    // 当 pitch 接近 ±90° 时，需要特殊处理避免万向节锁
    let up = if (pitch.abs() - FRAC_PI_2).abs() < 0.01 {
        // 接近垂直时，用 x 轴方向作为上方向
        if pitch > 0.0 {
            Vector3::new(-sin_yaw, 0.0, -cos_yaw)
        } else {
            Vector3::new(sin_yaw, 0.0, cos_yaw)
        }
    } else {
        Vector3::y()
    };

    (eye_pos, target, up)
}

/// 线性插值两个 Vector3
///
/// # 参数
/// - `a`: 起始向量
/// - `b`: 目标向量
/// - `t`: 插值参数 [0, 1]
pub fn lerp_vec3(a: Vector3<f32>, b: Vector3<f32>, t: f32) -> Vector3<f32> {
    a + (b - a) * t
}

/// 三次缓出函数（ease out cubic）
///
/// 开始快，结束慢。适用于减速运动。
/// f(t) = 1 - (1-t)³
pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// 二次缓入缓出函数（ease in out quad）
///
/// 开始慢，中间快，结束慢。适用于平滑的启动和停止。
/// f(t) = 2t²  当 t < 0.5
/// f(t) = 1 - (-2t+2)² / 2  当 t >= 0.5
pub fn ease_in_out_quad(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        2.0 * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rotation_system_basic() {
        let mut transforms = vec![Transform::default()];
        let rotations = vec![Rotation {
            angular_velocity: PI,
            axis: Vector3::y(),
        }];

        rotation_system(&mut transforms, &rotations, 1.0);

        // 旋转 PI 弧度（180°），四元数的角度应该是 PI
        let (axis, angle) = transforms[0].rotation.axis_angle().unwrap();
        assert!((angle - PI).abs() < 1e-5, "angle = {}", angle);
        assert!((axis.y - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_orbit_system_circular() {
        let mut transforms = vec![Transform::default()];
        let mut orbits = vec![Orbit {
            semi_major_axis: 10.0,
            eccentricity: 0.0,
            inclination: 0.0,
            orbital_period: 4.0, // 4 秒一圈
            current_angle: 0.0,
            ascending_node: 0.0,
            arg_of_perihelion: 0.0,
        }];

        // 运行 1 秒，应该转 90°
        orbit_system(&mut transforms, &mut orbits, 1.0);

        assert!((orbits[0].current_angle - PI / 2.0).abs() < 1e-5);

        // 圆形轨道 + 90° 应该在 y 方向（z 轴是轨道平面法向？）
        // 这里 x = r*cos(90°) = 0, y = r*sin(90°) = r
        // 但经过零旋转后，应该是 x = 0, y = r, z = 0
        let pos = &transforms[0].position;
        assert!(pos.x.abs() < 1e-5, "x = {}", pos.x);
        assert!((pos.y - 10.0).abs() < 1e-5, "y = {}", pos.y);
        assert!(pos.z.abs() < 1e-5, "z = {}", pos.z);
    }

    #[test]
    fn test_camera_orbit_system_basic() {
        let target = Vector3::new(0.0, 0.0, 0.0);

        // yaw=0, pitch=0, distance=10 → 相机在 z 轴正方向？
        // 取决于球坐标约定。我们的公式：
        // x = distance * cos(pitch) * sin(yaw)
        // y = distance * sin(pitch)
        // z = distance * cos(pitch) * cos(yaw)
        // yaw=0 → sin=0, cos=1 → x=0, z=distance
        let (eye, tgt, up) = camera_orbit_system(0.0, 0.0, 10.0, target);

        assert!((eye.x - 0.0).abs() < 1e-5);
        assert!((eye.y - 0.0).abs() < 1e-5);
        assert!((eye.z - 10.0).abs() < 1e-5);
        assert_eq!(tgt, target);
        assert!((up - Vector3::y()).norm() < 1e-5);
    }

    #[test]
    fn test_pitch_clamping() {
        let target = Vector3::zeros();
        let too_high = 90.0 * PI / 180.0 + 0.5; // 超过 89°
        let (_, _, _) = camera_orbit_system(0.0, too_high, 10.0, target);
        // 不崩溃即可，clamp 在内部完成
        // 可以通过检查返回值验证，但函数签名不返回 clamped 值
    }

    #[test]
    fn test_lerp_vec3() {
        let a = Vector3::new(0.0, 0.0, 0.0);
        let b = Vector3::new(10.0, 20.0, 30.0);

        let result = lerp_vec3(a, b, 0.5);
        assert!((result.x - 5.0).abs() < 1e-5);
        assert!((result.y - 10.0).abs() < 1e-5);
        assert!((result.z - 15.0).abs() < 1e-5);
    }

    #[test]
    fn test_ease_out_cubic() {
        assert!((ease_out_cubic(0.0) - 0.0).abs() < 1e-5);
        assert!((ease_out_cubic(1.0) - 1.0).abs() < 1e-5);
        // 0.5 处应该大于 0.5（因为是 ease out，前快后慢）
        assert!(ease_out_cubic(0.5) > 0.5);
    }

    #[test]
    fn test_ease_in_out_quad() {
        assert!((ease_in_out_quad(0.0) - 0.0).abs() < 1e-5);
        assert!((ease_in_out_quad(1.0) - 1.0).abs() < 1e-5);
        assert!((ease_in_out_quad(0.5) - 0.5).abs() < 1e-5);
    }

    #[test]
    fn test_normalize_angle() {
        assert!((normalize_angle(3.0 * PI) - PI).abs() < 1e-5);
        assert!((normalize_angle(-PI) - PI).abs() < 1e-5);
        assert!((normalize_angle(0.0) - 0.0).abs() < 1e-5);
    }
}
