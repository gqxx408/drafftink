//! 太阳系场景：太阳、八大行星及其轨道的预设数据
//!
//! 轨道距离和大小做艺术化处理，不按真实比例，
//! 以便在可视范围内清晰展示所有行星。

use nalgebra::{UnitQuaternion, Vector3};

use crate::ecs::{Label, Material, Orbit, PlanetInfo, Rotation, Transform};
use crate::geometry::icosphere;
use crate::resources::ResourceCache;

/// 太阳系实体数据
///
/// 采用并行数组架构，相同索引对应同一实体。
/// 实体顺序：太阳、水星、金星、地球、火星、木星、土星、土星环、天王星、海王星。
pub struct SolarSystemScene {
    pub transforms: Vec<Transform>,
    pub meshes: Vec<Option<u64>>,     // 网格资源 ID，None 表示无网格
    pub materials: Vec<Option<u64>>,  // 材质资源 ID
    pub orbits: Vec<Option<Orbit>>,   // 轨道参数
    pub rotations: Vec<Option<Rotation>>,
    pub labels: Vec<Option<Label>>,
    pub planet_infos: Vec<Option<PlanetInfo>>,
    pub names: Vec<String>,
}

impl SolarSystemScene {
    /// 创建太阳系场景预设
    ///
    /// 会在 cache 中创建 icosphere 网格（细分 3 级）和所有天体的材质，
    /// 并填充太阳 + 八大行星（含土星环）的数据。
    pub fn new(cache: &mut ResourceCache) -> Self {
        // --- 创建共享网格 ---
        let sphere_mesh_id = cache.add_mesh("icosphere_3", icosphere(3));

        // --- 创建材质 ---
        let sun_mat_id = cache.add_material(
            "sun",
            Material {
                albedo: [1.0, 0.95, 0.6],
                emissive: [1.0, 0.85, 0.3],
                roughness: 0.3,
                metallic: 0.0,
                texture_id: None,
            },
        );

        let mercury_mat_id = cache.add_material(
            "mercury",
            Material {
                albedo: [0.65, 0.62, 0.58],
                emissive: [0.0, 0.0, 0.0],
                roughness: 0.9,
                metallic: 0.0,
                texture_id: None,
            },
        );

        let venus_mat_id = cache.add_material(
            "venus",
            Material {
                albedo: [0.95, 0.86, 0.62],
                emissive: [0.0, 0.0, 0.0],
                roughness: 0.8,
                metallic: 0.0,
                texture_id: None,
            },
        );

        let earth_mat_id = cache.add_material(
            "earth",
            Material {
                albedo: [0.25, 0.55, 0.95],
                emissive: [0.0, 0.0, 0.0],
                roughness: 0.6,
                metallic: 0.1,
                texture_id: None,
            },
        );

        let mars_mat_id = cache.add_material(
            "mars",
            Material {
                albedo: [0.92, 0.38, 0.22],
                emissive: [0.0, 0.0, 0.0],
                roughness: 0.85,
                metallic: 0.0,
                texture_id: None,
            },
        );

        let jupiter_mat_id = cache.add_material(
            "jupiter",
            Material {
                albedo: [0.88, 0.62, 0.38],
                emissive: [0.0, 0.0, 0.0],
                roughness: 0.7,
                metallic: 0.0,
                texture_id: None,
            },
        );

        let saturn_mat_id = cache.add_material(
            "saturn",
            Material {
                albedo: [0.92, 0.84, 0.58],
                emissive: [0.0, 0.0, 0.0],
                roughness: 0.75,
                metallic: 0.0,
                texture_id: None,
            },
        );

        let saturn_ring_mat_id = cache.add_material(
            "saturn_ring",
            Material {
                albedo: [0.85, 0.76, 0.48],
                emissive: [0.0, 0.0, 0.0],
                roughness: 0.6,
                metallic: 0.2,
                texture_id: None,
            },
        );

        let uranus_mat_id = cache.add_material(
            "uranus",
            Material {
                albedo: [0.55, 0.88, 0.88],
                emissive: [0.0, 0.0, 0.0],
                roughness: 0.5,
                metallic: 0.1,
                texture_id: None,
            },
        );

        let neptune_mat_id = cache.add_material(
            "neptune",
            Material {
                albedo: [0.22, 0.35, 0.85],
                emissive: [0.0, 0.0, 0.0],
                roughness: 0.5,
                metallic: 0.1,
                texture_id: None,
            },
        );

        // --- 构建实体数据 ---
        let mut transforms: Vec<Transform> = Vec::new();
        let mut meshes: Vec<Option<u64>> = Vec::new();
        let mut materials: Vec<Option<u64>> = Vec::new();
        let mut orbits: Vec<Option<Orbit>> = Vec::new();
        let mut rotations: Vec<Option<Rotation>> = Vec::new();
        let mut labels: Vec<Option<Label>> = Vec::new();
        let mut planet_infos: Vec<Option<PlanetInfo>> = Vec::new();
        let mut names: Vec<String> = Vec::new();

        // ===== 太阳 =====
        transforms.push(Transform {
            position: Vector3::zeros(),
            rotation: UnitQuaternion::identity(),
            scale: 2.2,
        });
        meshes.push(Some(sphere_mesh_id));
        materials.push(Some(sun_mat_id));
        orbits.push(None);
        rotations.push(Some(Rotation {
            angular_velocity: 0.08,
            axis: Vector3::y(),
        }));
        labels.push(Some(Label {
            text: "太阳".to_string(),
            offset: Vector3::new(0.0, 2.5, 0.0),
            color: [1.0, 0.9, 0.4],
            visible: true,
        }));
        planet_infos.push(Some(PlanetInfo {
            name: "太阳".to_string(),
            description: "太阳系的中心恒星，一颗 G 型主序星，占太阳系总质量的 99.86%。".to_string(),
            diameter_km: 1_392_700.0,
            mass_kg: 1.989e30,
        }));
        names.push("sun".to_string());

        // ===== 水星 =====
        let mercury_orbit = Orbit {
            semi_major_axis: 3.5,
            eccentricity: 0.205,
            inclination: 0.12,
            orbital_period: 8.0,
            current_angle: 0.5,
            ascending_node: 0.0,
            arg_of_perihelion: 0.8,
        };
        transforms.push(orbit_position(&mercury_orbit, 0.38));
        meshes.push(Some(sphere_mesh_id));
        materials.push(Some(mercury_mat_id));
        orbits.push(Some(mercury_orbit));
        rotations.push(Some(Rotation {
            angular_velocity: 0.05,
            axis: Vector3::y(),
        }));
        labels.push(Some(Label {
            text: "水星".to_string(),
            offset: Vector3::new(0.0, 0.6, 0.0),
            color: [0.8, 0.8, 0.8],
            visible: true,
        }));
        planet_infos.push(Some(PlanetInfo {
            name: "水星".to_string(),
            description: "太阳系中最小且离太阳最近的行星，表面布满陨石坑，无大气层。".to_string(),
            diameter_km: 4_879.0,
            mass_kg: 3.301e23,
        }));
        names.push("mercury".to_string());

        // ===== 金星 =====
        let venus_orbit = Orbit {
            semi_major_axis: 5.2,
            eccentricity: 0.007,
            inclination: 0.06,
            orbital_period: 12.5,
            current_angle: 1.8,
            ascending_node: 0.0,
            arg_of_perihelion: 1.3,
        };
        transforms.push(orbit_position(&venus_orbit, 0.95));
        meshes.push(Some(sphere_mesh_id));
        materials.push(Some(venus_mat_id));
        orbits.push(Some(venus_orbit));
        rotations.push(Some(Rotation {
            angular_velocity: -0.03, // 逆向自转
            axis: Vector3::y(),
        }));
        labels.push(Some(Label {
            text: "金星".to_string(),
            offset: Vector3::new(0.0, 0.7, 0.0),
            color: [1.0, 0.9, 0.6],
            visible: true,
        }));
        planet_infos.push(Some(PlanetInfo {
            name: "金星".to_string(),
            description: "太阳系中最热的行星，被浓厚的二氧化碳大气层覆盖，逆向自转。".to_string(),
            diameter_km: 12_104.0,
            mass_kg: 4.867e24,
        }));
        names.push("venus".to_string());

        // ===== 地球 =====
        let earth_orbit = Orbit {
            semi_major_axis: 7.0,
            eccentricity: 0.017,
            inclination: 0.0,
            orbital_period: 16.0,
            current_angle: 2.5,
            ascending_node: 0.0,
            arg_of_perihelion: 1.99,
        };
        transforms.push(orbit_position(&earth_orbit, 1.0));
        meshes.push(Some(sphere_mesh_id));
        materials.push(Some(earth_mat_id));
        orbits.push(Some(earth_orbit));
        rotations.push(Some(Rotation {
            angular_velocity: 0.3,
            axis: Vector3::y(),
        }));
        labels.push(Some(Label {
            text: "地球".to_string(),
            offset: Vector3::new(0.0, 0.75, 0.0),
            color: [0.5, 0.8, 1.0],
            visible: true,
        }));
        planet_infos.push(Some(PlanetInfo {
            name: "地球".to_string(),
            description: "我们的家园，太阳系中唯一已知存在生命的行星，表面 71% 被水覆盖。".to_string(),
            diameter_km: 12_742.0,
            mass_kg: 5.972e24,
        }));
        names.push("earth".to_string());

        // ===== 火星 =====
        let mars_orbit = Orbit {
            semi_major_axis: 8.8,
            eccentricity: 0.093,
            inclination: 0.03,
            orbital_period: 22.0,
            current_angle: 3.8,
            ascending_node: 0.0,
            arg_of_perihelion: 2.95,
        };
        transforms.push(orbit_position(&mars_orbit, 0.53));
        meshes.push(Some(sphere_mesh_id));
        materials.push(Some(mars_mat_id));
        orbits.push(Some(mars_orbit));
        rotations.push(Some(Rotation {
            angular_velocity: 0.28,
            axis: Vector3::y(),
        }));
        labels.push(Some(Label {
            text: "火星".to_string(),
            offset: Vector3::new(0.0, 0.65, 0.0),
            color: [1.0, 0.6, 0.4],
            visible: true,
        }));
        planet_infos.push(Some(PlanetInfo {
            name: "火星".to_string(),
            description: "红色星球，表面富含氧化铁。拥有太阳系最高的火山——奥林帕斯山。".to_string(),
            diameter_km: 6_779.0,
            mass_kg: 6.417e23,
        }));
        names.push("mars".to_string());

        // ===== 木星 =====
        let jupiter_orbit = Orbit {
            semi_major_axis: 11.5,
            eccentricity: 0.049,
            inclination: 0.02,
            orbital_period: 35.0,
            current_angle: 0.3,
            ascending_node: 0.0,
            arg_of_perihelion: 0.55,
        };
        transforms.push(orbit_position(&jupiter_orbit, 1.0));
        meshes.push(Some(sphere_mesh_id));
        materials.push(Some(jupiter_mat_id));
        orbits.push(Some(jupiter_orbit));
        rotations.push(Some(Rotation {
            angular_velocity: 0.8,
            axis: Vector3::y(),
        }));
        labels.push(Some(Label {
            text: "木星".to_string(),
            offset: Vector3::new(0.0, 1.3, 0.0),
            color: [0.95, 0.75, 0.5],
            visible: true,
        }));
        planet_infos.push(Some(PlanetInfo {
            name: "木星".to_string(),
            description: "太阳系最大的行星，气态巨行星，著名的大红斑是一场持续数百年的风暴。".to_string(),
            diameter_km: 139_820.0,
            mass_kg: 1.898e27,
        }));
        names.push("jupiter".to_string());

        // ===== 土星 =====
        let saturn_orbit = Orbit {
            semi_major_axis: 14.5,
            eccentricity: 0.057,
            inclination: 0.04,
            orbital_period: 50.0,
            current_angle: 2.1,
            ascending_node: 0.0,
            arg_of_perihelion: 1.6,
        };
        transforms.push(orbit_position(&saturn_orbit, 0.9));
        meshes.push(Some(sphere_mesh_id));
        materials.push(Some(saturn_mat_id));
        orbits.push(Some(saturn_orbit.clone()));
        rotations.push(Some(Rotation {
            angular_velocity: 0.7,
            axis: Vector3::y(),
        }));
        labels.push(Some(Label {
            text: "土星".to_string(),
            offset: Vector3::new(0.0, 1.1, 0.0),
            color: [0.95, 0.9, 0.6],
            visible: true,
        }));
        planet_infos.push(Some(PlanetInfo {
            name: "土星".to_string(),
            description: "以其壮丽的环系统闻名，密度低于水，是太阳系第二大行星。".to_string(),
            diameter_km: 116_460.0,
            mass_kg: 5.683e26,
        }));
        names.push("saturn".to_string());

        // ===== 土星环 =====
        // 作为独立实体，与土星共享轨道位置
        // 使用更大的缩放来模拟环（在渲染层配合非均匀缩放或专用网格）
        let saturn_pos = transforms.last().unwrap().position;
        transforms.push(Transform {
            position: saturn_pos,
            rotation: UnitQuaternion::from_euler_angles(0.45, 0.0, 0.0), // 略微倾斜
            scale: 1.6,
        });
        meshes.push(Some(sphere_mesh_id));
        materials.push(Some(saturn_ring_mat_id));
        orbits.push(Some(saturn_orbit));
        rotations.push(Some(Rotation {
            angular_velocity: 0.5,
            axis: Vector3::y(),
        }));
        labels.push(None);
        planet_infos.push(None);
        names.push("saturn_ring".to_string());

        // ===== 天王星 =====
        let uranus_orbit = Orbit {
            semi_major_axis: 17.5,
            eccentricity: 0.046,
            inclination: 0.013,
            orbital_period: 72.0,
            current_angle: 4.2,
            ascending_node: 0.0,
            arg_of_perihelion: 2.2,
        };
        transforms.push(orbit_position(&uranus_orbit, 0.6));
        meshes.push(Some(sphere_mesh_id));
        materials.push(Some(uranus_mat_id));
        orbits.push(Some(uranus_orbit));
        rotations.push(Some(Rotation {
            angular_velocity: 0.5,
            axis: Vector3::x(), // 侧躺自转：自转轴接近 x 轴（几乎躺在轨道面上）
        }));
        labels.push(Some(Label {
            text: "天王星".to_string(),
            offset: Vector3::new(0.0, 0.9, 0.0),
            color: [0.6, 0.9, 0.95],
            visible: true,
        }));
        planet_infos.push(Some(PlanetInfo {
            name: "天王星".to_string(),
            description: "侧躺着自转的冰巨星，自转轴几乎与公转轨道平行，呈淡青色。".to_string(),
            diameter_km: 50_724.0,
            mass_kg: 8.681e25,
        }));
        names.push("uranus".to_string());

        // ===== 海王星 =====
        let neptune_orbit = Orbit {
            semi_major_axis: 20.5,
            eccentricity: 0.011,
            inclination: 0.02,
            orbital_period: 95.0,
            current_angle: 5.8,
            ascending_node: 0.0,
            arg_of_perihelion: 3.4,
        };
        transforms.push(orbit_position(&neptune_orbit, 0.45));
        meshes.push(Some(sphere_mesh_id));
        materials.push(Some(neptune_mat_id));
        orbits.push(Some(neptune_orbit));
        rotations.push(Some(Rotation {
            angular_velocity: 0.45,
            axis: Vector3::y(),
        }));
        labels.push(Some(Label {
            text: "海王星".to_string(),
            offset: Vector3::new(0.0, 0.85, 0.0),
            color: [0.4, 0.55, 1.0],
            visible: true,
        }));
        planet_infos.push(Some(PlanetInfo {
            name: "海王星".to_string(),
            description: "太阳系最远的行星，深蓝色冰巨星，拥有太阳系最强的风暴。".to_string(),
            diameter_km: 49_244.0,
            mass_kg: 1.024e26,
        }));
        names.push("neptune".to_string());

        Self {
            transforms,
            meshes,
            materials,
            orbits,
            rotations,
            labels,
            planet_infos,
            names,
        }
    }

    /// 返回实体总数
    pub fn entity_count(&self) -> usize {
        self.names.len()
    }

    /// 根据名称查找行星（或太阳）的实体索引
    ///
    /// 名称不区分大小写，支持中英文名称：
    /// - 英文：sun, mercury, venus, earth, mars, jupiter, saturn, uranus, neptune
    /// - 中文：太阳、水星、金星、地球、火星、木星、土星、天王星、海王星
    pub fn get_planet_index_by_name(&self, name: &str) -> Option<usize> {
        let name_lower = name.to_lowercase();
        self.names.iter().position(|n| n.to_lowercase() == name_lower)
    }
}

/// 根据轨道参数和初始角度计算行星初始位置
///
/// 使用简化的开普勒轨道近似，考虑偏心率和倾角。
fn orbit_position(orbit: &Orbit, scale: f32) -> Transform {
    let angle = orbit.current_angle;
    let a = orbit.semi_major_axis;
    let e = orbit.eccentricity;

    // 简化：用椭圆参数方程计算位置（近似处理）
    // r = a * (1 - e^2) / (1 + e * cos(theta))
    let r = a * (1.0 - e * e) / (1.0 + e * angle.cos());

    // 在轨道平面内的位置
    let x = r * angle.cos();
    let z = r * angle.sin();

    // 应用倾角（绕 x 轴旋转）
    let y = z * orbit.inclination.sin();
    let z = z * orbit.inclination.cos();

    Transform {
        position: Vector3::new(x, y, z),
        rotation: UnitQuaternion::identity(),
        scale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_solar_system_entity_count() {
        let mut cache = ResourceCache::new();
        let scene = SolarSystemScene::new(&mut cache);

        // 太阳 + 8 大行星 + 土星环 = 10
        assert_eq!(scene.entity_count(), 10);
    }

    #[test]
    fn test_all_arrays_same_length() {
        let mut cache = ResourceCache::new();
        let scene = SolarSystemScene::new(&mut cache);

        let n = scene.entity_count();
        assert_eq!(scene.transforms.len(), n);
        assert_eq!(scene.meshes.len(), n);
        assert_eq!(scene.materials.len(), n);
        assert_eq!(scene.orbits.len(), n);
        assert_eq!(scene.rotations.len(), n);
        assert_eq!(scene.labels.len(), n);
        assert_eq!(scene.planet_infos.len(), n);
    }

    #[test]
    fn test_sun_has_no_orbit() {
        let mut cache = ResourceCache::new();
        let scene = SolarSystemScene::new(&mut cache);

        let sun_idx = scene.get_planet_index_by_name("sun").unwrap();
        assert!(scene.orbits[sun_idx].is_none());
        // 太阳有自发光材质
        let mat_id = scene.materials[sun_idx].unwrap();
        let mat = cache.get_material(mat_id).unwrap();
        assert!(mat.emissive[0] > 0.0 || mat.emissive[1] > 0.0);
    }

    #[test]
    fn test_get_planet_index_by_name() {
        let mut cache = ResourceCache::new();
        let scene = SolarSystemScene::new(&mut cache);

        assert_eq!(scene.get_planet_index_by_name("sun"), Some(0));
        assert_eq!(scene.get_planet_index_by_name("SUN"), Some(0));
        assert_eq!(scene.get_planet_index_by_name("earth"), Some(3));
        assert_eq!(scene.get_planet_index_by_name("neptune"), Some(9));
        assert_eq!(scene.get_planet_index_by_name("pluto"), None);
    }

    #[test]
    fn test_planets_have_orbits() {
        let mut cache = ResourceCache::new();
        let scene = SolarSystemScene::new(&mut cache);

        // 太阳没有轨道
        assert!(scene.orbits[0].is_none());
        // 其余行星（含土星环）都有轨道
        for i in 1..scene.entity_count() {
            assert!(
                scene.orbits[i].is_some(),
                "实体 {} ({}) 应该有轨道",
                i,
                scene.names[i]
            );
        }
    }

    #[test]
    fn test_venus_retrograde_rotation() {
        let mut cache = ResourceCache::new();
        let scene = SolarSystemScene::new(&mut cache);

        let idx = scene.get_planet_index_by_name("venus").unwrap();
        let rot = scene.rotations[idx].as_ref().unwrap();
        // 金星逆向自转：角速度为负
        assert!(rot.angular_velocity < 0.0);
    }

    #[test]
    fn test_uranus_sideways_rotation() {
        let mut cache = ResourceCache::new();
        let scene = SolarSystemScene::new(&mut cache);

        let idx = scene.get_planet_index_by_name("uranus").unwrap();
        let rot = scene.rotations[idx].as_ref().unwrap();
        // 天王星侧躺自转：自转轴 x 分量较大
        assert!(rot.axis.x.abs() > 0.5);
    }

    #[test]
    fn test_cache_is_populated() {
        let mut cache = ResourceCache::new();
        let _scene = SolarSystemScene::new(&mut cache);

        // 一个网格（icosphere_3）+ 10 个材质
        assert_eq!(cache.mesh_count(), 1);
        assert_eq!(cache.material_count(), 10);
    }

    #[test]
    fn test_saturn_has_ring() {
        let mut cache = ResourceCache::new();
        let scene = SolarSystemScene::new(&mut cache);

        let saturn_idx = scene.get_planet_index_by_name("saturn").unwrap();
        let ring_idx = scene.get_planet_index_by_name("saturn_ring").unwrap();

        // 土星环在土星之后
        assert_eq!(ring_idx, saturn_idx + 1);
        // 土星环没有行星信息组件
        assert!(scene.planet_infos[ring_idx].is_none());
        // 土星环有网格和材质
        assert!(scene.meshes[ring_idx].is_some());
        assert!(scene.materials[ring_idx].is_some());
    }
}
