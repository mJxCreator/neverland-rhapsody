//! 生物实体：把实体生命周期状态与 AI 大脑组合成管理器实际存储的类型。
//!
//! [`Creature`] 只是组合体，不包含物理或渲染逻辑；这些能力由独立模块按需访问字段。

use super::creature_ai::CreatureBrain;
use super::entity::{Entity, EntityId, EntityTypeId};

/// 一个生物实体。
#[derive(Clone, Debug)]
pub struct Creature {
    /// 实体基础状态。
    pub entity: Entity,
    /// AI 状态机。
    pub brain: CreatureBrain,
}

impl Creature {
    /// 由实体创建生物，AI 锚点取出生位置。
    pub fn new(entity: Entity) -> Self {
        let brain = CreatureBrain::from(&entity);
        Self { entity, brain }
    }

    /// 用已有大脑组装，用于存档恢复。
    pub fn with_brain(entity: Entity, brain: CreatureBrain) -> Self {
        Self { entity, brain }
    }

    /// 稳定 ID。
    pub fn id(&self) -> EntityId {
        self.entity.id()
    }

    /// 实体类型。
    pub fn kind(&self) -> EntityTypeId {
        self.entity.kind()
    }

    /// 是否存活。
    pub fn is_alive(&self) -> bool {
        self.entity.is_alive()
    }

    /// 是否已死亡。
    pub fn is_dead(&self) -> bool {
        self.entity.is_dead()
    }

    /// 死亡时让 AI 同步进入死亡状态。
    pub fn mark_dead(&mut self) {
        self.brain.mark_dead();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::ai::AiState;
    use crate::entity::entity::{EntityDimensions, EntityTypeId};
    use glam::Vec3;

    #[test]
    fn new_creature_gets_idle_brain() {
        let entity = Entity::new(
            EntityId::new(1),
            EntityTypeId::COW,
            Vec3::new(0.0, 10.0, 0.0),
            0.0,
            EntityDimensions::new(0.9, 1.4),
            14,
        );
        let creature = Creature::new(entity);
        assert_eq!(creature.id(), EntityId::new(1));
        assert_eq!(creature.brain.state(), AiState::Idle);
        assert!(creature.is_alive());
    }

    #[test]
    fn mark_dead_propagates_to_brain() {
        let entity = Entity::new(
            EntityId::new(2),
            EntityTypeId::PIG,
            Vec3::ZERO,
            0.0,
            EntityDimensions::new(0.9, 0.9),
            10,
        );
        let mut creature = Creature::new(entity);
        creature.entity.take_damage(10);
        creature.mark_dead();
        assert_eq!(creature.brain.state(), AiState::Dead);
        assert!(!creature.is_alive());
    }
}
