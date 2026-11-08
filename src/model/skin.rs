//! 蒙皮：把 JOINTS_0 / WEIGHTS_0 与骨骼矩阵结合，计算每顶点变形结果。
//!
//! 本版本采用 CPU 蒙皮：低多边形生物顶点量很小，CPU 变形便于调试与单元测试，
//! 同时避免在顶点着色器中传递额外骨骼数据。静态模型不执行蒙皮。

use glam::{Mat3, Mat4, Vec3};

use super::mesh::ModelPrimitive;

/// 每个顶点最多受四个骨骼影响。
pub const MAX_JOINTS_PER_VERTEX: usize = 4;

/// 蒙皮后的顶点结果：位置与法线都在模型局部空间。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkinnedVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

/// 校验蒙皮索引与权重；返回可读错误。
pub fn validate_skin(primitive: &ModelPrimitive, joint_count: usize) -> Result<(), String> {
    let joints = primitive
        .joints
        .as_ref()
        .ok_or_else(|| "缺少 JOINTS_0".to_string())?;
    let weights = primitive
        .weights
        .as_ref()
        .ok_or_else(|| "缺少 WEIGHTS_0".to_string())?;
    for (index, (joint, weight)) in joints.iter().zip(weights).enumerate() {
        if joint.iter().any(|value| *value as usize >= joint_count) {
            return Err(format!("顶点 {index} 引用了越界的骨骼索引"));
        }
        if weight
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err(format!("顶点 {index} 的骨骼权重非法"));
        }
        let total: f32 = weight.iter().sum();
        if total <= 0.0 {
            return Err(format!("顶点 {index} 的骨骼权重全为 0"));
        }
    }
    Ok(())
}

/// 对每个顶点应用蒙皮矩阵，返回局部空间的位置与法线。
///
/// 没有蒙皮数据的图元按静态网格处理，原样返回顶点。
pub fn skin_primitive(primitive: &ModelPrimitive, skin: &[Mat4]) -> Vec<SkinnedVertex> {
    let Some(joints) = primitive.joints.as_ref() else {
        return static_vertices(primitive);
    };
    let Some(weights) = primitive.weights.as_ref() else {
        return static_vertices(primitive);
    };
    primitive
        .positions
        .iter()
        .zip(&primitive.normals)
        .zip(joints.iter().zip(weights))
        .map(|((position, normal), (joints, weights))| {
            let position = Vec3::from_array(*position);
            let normal = Vec3::from_array(*normal);
            let mut skinned_position = Vec3::ZERO;
            let mut skinned_normal = Vec3::ZERO;
            for (slot, joint) in joints.iter().enumerate() {
                let weight = weights[slot];
                if weight <= 0.0 {
                    continue;
                }
                let Some(matrix) = skin.get(*joint as usize) else {
                    continue;
                };
                skinned_position += weight * (matrix * position.extend(1.0)).truncate();
                let normal_matrix = Mat3::from_cols(
                    matrix.x_axis.truncate(),
                    matrix.y_axis.truncate(),
                    matrix.z_axis.truncate(),
                );
                skinned_normal += weight * (normal_matrix * normal);
            }
            if skinned_normal.length_squared() <= f32::EPSILON {
                skinned_normal = normal;
            }
            SkinnedVertex {
                position: skinned_position.to_array(),
                normal: skinned_normal.normalize_or_zero().to_array(),
            }
        })
        .collect()
}

fn static_vertices(primitive: &ModelPrimitive) -> Vec<SkinnedVertex> {
    primitive
        .positions
        .iter()
        .zip(&primitive.normals)
        .map(|(position, normal)| SkinnedVertex {
            position: *position,
            normal: *normal,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::types::Transform;

    fn primitive() -> ModelPrimitive {
        ModelPrimitive {
            positions: vec![[0.0, 0.0, 0.0]],
            normals: vec![[0.0, 1.0, 0.0]],
            uvs: vec![[0.0, 0.0]],
            tangents: None,
            joints: Some(vec![[0, 0, 0, 0]]),
            weights: Some(vec![[1.0, 0.0, 0.0, 0.0]]),
            indices: vec![0, 0, 0],
            material: 0,
        }
    }

    #[test]
    fn single_joint_translation_moves_vertex() {
        let primitive = primitive();
        let skin = vec![Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0))];
        let result = skin_primitive(&primitive, &skin);
        assert_eq!(result.len(), 1);
        assert!((Vec3::from_array(result[0].position) - Vec3::new(1.0, 2.0, 3.0)).length() < 1e-5);
    }

    #[test]
    fn two_joints_blend_by_weight() {
        let mut primitive = primitive();
        primitive.weights = Some(vec![[0.5, 0.5, 0.0, 0.0]]);
        primitive.joints = Some(vec![[0, 1, 0, 0]]);
        let skin = vec![
            Mat4::IDENTITY,
            Mat4::from_translation(Vec3::new(2.0, 0.0, 0.0)),
        ];
        let result = skin_primitive(&primitive, &skin);
        assert!((result[0].position[0] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn invalid_joint_indices_are_rejected() {
        let mut primitive = primitive();
        primitive.joints = Some(vec![[3, 0, 0, 0]]);
        assert!(validate_skin(&primitive, 2).is_err());
    }

    #[test]
    fn zero_weights_are_rejected() {
        let mut primitive = primitive();
        primitive.weights = Some(vec![[0.0; 4]]);
        assert!(validate_skin(&primitive, 1).is_err());
    }

    #[test]
    fn transform_rotation_produces_finite_normals() {
        let primitive = primitive();
        let skin = vec![
            Transform::new(Vec3::ZERO, glam::Quat::from_rotation_z(1.0), Vec3::ONE).to_matrix(),
        ];
        let result = skin_primitive(&primitive, &skin);
        assert!(result[0].normal.iter().all(|value| value.is_finite()));
    }
}
