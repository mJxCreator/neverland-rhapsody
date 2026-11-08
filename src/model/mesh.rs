//! 网格数据：顶点属性、索引、蒙皮权重与包围盒。
//!
//! 所有坐标都是模型局部空间；节点变换在导入阶段已经烘焙，避免渲染阶段
//! 重复修正坐标。静态网格与蒙皮网格使用同一份数据结构。

use glam::Vec3;

/// 轴对齐包围盒。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshBounds {
    pub min: Vec3,
    pub max: Vec3,
}

impl MeshBounds {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    /// 由顶点计算包围盒；空顶点返回 `None`。
    pub fn from_positions(positions: &[[f32; 3]]) -> Option<Self> {
        let mut iter = positions.iter();
        let first = iter.next()?;
        let mut min = Vec3::from_array(*first);
        let mut max = min;
        for position in iter {
            let value = Vec3::from_array(*position);
            min = min.min(value);
            max = max.max(value);
        }
        Some(Self { min, max })
    }

    /// 合并两个包围盒。
    pub fn union(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    /// 包围盒尺寸。
    pub fn size(self) -> Vec3 {
        self.max - self.min
    }

    /// 包围盒中心。
    pub fn center(self) -> Vec3 {
        (self.min + self.max) * 0.5
    }
}

/// 一个网格图元：单一材质与一组顶点/索引。
#[derive(Clone, Debug, PartialEq)]
pub struct ModelPrimitive {
    /// 顶点位置（模型局部空间）。
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// 可选切线；本版本不解算法线贴图，仅保留数据。
    pub tangents: Option<Vec<[f32; 4]>>,
    /// 每顶点四个骨骼索引。
    pub joints: Option<Vec<[u16; 4]>>,
    /// 每顶点四个骨骼权重，加载时已归一化。
    pub weights: Option<Vec<[f32; 4]>>,
    pub indices: Vec<u32>,
    /// 材质在 `ModelAsset::materials` 中的下标。
    pub material: usize,
}

impl ModelPrimitive {
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// 是否具备完整的蒙皮数据。
    pub fn is_skinned(&self) -> bool {
        self.joints.is_some() && self.weights.is_some()
    }

    /// 校验各顶点属性长度一致、索引完整；返回可读的错误信息。
    pub fn validate(&self) -> Result<(), String> {
        let count = self.positions.len();
        if !self.normals.len().eq(&count) {
            return Err("NORMAL 数量与 POSITION 不一致".to_string());
        }
        if !self.uvs.len().eq(&count) {
            return Err("TEXCOORD_0 数量与 POSITION 不一致".to_string());
        }
        if let Some(tangents) = &self.tangents {
            if !tangents.len().eq(&count) {
                return Err("TANGENT 数量与 POSITION 不一致".to_string());
            }
        }
        if let Some(joints) = &self.joints {
            if !joints.len().eq(&count) {
                return Err("JOINTS_0 数量与 POSITION 不一致".to_string());
            }
        }
        if let Some(weights) = &self.weights {
            if !weights.len().eq(&count) {
                return Err("WEIGHTS_0 数量与 POSITION 不一致".to_string());
            }
        }
        if !self.indices.len().is_multiple_of(3) {
            return Err("索引数量不是 3 的倍数".to_string());
        }
        if self.indices.iter().any(|index| *index as usize >= count) {
            return Err("索引越界：引用了不存在的顶点".to_string());
        }
        Ok(())
    }

    /// 由三角形顶点计算平坦法线；用于缺少 NORMAL 的网格。
    pub fn compute_flat_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
        let mut normals = vec![[0.0, 1.0, 0.0]; positions.len()];
        for triangle in indices.chunks_exact(3) {
            let a = Vec3::from_array(positions[triangle[0] as usize]);
            let b = Vec3::from_array(positions[triangle[1] as usize]);
            let c = Vec3::from_array(positions[triangle[2] as usize]);
            let normal = (b - a).cross(c - a).normalize_or_zero();
            for &index in triangle {
                normals[index as usize] = normal.to_array();
            }
        }
        normals
    }
}

/// 一个网格，可包含多个材质图元。
#[derive(Clone, Debug)]
pub struct ModelMesh {
    pub name: String,
    pub primitives: Vec<ModelPrimitive>,
    pub bounds: MeshBounds,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_cover_all_vertices() {
        let bounds =
            MeshBounds::from_positions(&[[1.0, 2.0, 3.0], [-1.0, 0.0, 4.0], [0.0, 3.0, -2.0]])
                .expect("非空顶点");
        assert_eq!(bounds.min, Vec3::new(-1.0, 0.0, -2.0));
        assert_eq!(bounds.max, Vec3::new(1.0, 3.0, 4.0));
        assert_eq!(bounds.center(), Vec3::new(0.0, 1.5, 1.0));
    }

    #[test]
    fn validation_rejects_index_overflow() {
        let primitive = ModelPrimitive {
            positions: vec![[0.0; 3]],
            normals: vec![[0.0, 1.0, 0.0]],
            uvs: vec![[0.0, 0.0]],
            tangents: None,
            joints: None,
            weights: None,
            indices: vec![0, 1, 2],
            material: 0,
        };
        assert!(primitive.validate().is_err());
    }

    #[test]
    fn flat_normals_are_unit_length() {
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let normals = ModelPrimitive::compute_flat_normals(&positions, &[0, 1, 2]);
        assert_eq!(normals.len(), 3);
        for normal in normals {
            assert!((Vec3::from_array(normal).length() - 1.0).abs() < 1e-5);
        }
    }
}
