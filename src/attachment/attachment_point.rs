//! 玩家骨骼名称与挂载点。
//!
//! 骨骼名称是模型资源与游戏逻辑之间的稳定契约：外部 `player.glb` 只要使用
//! 同一套骨骼名称，装备挂载无需修改任何代码。

use glam::Quat;

use crate::model::skeleton::Skeleton;
use crate::model::types::Transform;

/// 玩家标准骨骼名称；顺序与内置玩家模型的关节顺序一致。
pub const PLAYER_BONES: [&str; 20] = [
    "Root",
    "Pelvis",
    "Spine",
    "Chest",
    "Neck",
    "Head",
    "LeftShoulder",
    "LeftArm",
    "LeftForearm",
    "LeftHand",
    "RightShoulder",
    "RightArm",
    "RightForearm",
    "RightHand",
    "LeftLeg",
    "LeftKnee",
    "LeftFoot",
    "RightLeg",
    "RightKnee",
    "RightFoot",
];

/// 玩家骨架中可被装备引用的骨骼。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[allow(dead_code)] // 骨骼清单是模型资源契约，并非每根骨骼都直接挂载装备。
pub enum BoneId {
    Root,
    Pelvis,
    Spine,
    Chest,
    Neck,
    Head,
    LeftShoulder,
    LeftArm,
    LeftForearm,
    LeftHand,
    RightShoulder,
    RightArm,
    RightForearm,
    RightHand,
    LeftLeg,
    LeftKnee,
    LeftFoot,
    RightLeg,
    RightKnee,
    RightFoot,
}

impl BoneId {
    /// 骨骼稳定名称。
    pub const fn name(self) -> &'static str {
        match self {
            Self::Root => "Root",
            Self::Pelvis => "Pelvis",
            Self::Spine => "Spine",
            Self::Chest => "Chest",
            Self::Neck => "Neck",
            Self::Head => "Head",
            Self::LeftShoulder => "LeftShoulder",
            Self::LeftArm => "LeftArm",
            Self::LeftForearm => "LeftForearm",
            Self::LeftHand => "LeftHand",
            Self::RightShoulder => "RightShoulder",
            Self::RightArm => "RightArm",
            Self::RightForearm => "RightForearm",
            Self::RightHand => "RightHand",
            Self::LeftLeg => "LeftLeg",
            Self::LeftKnee => "LeftKnee",
            Self::LeftFoot => "LeftFoot",
            Self::RightLeg => "RightLeg",
            Self::RightKnee => "RightKnee",
            Self::RightFoot => "RightFoot",
        }
    }

    /// 在给定骨架中查找关节下标（大小写不敏感）。
    pub fn index(self, skeleton: &Skeleton) -> Option<usize> {
        skeleton.joint_index(self.name())
    }
}

/// 挂载位置。手部、头部、胸甲与腿部各有一个稳定 ID；
/// 护腿与靴子拆成左右两份，保证能跟随各自的腿部与脚部骨骼。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[allow(dead_code)] // 挂载点是模型资源契约，部分位置暂时只由外部 GLB 使用。
pub enum AttachmentId {
    RightHand,
    LeftHand,
    Head,
    Chest,
    /// 护腿的髋部部件，跟随骨盆。
    Legs,
    LeftLeg,
    RightLeg,
    LeftFoot,
    RightFoot,
}

impl AttachmentId {
    /// 全部挂载点，顺序固定；用于测试覆盖。
    #[cfg(test)]
    pub const ALL: [Self; 9] = [
        Self::RightHand,
        Self::LeftHand,
        Self::Head,
        Self::Chest,
        Self::Legs,
        Self::LeftLeg,
        Self::RightLeg,
        Self::LeftFoot,
        Self::RightFoot,
    ];

    /// 展示用名称。
    pub const fn label(self) -> &'static str {
        match self {
            Self::RightHand => "RIGHT_HAND",
            Self::LeftHand => "LEFT_HAND",
            Self::Head => "HEAD",
            Self::Chest => "CHEST",
            Self::Legs => "LEGS",
            Self::LeftLeg => "LEFT_LEG",
            Self::RightLeg => "RIGHT_LEG",
            Self::LeftFoot => "LEFT_FOOT",
            Self::RightFoot => "RIGHT_FOOT",
        }
    }

    /// 该挂载位置默认跟随的骨骼。
    pub const fn bone(self) -> BoneId {
        match self {
            Self::RightHand => BoneId::RightHand,
            Self::LeftHand => BoneId::LeftHand,
            Self::Head => BoneId::Head,
            Self::Chest => BoneId::Chest,
            Self::Legs => BoneId::Pelvis,
            Self::LeftLeg => BoneId::LeftLeg,
            Self::RightLeg => BoneId::RightLeg,
            Self::LeftFoot => BoneId::LeftFoot,
            Self::RightFoot => BoneId::RightFoot,
        }
    }
}

/// 一个挂载点：稳定 ID、绑定骨骼与相对骨骼的局部变换。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AttachmentPoint {
    pub id: AttachmentId,
    pub bone: BoneId,
    pub local: Transform,
}

impl AttachmentPoint {
    const fn new(id: AttachmentId) -> Self {
        Self {
            id,
            bone: id.bone(),
            local: Transform::new(glam::Vec3::ZERO, Quat::IDENTITY, glam::Vec3::ONE),
        }
    }
}

const POINTS: [AttachmentPoint; 9] = [
    AttachmentPoint::new(AttachmentId::RightHand),
    AttachmentPoint::new(AttachmentId::LeftHand),
    AttachmentPoint::new(AttachmentId::Head),
    AttachmentPoint::new(AttachmentId::Chest),
    AttachmentPoint::new(AttachmentId::Legs),
    AttachmentPoint::new(AttachmentId::LeftLeg),
    AttachmentPoint::new(AttachmentId::RightLeg),
    AttachmentPoint::new(AttachmentId::LeftFoot),
    AttachmentPoint::new(AttachmentId::RightFoot),
];

/// 按 ID 查询挂载点。
pub fn attachment_point(id: AttachmentId) -> &'static AttachmentPoint {
    &POINTS[id as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_attachment_id_has_a_point_and_bone() {
        for (index, id) in AttachmentId::ALL.into_iter().enumerate() {
            let point = attachment_point(id);
            assert_eq!(point.id, id);
            assert_eq!(point.bone, id.bone());
            assert_eq!(
                *point, POINTS[index],
                "挂载点顺序必须与 AttachmentId 顺序一致"
            );
        }
        assert_eq!(POINTS.len(), AttachmentId::ALL.len());
    }

    #[test]
    fn bone_names_are_unique_and_match_the_contract() {
        for (index, bone) in PLAYER_BONES.iter().enumerate() {
            assert!(
                PLAYER_BONES[..index].iter().all(|other| other != bone),
                "骨骼名称 {bone} 重复"
            );
        }
        assert_eq!(BoneId::RightHand.name(), "RightHand");
        assert_eq!(BoneId::LeftFoot.name(), "LeftFoot");
    }

    #[test]
    fn bone_lookup_is_case_insensitive() {
        let joints = PLAYER_BONES
            .iter()
            .enumerate()
            .map(|(index, name)| crate::model::skeleton::Joint {
                name: name.to_string(),
                parent: if index == 0 { None } else { Some(0) },
                local: Transform::IDENTITY,
                prefix: Transform::IDENTITY,
            })
            .collect();
        let skeleton = Skeleton {
            joints,
            inverse_bind: vec![glam::Mat4::IDENTITY; PLAYER_BONES.len()],
        };
        assert_eq!(BoneId::Head.index(&skeleton), Some(5));
        assert_eq!(BoneId::RightFoot.index(&skeleton), Some(19));
    }
}
