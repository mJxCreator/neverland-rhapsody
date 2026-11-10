//! 模型基础类型：资源标识、变换、纹理与运行时模型资源。
//!
//! 模型资源与模型实例分离：`ModelAsset` 只描述可共享的静态数据，
//! 位置、朝向与动画状态保存在 [`super::instance::ModelInstance`] 中。

use glam::{Mat4, Quat, Vec3};

use super::animation::AnimationClip;
use super::material::ModelMaterial;
use super::mesh::{MeshBounds, ModelMesh};
use super::skeleton::Skeleton;
use crate::texture::types::TextureImage;

/// 模型资源唯一标识；同一名称在同一注册表中总是得到相同编号。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelId(u32);

impl ModelId {
    /// 无效模型：任何查询都应按“无模型”处理。
    pub const INVALID: Self = Self(u32::MAX);

    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }

    pub const fn is_valid(self) -> bool {
        self.0 != u32::MAX
    }
}

/// 仿射变换的分解形式，供骨骼绑定姿态与动画通道使用。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Transform {
    pub const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    pub const fn new(translation: Vec3, rotation: Quat, scale: Vec3) -> Self {
        Self {
            translation,
            rotation,
            scale,
        }
    }

    /// 由列主序矩阵分解；缩放为零时回退为 1，避免后续矩阵退化。
    pub fn from_matrix(matrix: Mat4) -> Self {
        let (scale, rotation, translation) = matrix.to_scale_rotation_translation();
        let scale = Vec3::new(
            safe_scale(scale.x),
            safe_scale(scale.y),
            safe_scale(scale.z),
        );
        Self {
            translation,
            rotation,
            scale,
        }
    }

    pub fn to_matrix(self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }

    /// 线性混合两个变换；用于动画切换时的姿态淡入，避免明显跳变。
    pub fn lerp(from: Self, to: Self, weight: f32) -> Self {
        let weight = weight.clamp(0.0, 1.0);
        if weight <= 0.0 {
            return from;
        }
        if weight >= 1.0 {
            return to;
        }
        Self {
            translation: from.translation.lerp(to.translation, weight),
            rotation: from
                .rotation
                .normalize()
                .lerp(to.rotation.normalize(), weight),
            scale: from.scale.lerp(to.scale, weight),
        }
    }
}

fn safe_scale(value: f32) -> f32 {
    if value.abs() < 1.0e-6 {
        1.0
    } else {
        value
    }
}

/// 一张已解码的模型纹理（PNG 或内嵌在 GLB 的图片）。
#[derive(Clone, Debug, PartialEq)]
pub struct ModelTexture {
    pub name: String,
    pub image: TextureImage,
}

/// 一个可共享的运行时模型资源。
#[derive(Clone, Debug)]
pub struct ModelAsset {
    pub name: String,
    pub meshes: Vec<ModelMesh>,
    pub materials: Vec<ModelMaterial>,
    pub textures: Vec<ModelTexture>,
    pub skeleton: Option<Skeleton>,
    pub animations: Vec<AnimationClip>,
}

impl ModelAsset {
    /// 所有网格包围盒的并集。
    pub fn bounds(&self) -> Option<MeshBounds> {
        self.meshes
            .iter()
            .map(|mesh| mesh.bounds)
            .reduce(MeshBounds::union)
    }

    /// 让模型脚底贴地的垂直补偿。
    ///
    /// 规范要求资源本身把原点放在脚底，但外部导出常有偏移；
    /// 这里只按包围盒最低点校正 Y，不做水平居中：
    /// 猪、牛等生物的头部前伸会让包围盒在 Z 轴上天然不对称，
    /// 强行居中反而会把模型整体后移。
    pub fn ground_offset(&self) -> Vec3 {
        match self.bounds() {
            Some(bounds) => Vec3::new(0.0, -bounds.min.y, 0.0),
            None => Vec3::ZERO,
        }
    }

    /// 按名称查找动画片段（大小写不敏感）。
    pub fn animation_index(&self, name: &str) -> Option<usize> {
        self.animations
            .iter()
            .position(|clip| clip.name.eq_ignore_ascii_case(name))
    }

    /// 该资源是否带有骨骼，可进行蒙皮与动画。
    pub fn is_skinned(&self) -> bool {
        self.skeleton.is_some()
    }

    /// 网格包含的三角形数量，用于启动期统计。
    pub fn triangle_count(&self) -> usize {
        self.meshes
            .iter()
            .flat_map(|mesh| mesh.primitives.iter())
            .map(|primitive| primitive.indices.len() / 3)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_roundtrips_through_matrix() {
        let transform = Transform::new(
            Vec3::new(1.0, 2.0, 3.0),
            Quat::from_rotation_y(0.7),
            Vec3::new(1.0, 2.0, 0.5),
        );
        let rebuilt = Transform::from_matrix(transform.to_matrix());
        assert!((rebuilt.translation - transform.translation).length() < 1e-5);
        assert!((rebuilt.scale - transform.scale).length() < 1e-5);
        assert!(rebuilt.rotation.abs_diff_eq(transform.rotation, 1e-5));
    }

    #[test]
    fn transform_lerp_hits_both_ends() {
        let from = Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
        let to = Transform::new(Vec3::X, Quat::from_rotation_y(1.0), Vec3::splat(2.0));
        assert_eq!(Transform::lerp(from, to, 0.0), from);
        assert_eq!(Transform::lerp(from, to, 1.0), to);
        let middle = Transform::lerp(from, to, 0.5);
        assert!((middle.translation - Vec3::new(0.5, 0.0, 0.0)).length() < 1e-6);
    }

    #[test]
    fn model_ids_are_stable_values() {
        assert_eq!(ModelId::new(3).raw(), 3);
        assert!(ModelId::new(0).is_valid());
        assert!(!ModelId::INVALID.is_valid());
    }
}
