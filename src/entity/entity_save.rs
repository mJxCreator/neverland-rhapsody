//! 实体存档记录：只保存可恢复的状态字段，不保存 AI 内部随机数或渲染数据。
//!
//! 记录以区块为单位随世界存档持久化；已死亡实体不写入存档，避免重新加载后复活。

use glam::Vec3;
use serde::{Deserialize, Serialize};

use super::ai::AiState;
use super::creature::Creature;
use super::creature_ai::CreatureBrain;
use super::entity::{Entity, EntityId, EntityTypeId};
use super::entity_registry;
use crate::world::ChunkPos;

/// 单个实体的存档记录。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct EntityRecord {
    /// 稳定 ID。
    pub id: u64,
    /// 实体类型原始值。
    #[serde(rename = "type")]
    pub kind: u16,
    /// 脚底中心位置。
    pub x: f32,
    /// 脚底中心位置。
    pub y: f32,
    /// 脚底中心位置。
    pub z: f32,
    /// 速度。
    #[serde(default)]
    pub vx: f32,
    /// 速度。
    #[serde(default)]
    pub vy: f32,
    /// 速度。
    #[serde(default)]
    pub vz: f32,
    /// 当前生命。
    pub health: u32,
    /// AI 状态码。
    #[serde(default)]
    pub state: u8,
    /// AI 当前状态剩余时间。
    #[serde(default)]
    pub state_timer: f32,
    /// 活动锚点 X。
    #[serde(default)]
    pub anchor_x: f32,
    /// 活动锚点 Z。
    #[serde(default)]
    pub anchor_z: f32,
}

impl EntityRecord {
    /// 从运行中的生物提取记录；死亡实体不生成记录。
    pub fn from_creature(creature: &Creature) -> Option<Self> {
        if !creature.is_alive() {
            return None;
        }
        let entity = &creature.entity;
        let position = entity.position();
        let velocity = entity.velocity();
        let anchor = creature.brain.anchor();
        Some(Self {
            id: entity.id().raw(),
            kind: entity.kind().raw(),
            x: position.x,
            y: position.y,
            z: position.z,
            vx: velocity.x,
            vy: velocity.y,
            vz: velocity.z,
            health: entity.health(),
            state: creature.brain.state().code(),
            state_timer: creature.brain.timer(),
            anchor_x: anchor.x,
            anchor_z: anchor.z,
        })
    }

    /// 位置。
    pub fn position(&self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }

    /// 记录所属区块。
    pub fn chunk(&self) -> ChunkPos {
        super::entity::chunk_of(self.position())
    }

    /// 字段是否有效：类型已注册、坐标有限、生命在合法范围内、且不是死亡状态。
    pub fn is_valid(&self) -> bool {
        if !self.position().is_finite() {
            return false;
        }
        if !Vec3::new(self.vx, self.vy, self.vz).is_finite() {
            return false;
        }
        if !self.state_timer.is_finite() || !self.anchor_x.is_finite() || !self.anchor_z.is_finite()
        {
            return false;
        }
        let kind = EntityTypeId::new(self.kind);
        let definition = match entity_registry::definition(kind) {
            Some(definition) => definition,
            None => return false,
        };
        if self.health == 0 || self.health > definition.max_health {
            return false;
        }
        AiState::from_code(self.state) != AiState::Dead
    }

    /// 恢复为运行中的生物；无效记录返回 `None`。
    pub fn to_creature(self) -> Option<Creature> {
        if !self.is_valid() {
            return None;
        }
        let kind = EntityTypeId::new(self.kind);
        let definition = entity_registry::definition(kind)?;
        let position = self.position();
        let velocity = Vec3::new(self.vx, self.vy, self.vz);
        let id = EntityId::new(self.id);
        let mut entity = Entity::new(
            id,
            kind,
            position,
            0.0,
            definition.dimensions(),
            definition.max_health,
        );
        entity.set_velocity(velocity);
        entity.set_health(self.health);
        if velocity.x.abs() > 0.01 || velocity.z.abs() > 0.01 {
            entity.set_yaw(velocity.x.atan2(velocity.z));
        }

        let anchor = if self.anchor_x.abs() < 1e-4 && self.anchor_z.abs() < 1e-4 {
            position
        } else {
            Vec3::new(self.anchor_x, position.y, self.anchor_z)
        };
        let brain = CreatureBrain::restore(
            AiState::from_code(self.state),
            self.state_timer,
            anchor,
            self.id as u32,
        );
        Some(Creature::with_brain(entity, brain))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::entity::EntityDimensions;

    fn sample_creature() -> Creature {
        let entity = Entity::new(
            EntityId::new(9),
            EntityTypeId::COW,
            Vec3::new(4.5, 12.0, -3.5),
            0.7,
            EntityDimensions::new(0.9, 1.4),
            14,
        );
        Creature::new(entity)
    }

    #[test]
    fn roundtrip_preserves_state() {
        let mut creature = sample_creature();
        creature.entity.set_health(9);
        creature.entity.set_velocity(Vec3::new(0.4, -1.0, 0.2));
        creature.brain.set_state(AiState::Wandering, 1.25);
        creature
            .brain
            .flee(Vec3::new(4.5, 12.0, -3.5), Vec3::ZERO, 3.0);

        let record = EntityRecord::from_creature(&creature).unwrap();
        assert_eq!(record.id, 9);
        assert_eq!(record.kind, EntityTypeId::COW.raw());
        assert!((record.state_timer - 3.0).abs() < 1e-5);

        let restored = record.to_creature().unwrap();
        assert_eq!(restored.id(), EntityId::new(9));
        assert_eq!(restored.entity.health(), 9);
        assert!((restored.entity.position() - Vec3::new(4.5, 12.0, -3.5)).length() < 1e-5);
        assert_eq!(restored.brain.state(), AiState::Fleeing);
    }

    #[test]
    fn dead_creatures_are_not_recorded() {
        let mut creature = sample_creature();
        creature.entity.take_damage(14);
        assert!(EntityRecord::from_creature(&creature).is_none());
    }

    #[test]
    fn invalid_records_are_rejected() {
        let mut record = EntityRecord::from_creature(&sample_creature()).unwrap();
        record.kind = 999;
        assert!(!record.is_valid());
        assert!(record.to_creature().is_none());

        let mut record = EntityRecord::from_creature(&sample_creature()).unwrap();
        record.health = 0;
        assert!(!record.is_valid());

        let mut record = EntityRecord::from_creature(&sample_creature()).unwrap();
        record.x = f32::NAN;
        assert!(!record.is_valid());

        let mut record = EntityRecord::from_creature(&sample_creature()).unwrap();
        record.state = AiState::Dead.code();
        assert!(!record.is_valid());
    }

    #[test]
    fn serde_roundtrip_keeps_all_fields() {
        let record = EntityRecord::from_creature(&sample_creature()).unwrap();
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("\"type\""));
        let decoded: EntityRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, record);
    }

    #[test]
    fn chunk_follows_position() {
        let record = EntityRecord::from_creature(&sample_creature()).unwrap();
        assert_eq!(record.chunk(), ChunkPos::new(0, 0, -1));
    }
}
