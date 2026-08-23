//! 等距矩形投影（equirectangular projection）
//!
//! 用于 2D 地图模式，将经纬度坐标与 2D 平面坐标、球面 3D 坐标互相转换。

use std::f32::consts::PI;
use nalgebra::Point3;

/// 经纬度转 2D 地图坐标。
///
/// - `lon`: 经度（弧度，-π 到 π）
/// - `lat`: 纬度（弧度，-π/2 到 π/2）
/// - `width`: 地图宽度
/// - `height`: 地图高度
///
/// 返回 `(x, y)`，x 范围 `[0, width]`，y 范围 `[0, height]`。
pub fn lon_lat_to_xy(lon: f32, lat: f32, width: f32, height: f32) -> (f32, f32) {
    let x = (lon / (2.0 * PI) + 0.5) * width;
    let y = (0.5 - lat / PI) * height;
    (x, y)
}

/// 2D 地图坐标转经纬度。
///
/// - `x`: 水平坐标（0 到 width）
/// - `y`: 垂直坐标（0 到 height）
/// - `width`: 地图宽度
/// - `height`: 地图高度
///
/// 返回 `(lon, lat)` 弧度，lon 范围 `[-π, π]`，lat 范围 `[-π/2, π/2]`。
pub fn xy_to_lon_lat(x: f32, y: f32, width: f32, height: f32) -> (f32, f32) {
    let lon = (x / width - 0.5) * 2.0 * PI;
    let lat = (0.5 - y / height) * PI;
    (lon, lat)
}

/// 球面上的 3D 点转经纬度。
///
/// 假设点位于单位球面上（若不是单位球，结果相当于按向量方向投影到单位球）。
///
/// - `point`: 球面上的 3D 点
///
/// 返回 `(lon, lat)` 弧度。
/// - `lon = atan2(x, z)`
/// - `lat = asin(y / radius)`，自动根据点到原点的距离计算半径
pub fn sphere_point_to_lon_lat(point: &Point3<f32>) -> (f32, f32) {
    let radius = point.coords.norm();
    if radius == 0.0 {
        return (0.0, 0.0);
    }
    let lon = point.x.atan2(point.z);
    let lat = (point.y / radius).clamp(-1.0, 1.0).asin();
    (lon, lat)
}

/// 经纬度转球面上的 3D 点。
///
/// - `lon`: 经度（弧度）
/// - `lat`: 纬度（弧度）
/// - `radius`: 球面半径
///
/// 返回球面上对应的 3D 点坐标。
pub fn lon_lat_to_sphere_point(lon: f32, lat: f32, radius: f32) -> Point3<f32> {
    let cos_lat = lat.cos();
    let sin_lat = lat.sin();
    let x = radius * cos_lat * lon.sin();
    let y = radius * sin_lat;
    let z = radius * cos_lat * lon.cos();
    Point3::new(x, y, z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn test_lon_lat_to_xy_center() {
        let (x, y) = lon_lat_to_xy(0.0, 0.0, 1000.0, 500.0);
        assert!(approx_eq(x, 500.0, 1e-6));
        assert!(approx_eq(y, 250.0, 1e-6));
    }

    #[test]
    fn test_lon_lat_to_xy_corners() {
        // 左上角：lon=-π, lat=π/2
        let (x, y) = lon_lat_to_xy(-PI, FRAC_PI_2, 1000.0, 500.0);
        assert!(approx_eq(x, 0.0, 1e-6));
        assert!(approx_eq(y, 0.0, 1e-6));

        // 右下角：lon=π, lat=-π/2
        let (x, y) = lon_lat_to_xy(PI, -FRAC_PI_2, 1000.0, 500.0);
        assert!(approx_eq(x, 1000.0, 1e-6));
        assert!(approx_eq(y, 500.0, 1e-6));
    }

    #[test]
    fn test_xy_to_lon_lat_roundtrip() {
        let width = 1000.0;
        let height = 500.0;
        let lon = PI / 4.0;
        let lat = PI / 6.0;

        let (x, y) = lon_lat_to_xy(lon, lat, width, height);
        let (lon2, lat2) = xy_to_lon_lat(x, y, width, height);

        assert!(approx_eq(lon, lon2, 1e-6));
        assert!(approx_eq(lat, lat2, 1e-6));
    }

    #[test]
    fn test_sphere_point_lon_lat_roundtrip() {
        let lon = PI / 3.0;
        let lat = -PI / 4.0;
        let radius = 10.0;

        let point = lon_lat_to_sphere_point(lon, lat, radius);
        let (lon2, lat2) = sphere_point_to_lon_lat(&point);

        assert!(approx_eq(lon, lon2, 1e-6));
        assert!(approx_eq(lat, lat2, 1e-6));
    }

    #[test]
    fn test_lon_lat_to_sphere_point_radius() {
        let point = lon_lat_to_sphere_point(0.0, 0.0, 5.0);
        assert!(approx_eq(point.coords.norm(), 5.0, 1e-6));
        // 经度 0、纬度 0 应该在 z 轴正方向
        assert!(approx_eq(point.z, 5.0, 1e-6));
        assert!(approx_eq(point.x, 0.0, 1e-6));
        assert!(approx_eq(point.y, 0.0, 1e-6));
    }
}
