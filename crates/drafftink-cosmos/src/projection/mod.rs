//! 投影模块：各种地图投影算法

pub mod equirectangular;

pub use equirectangular::{
    lon_lat_to_sphere_point,
    lon_lat_to_xy,
    sphere_point_to_lon_lat,
    xy_to_lon_lat,
};
