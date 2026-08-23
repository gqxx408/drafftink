//! 资源缓存：网格、材质等的缓存管理，避免重复加载

use std::collections::HashMap;

use crate::ecs::Material;
use crate::geometry::MeshData;

/// 资源缓存（网格 + 材质）
///
/// 使用 u64 作为资源 ID，内部维护名称到 ID 的映射，
/// 支持通过 ID 或名称查询资源。
#[derive(Debug)]
pub struct ResourceCache {
    meshes: HashMap<u64, MeshData>,
    materials: HashMap<u64, Material>,
    mesh_name_to_id: HashMap<String, u64>,
    material_name_to_id: HashMap<String, u64>,
    next_mesh_id: u64,
    next_material_id: u64,
}

impl ResourceCache {
    /// 创建空的资源缓存
    pub fn new() -> Self {
        Self {
            meshes: HashMap::new(),
            materials: HashMap::new(),
            mesh_name_to_id: HashMap::new(),
            material_name_to_id: HashMap::new(),
            next_mesh_id: 0,
            next_material_id: 0,
        }
    }

    /// 添加网格，返回分配的网格 ID
    ///
    /// 如果同名网格已存在，会覆盖原有数据并返回已有 ID。
    pub fn add_mesh(&mut self, name: &str, mesh: MeshData) -> u64 {
        if let Some(&id) = self.mesh_name_to_id.get(name) {
            self.meshes.insert(id, mesh);
            return id;
        }
        let id = self.next_mesh_id;
        self.next_mesh_id += 1;
        self.meshes.insert(id, mesh);
        self.mesh_name_to_id.insert(name.to_string(), id);
        id
    }

    /// 根据 ID 获取网格引用
    pub fn get_mesh(&self, id: u64) -> Option<&MeshData> {
        self.meshes.get(&id)
    }

    /// 根据名称获取网格 ID 和引用
    pub fn get_mesh_by_name(&self, name: &str) -> Option<(u64, &MeshData)> {
        let id = *self.mesh_name_to_id.get(name)?;
        let mesh = self.meshes.get(&id)?;
        Some((id, mesh))
    }

    /// 添加材质，返回分配的材质 ID
    ///
    /// 如果同名材质已存在，会覆盖原有数据并返回已有 ID。
    pub fn add_material(&mut self, name: &str, material: Material) -> u64 {
        if let Some(&id) = self.material_name_to_id.get(name) {
            self.materials.insert(id, material);
            return id;
        }
        let id = self.next_material_id;
        self.next_material_id += 1;
        self.materials.insert(id, material);
        self.material_name_to_id.insert(name.to_string(), id);
        id
    }

    /// 根据 ID 获取材质引用
    pub fn get_material(&self, id: u64) -> Option<&Material> {
        self.materials.get(&id)
    }

    /// 根据名称获取材质 ID 和引用
    pub fn get_material_by_name(&self, name: &str) -> Option<(u64, &Material)> {
        let id = *self.material_name_to_id.get(name)?;
        let material = self.materials.get(&id)?;
        Some((id, material))
    }

    /// 返回缓存中的网格数量
    pub fn mesh_count(&self) -> usize {
        self.meshes.len()
    }

    /// 返回缓存中的材质数量
    pub fn material_count(&self) -> usize {
        self.materials.len()
    }
}

impl Default for ResourceCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::icosphere;

    #[test]
    fn test_new_cache_is_empty() {
        let cache = ResourceCache::new();
        assert_eq!(cache.mesh_count(), 0);
        assert_eq!(cache.material_count(), 0);
    }

    #[test]
    fn test_add_and_get_mesh() {
        let mut cache = ResourceCache::new();
        let mesh = icosphere(1);
        let id = cache.add_mesh("sphere", mesh);

        assert_eq!(cache.mesh_count(), 1);
        assert!(cache.get_mesh(id).is_some());
        assert_eq!(cache.get_mesh(id).unwrap().vertices.len(), 42);
    }

    #[test]
    fn test_get_mesh_by_name() {
        let mut cache = ResourceCache::new();
        let mesh = icosphere(0);
        let id = cache.add_mesh("ico", mesh);

        let result = cache.get_mesh_by_name("ico");
        assert!(result.is_some());
        let (found_id, found_mesh) = result.unwrap();
        assert_eq!(found_id, id);
        assert_eq!(found_mesh.vertices.len(), 12);
    }

    #[test]
    fn test_get_missing_mesh() {
        let cache = ResourceCache::new();
        assert!(cache.get_mesh(42).is_none());
        assert!(cache.get_mesh_by_name("missing").is_none());
    }

    #[test]
    fn test_add_duplicate_mesh_name_overwrites() {
        let mut cache = ResourceCache::new();
        let mesh1 = icosphere(0);
        let id1 = cache.add_mesh("sphere", mesh1);

        let mesh2 = icosphere(1);
        let id2 = cache.add_mesh("sphere", mesh2);

        // 同名返回相同 ID
        assert_eq!(id1, id2);
        // 仍然只有一个网格
        assert_eq!(cache.mesh_count(), 1);
        // 数据已更新为细分 1 级的版本
        assert_eq!(cache.get_mesh(id1).unwrap().vertices.len(), 42);
    }

    #[test]
    fn test_add_and_get_material() {
        let mut cache = ResourceCache::new();
        let mat = Material {
            albedo: [1.0, 0.0, 0.0],
            ..Default::default()
        };
        let id = cache.add_material("red", mat);

        assert_eq!(cache.material_count(), 1);
        let found = cache.get_material(id).unwrap();
        assert_eq!(found.albedo, [1.0, 0.0, 0.0]);
    }

    #[test]
    fn test_get_material_by_name() {
        let mut cache = ResourceCache::new();
        let mat = Material {
            emissive: [1.0, 1.0, 0.0],
            ..Default::default()
        };
        let id = cache.add_material("sun", mat);

        let result = cache.get_material_by_name("sun");
        assert!(result.is_some());
        let (found_id, found_mat) = result.unwrap();
        assert_eq!(found_id, id);
        assert_eq!(found_mat.emissive, [1.0, 1.0, 0.0]);
    }

    #[test]
    fn test_multiple_meshes_and_materials() {
        let mut cache = ResourceCache::new();

        cache.add_mesh("low", icosphere(0));
        cache.add_mesh("med", icosphere(1));
        cache.add_mesh("high", icosphere(2));

        cache.add_material("a", Material::default());
        cache.add_material("b", Material::default());

        assert_eq!(cache.mesh_count(), 3);
        assert_eq!(cache.material_count(), 2);

        // ID 独立分配
        let (mesh_id, _) = cache.get_mesh_by_name("low").unwrap();
        let (mat_id, _) = cache.get_material_by_name("a").unwrap();
        // 网格和材质的 ID 序列各自独立
        assert_eq!(mesh_id, 0);
        assert_eq!(mat_id, 0);
    }
}
