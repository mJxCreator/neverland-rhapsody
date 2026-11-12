//! 挂载变换求解：世界变换 = 玩家根变换 × 骨骼全局变换 × 挂载局部变换。
//!
//! 骨骼全局变换已经包含绑定姿态，因此挂载模型可以使用骨骼局部空间坐标，
//! 不需要为人物模型的比例变化重新调整挂载位置。

use glam::Mat4;

use super::attachment_point::{AttachmentPoint, BoneId};
use crate::model::skeleton::{Pose, Skeleton};
use crate::model::types::Transform;

/// 指定骨骼在当前姿态下的模型空间矩阵。
pub fn bone_global(pose: &Pose, skeleton: &Skeleton, bone: BoneId) -> Option<Mat4> {
    let index = bone.index(skeleton)?;
    pose.globals.get(index).copied()
}

/// 求解一个挂载模型的最终世界变换。
///
/// 骨骼缺失、姿态关节数量不足或矩阵含非法数值时返回 `None`，
/// 调用方应跳过该挂载而不是让整件装备或整个角色消失。
pub fn attachment_transform(
    root: Mat4,
    pose: &Pose,
    skeleton: &Skeleton,
    point: &AttachmentPoint,
    local: Transform,
) -> Option<Mat4> {
    let bone = bone_global(pose, skeleton, point.bone)?;
    let matrix = root * bone * point.local.to_matrix() * local.to_matrix();
    matrix.is_finite().then_some(matrix)
}

#[cfg(test)]
mod tests {
    use glam::{Quat, Vec3};

    use super::*;
    use crate::attachment::attachment_point::{attachment_point, AttachmentId};
    use crate::model::skeleton::Joint;

    fn skeleton() -> Skeleton {
        let joints: Vec<Joint> = super::super::attachment_point::PLAYER_BONES
            .iter()
            .map(|name| Joint {
                name: name.to_string(),
                parent: None,
                local: Transform::IDENTITY,
                prefix: Transform::IDENTITY,
            })
            .collect();
        Skeleton {
            joints,
            inverse_bind: vec![Mat4::IDENTITY; 20],
        }
    }

    #[test]
    fn attachment_follows_bone_translation() {
        let skeleton = skeleton();
        let mut locals = skeleton.bind_locals();
        // 把右手骨骼移动到 (1, 2, 3)。
        locals[13].translation = Vec3::new(1.0, 2.0, 3.0);
        let pose = Pose::from_locals(&skeleton, &locals);
        let point = attachment_point(AttachmentId::RightHand);
        let root = Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0));
        let matrix = attachment_transform(root, &pose, &skeleton, point, Transform::IDENTITY)
            .expect("右手骨骼存在");
        assert!((matrix.w_axis.truncate() - Vec3::new(11.0, 2.0, 3.0)).length() < 1e-5);
    }

    #[test]
    fn attachment_local_offset_is_relative_to_the_bone() {
        let skeleton = skeleton();
        let pose = Pose::from_locals(&skeleton, &skeleton.bind_locals());
        let point = attachment_point(AttachmentId::Head);
        let local = Transform::new(Vec3::new(0.0, 0.25, 0.0), Quat::IDENTITY, Vec3::ONE);
        let matrix = attachment_transform(Mat4::IDENTITY, &pose, &skeleton, point, local)
            .expect("头部骨骼存在");
        assert!((matrix.w_axis.y - 0.25).abs() < 1e-5);
    }

    #[test]
    fn missing_bone_returns_none() {
        let skeleton = Skeleton {
            joints: Vec::new(),
            inverse_bind: Vec::new(),
        };
        let pose = Pose::from_locals(&skeleton, &[]);
        assert!(bone_global(&pose, &skeleton, BoneId::Head).is_none());
        assert!(attachment_transform(
            Mat4::IDENTITY,
            &pose,
            &skeleton,
            attachment_point(AttachmentId::Head),
            Transform::IDENTITY,
        )
        .is_none());
    }
}
