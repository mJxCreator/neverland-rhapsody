//! 运行时挂载列表：由视觉层按当前装备状态重建，并生成静态模型绘制批次。
//!
//! 管理器只保存“挂载点 → 模型 + 局部变换”，不读写装备属性。
//! 每帧重建的代价极小（最多 9 个挂载），换来装备切换即时生效且不会产生陈旧状态。

use std::collections::HashSet;

use glam::Mat4;

use super::attachment_point::{attachment_point, AttachmentId};
use super::attachment_transform::attachment_transform;
use crate::model::skeleton::{Pose, Skeleton};
use crate::model::types::{ModelId, Transform};
use crate::model::{static_draws, ModelDraw, ModelRegistry};

/// 一个已解析的挂载项。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActiveAttachment {
    pub id: AttachmentId,
    pub model: ModelId,
    /// 相对挂载点（骨骼）的局部变换。
    pub local: Transform,
}

/// 全部当前挂载项，按挂载点唯一。
#[derive(Debug, Default)]
pub struct AttachmentManager {
    active: Vec<ActiveAttachment>,
    /// 已告警过的（模型名, 挂载点），避免每帧刷屏。
    warned: HashSet<(String, AttachmentId)>,
}

impl AttachmentManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// 清空全部挂载；装备状态重建前调用。
    pub fn clear(&mut self) {
        self.active.clear();
    }

    /// 添加或替换同一挂载点的模型。
    pub fn attach(&mut self, id: AttachmentId, model: ModelId, local: Transform) {
        if let Some(existing) = self.active.iter_mut().find(|item| item.id == id) {
            *existing = ActiveAttachment { id, model, local };
            return;
        }
        self.active.push(ActiveAttachment { id, model, local });
    }

    #[cfg(test)]
    pub fn get(&self, id: AttachmentId) -> Option<&ActiveAttachment> {
        self.active.iter().find(|item| item.id == id)
    }

    #[cfg(test)]
    pub fn detach(&mut self, id: AttachmentId) {
        self.active.retain(|item| item.id != id);
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.active.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }

    /// 生成全部挂载模型的静态绘制批次。
    ///
    /// 骨骼或模型缺失时跳过该挂载并输出一次告警，不影响玩家本体与其他装备。
    pub fn build_draws(
        &mut self,
        models: &ModelRegistry,
        root: Mat4,
        pose: &Pose,
        skeleton: &Skeleton,
        out: &mut Vec<ModelDraw>,
    ) {
        for attachment in &self.active {
            let Some(asset) = models.get(attachment.model) else {
                continue;
            };
            let point = attachment_point(attachment.id);
            let Some(transform) =
                attachment_transform(root, pose, skeleton, point, attachment.local)
            else {
                if self.warned.insert((asset.name.clone(), attachment.id)) {
                    tracing::warn!(
                        model = %asset.name,
                        attachment = attachment.id.label(),
                        bone = point.bone.name(),
                        "玩家骨架缺少挂载骨骼，已跳过该装备模型"
                    );
                }
                continue;
            };
            static_draws(&asset, attachment.model, transform, 0.0, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::Mat4;

    use super::*;
    use crate::attachment::attachment_point::PLAYER_BONES;
    use crate::model::skeleton::Joint;

    fn skeleton() -> Skeleton {
        let joints = PLAYER_BONES
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
            inverse_bind: vec![Mat4::IDENTITY; PLAYER_BONES.len()],
        }
    }

    fn registry() -> (ModelRegistry, ModelId) {
        let mut registry = ModelRegistry::new();
        // 挂载装备使用静态网格；第一人称手臂是现成的非蒙皮模型。
        registry.register_builtin(
            crate::player_visual::player_model::FIRST_PERSON_ARMS_MODEL,
            crate::player_visual::player_model::first_person_arms(),
        );
        let id = registry
            .by_name(crate::player_visual::player_model::FIRST_PERSON_ARMS_MODEL)
            .expect("内置手臂模型已注册");
        (registry, id)
    }

    #[test]
    fn attach_replaces_the_same_point_without_duplicates() {
        let mut manager = AttachmentManager::new();
        manager.attach(AttachmentId::Head, ModelId::new(0), Transform::IDENTITY);
        manager.attach(AttachmentId::Chest, ModelId::new(1), Transform::IDENTITY);
        manager.attach(AttachmentId::Head, ModelId::new(2), Transform::IDENTITY);
        assert_eq!(manager.len(), 2);
        assert_eq!(
            manager.get(AttachmentId::Head).map(|item| item.model),
            Some(ModelId::new(2))
        );
        manager.detach(AttachmentId::Chest);
        assert_eq!(manager.len(), 1);
    }

    #[test]
    fn draws_are_skipped_for_missing_bones() {
        let (registry, model) = registry();
        let mut manager = AttachmentManager::new();
        manager.attach(AttachmentId::RightHand, model, Transform::IDENTITY);
        let empty = Skeleton {
            joints: Vec::new(),
            inverse_bind: Vec::new(),
        };
        let pose = Pose::from_locals(&empty, &[]);
        let mut draws = Vec::new();
        manager.build_draws(&registry, Mat4::IDENTITY, &pose, &empty, &mut draws);
        assert!(draws.is_empty());
    }

    #[test]
    fn draws_are_generated_for_valid_bones() {
        let (registry, model) = registry();
        let skeleton = skeleton();
        let pose = Pose::from_locals(&skeleton, &skeleton.bind_locals());
        let mut manager = AttachmentManager::new();
        manager.attach(AttachmentId::RightHand, model, Transform::IDENTITY);
        let mut draws = Vec::new();
        manager.build_draws(&registry, Mat4::IDENTITY, &pose, &skeleton, &mut draws);
        assert!(!draws.is_empty());
    }
}
