//! 模型渲染数据：把模型实例转换为绘制批次所需的顶点与世界变换。
//!
//! 顶点使用模型局部空间，实例世界变换单独传递；这样静态模型可以缓存
//! GPU 顶点缓冲，只有蒙皮模型需要逐帧更新顶点。

use bytemuck::{Pod, Zeroable};
use glam::Mat4;

use super::material::AlphaMode;
use super::mesh::ModelPrimitive;
use super::skin::skin_primitive;
use super::types::{ModelAsset, ModelId};
use super::ModelInstance;

/// 模型渲染顶点；颜色已包含材质基础颜色。
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct ModelVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

impl ModelVertex {
    /// 静态图元的 GPU 顶点数据。
    pub fn from_primitive(primitive: &ModelPrimitive, color: [f32; 4]) -> Vec<Self> {
        primitive
            .positions
            .iter()
            .zip(&primitive.normals)
            .zip(&primitive.uvs)
            .map(|((position, normal), uv)| Self {
                position: *position,
                normal: *normal,
                uv: *uv,
                color,
            })
            .collect()
    }
}

/// 绘制几何来源。
#[derive(Clone, Debug)]
pub enum ModelGeometry {
    /// 静态图元：由渲染器按（模型、网格、图元）缓存并共享 GPU 缓冲。
    Static { mesh: usize, primitive: usize },
    /// 蒙皮顶点：每帧由 CPU 计算并上传。
    Dynamic(Vec<ModelVertex>),
}

/// 一个待绘制批次。
#[derive(Clone, Debug)]
pub struct ModelDraw {
    pub model: ModelId,
    pub material: usize,
    pub alpha: AlphaMode,
    /// 实例世界变换（已包含缩放到实体尺寸的缩放）。
    pub transform: Mat4,
    pub geometry: ModelGeometry,
}

/// 把模型实例转换为绘制批次。
///
/// 静态图元在网格为空或没有骨骼时使用缓存路径；蒙皮图元使用当前姿态。
pub fn instance_draws(
    asset: &ModelAsset,
    instance: &ModelInstance,
    transform: Mat4,
    out: &mut Vec<ModelDraw>,
) {
    let pose_skin = &instance.pose().skin;
    // 外部模型的原点常不在脚底，按包围盒最低点补偿后再套用实例世界变换。
    let transform = transform * Mat4::from_translation(asset.ground_offset());
    for (mesh_index, mesh) in asset.meshes.iter().enumerate() {
        for (primitive_index, primitive) in mesh.primitives.iter().enumerate() {
            let material = asset
                .materials
                .get(primitive.material)
                .cloned()
                .unwrap_or_default();
            let skinned =
                primitive.is_skinned() && asset.skeleton.is_some() && !pose_skin.is_empty();
            let geometry = if skinned {
                let skinned_vertices = skin_primitive(primitive, pose_skin);
                // 动态路径直接把顶点交给三角形列表绘制，必须按索引展开：
                // 否则方形只剩一个三角形，立方体相邻面顶点会被错连成退化三角形。
                let order: Vec<u32> = if primitive.indices.is_empty() {
                    (0..skinned_vertices.len() as u32).collect()
                } else {
                    primitive.indices.clone()
                };
                let vertices = order
                    .into_iter()
                    .filter_map(|index| {
                        let vertex = skinned_vertices.get(index as usize)?;
                        let uv = primitive
                            .uvs
                            .get(index as usize)
                            .copied()
                            .unwrap_or([0.0; 2]);
                        Some(ModelVertex {
                            position: vertex.position,
                            normal: vertex.normal,
                            uv,
                            color: material.base_color,
                        })
                    })
                    .collect();
                ModelGeometry::Dynamic(vertices)
            } else {
                ModelGeometry::Static {
                    mesh: mesh_index,
                    primitive: primitive_index,
                }
            };
            out.push(ModelDraw {
                model: instance.model,
                material: primitive.material,
                alpha: material.alpha_mode,
                transform,
                geometry,
            });
        }
    }
}

/// 统计顶点数，用于按模型规模调整动态缓冲容量。
pub fn dynamic_vertex_count(draws: &[ModelDraw]) -> usize {
    draws
        .iter()
        .filter_map(|draw| match &draw.geometry {
            ModelGeometry::Dynamic(vertices) => Some(vertices.len()),
            ModelGeometry::Static { .. } => None,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::mesh::ModelMesh;
    use crate::model::types::Transform;
    use glam::Vec3;

    fn asset() -> ModelAsset {
        let primitive = ModelPrimitive {
            positions: vec![[0.0, 0.0, 0.0]],
            normals: vec![[0.0, 1.0, 0.0]],
            uvs: vec![[0.0, 0.0]],
            tangents: None,
            joints: None,
            weights: None,
            indices: vec![0, 0, 0],
            material: 0,
        };
        ModelAsset {
            name: "box".to_string(),
            meshes: vec![ModelMesh {
                name: "box".to_string(),
                bounds: crate::model::mesh::MeshBounds::new(Vec3::ZERO, Vec3::ONE),
                primitives: vec![primitive],
            }],
            materials: vec![crate::model::material::ModelMaterial::colored(
                "body",
                [1.0, 0.5, 0.5, 1.0],
            )],
            textures: Vec::new(),
            skeleton: None,
            animations: Vec::new(),
        }
    }

    #[test]
    fn static_model_uses_cached_geometry() {
        let asset = asset();
        let instance = ModelInstance::new(ModelId::new(0));
        let mut draws = Vec::new();
        instance_draws(
            &asset,
            &instance,
            Transform::IDENTITY.to_matrix(),
            &mut draws,
        );
        assert_eq!(draws.len(), 1);
        assert!(matches!(
            draws[0].geometry,
            ModelGeometry::Static {
                mesh: 0,
                primitive: 0
            }
        ));
        assert_eq!(draws[0].alpha, AlphaMode::Opaque);
    }

    #[test]
    fn dynamic_vertex_count_ignores_static_batches() {
        let asset = asset();
        let instance = ModelInstance::new(ModelId::new(0));
        let mut draws = Vec::new();
        instance_draws(&asset, &instance, Mat4::IDENTITY, &mut draws);
        assert_eq!(dynamic_vertex_count(&draws), 0);
    }

    #[test]
    fn skinned_quad_is_expanded_by_indices() {
        // 4 个顶点 + 6 个索引的方形，动态路径必须展开成 6 个顶点，
        // 否则三角形列表只能画出半个方片。
        let primitive = ModelPrimitive {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            tangents: None,
            joints: Some(vec![[0, 0, 0, 0]; 4]),
            weights: Some(vec![[1.0, 0.0, 0.0, 0.0]; 4]),
            indices: vec![0, 1, 2, 0, 2, 3],
            material: 0,
        };
        let mut asset = asset();
        asset.meshes[0].primitives = vec![primitive];
        asset.skeleton = Some(crate::model::skeleton::Skeleton {
            joints: vec![crate::model::skeleton::Joint {
                name: "root".to_string(),
                parent: None,
                local: Transform::IDENTITY,
                prefix: Transform::IDENTITY,
            }],
            inverse_bind: vec![Mat4::IDENTITY],
        });
        let mut instance = ModelInstance::new(ModelId::new(0));
        instance.update(&asset, 0.0);
        let mut draws = Vec::new();
        instance_draws(&asset, &instance, Mat4::IDENTITY, &mut draws);
        let ModelGeometry::Dynamic(vertices) = &draws[0].geometry else {
            panic!("蒙皮图元应走动态路径");
        };
        assert_eq!(vertices.len(), 6);
        assert_eq!(vertices[3].uv, [0.0, 0.0]);
        assert_eq!(vertices[4].uv, [1.0, 1.0]);
        assert_eq!(vertices[5].uv, [0.0, 1.0]);
    }
}
