//! 实体管理器：ID 分配、生成/销毁、区块归属、AI 更新与存档快照。
//!
//! 管理器是实体的唯一归属地；区块切换时实体仍留在同一列表，但所属区块字段会同步更新，
//! 因此不会出现“同一实体同时属于旧区块与新区块”的情况。

use glam::Vec3;

use super::ai::AiConfig;
use super::creature::Creature;
use super::entity::{chunk_of, DamageResult, Entity, EntityId, EntityTypeId};
use super::entity_registry;
use super::entity_save::EntityRecord;
use crate::world::{ChunkPos, World};

/// 死亡实体在掉落结算后保留的时间，随后销毁。
pub const DEATH_LINGER_SECONDS: f32 = 0.6;

/// 实体管理器。
pub struct EntityManager {
    entities: Vec<Creature>,
    next_id: u64,
}

impl Default for EntityManager {
    fn default() -> Self {
        Self::new()
    }
}

impl EntityManager {
    /// 创建空管理器。
    pub fn new() -> Self {
        Self {
            entities: Vec::new(),
            next_id: 1,
        }
    }

    /// 当前全部生物（含尚未清理的死亡实体）。
    pub fn creatures(&self) -> &[Creature] {
        &self.entities
    }

    /// 可变访问全部生物，仅供物理系统使用。
    pub fn creatures_mut(&mut self) -> &mut [Creature] {
        &mut self.entities
    }

    /// 总数。
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// 是否为空。
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// 存活数量。
    pub fn alive_count(&self) -> usize {
        self.entities
            .iter()
            .filter(|creature| creature.is_alive())
            .count()
    }

    /// 按 ID 查询。
    pub fn get(&self, id: EntityId) -> Option<&Creature> {
        self.entities.iter().find(|creature| creature.id() == id)
    }

    /// 按 ID 可变查询。
    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut Creature> {
        self.entities
            .iter_mut()
            .find(|creature| creature.id() == id)
    }

    /// 生成一个实体；类型未注册或坐标非法时返回 `None`。
    pub fn spawn(&mut self, kind: EntityTypeId, position: Vec3, yaw: f32) -> Option<EntityId> {
        if !position.is_finite() || !yaw.is_finite() {
            return None;
        }
        let definition = entity_registry::definition(kind)?;
        let id = EntityId::new(self.next_id);
        let entity = Entity::new(
            id,
            kind,
            position,
            yaw,
            definition.dimensions(),
            definition.max_health,
        );
        self.entities.push(Creature::new(entity));
        self.next_id += 1;
        Some(id)
    }

    /// 结算伤害并让目标逃离伤害来源；重复命中已死亡目标返回 [`DamageResult::Ignored`]。
    pub fn apply_damage(
        &mut self,
        id: EntityId,
        amount: u32,
        source: Vec3,
        flee_seconds: f32,
    ) -> DamageResult {
        let creature = match self.get_mut(id) {
            Some(creature) => creature,
            None => return DamageResult::Ignored,
        };
        if !creature.is_alive() {
            return DamageResult::Ignored;
        }
        let position = creature.entity.position();
        let result = creature.entity.take_damage(amount);
        match result {
            DamageResult::Applied { .. } => {
                creature.brain.flee(position, source, flee_seconds);
            }
            DamageResult::Killed => creature.mark_dead(),
            DamageResult::Ignored => {}
        }
        result
    }

    /// 让所有存活生物的 AI 推进一次；决策频率由 [`AiConfig`] 控制。
    pub fn update_ai(
        &mut self,
        delta: f32,
        player_position: Vec3,
        world: &World,
        config: &AiConfig,
    ) {
        if !delta.is_finite() || delta <= 0.0 {
            return;
        }
        for creature in self
            .entities
            .iter_mut()
            .filter(|creature| creature.is_alive())
        {
            let definition = match entity_registry::definition(creature.kind()) {
                Some(definition) => definition,
                None => continue,
            };
            let position = creature.entity.position();
            let grounded = creature.entity.is_grounded();
            creature.brain.update(
                position,
                player_position,
                world,
                definition,
                grounded,
                config,
                delta,
            );
        }
    }

    /// 推进死亡计时并移除到期实体；避免在遍历中修改列表。
    pub fn update_deaths(&mut self, delta: f32) -> usize {
        let mut removed = 0;
        let mut index = 0;
        while index < self.entities.len() {
            let expired = {
                let creature = &mut self.entities[index];
                creature.is_dead() && creature.entity.tick_death(delta, DEATH_LINGER_SECONDS)
            };
            if expired {
                self.entities.swap_remove(index);
                removed += 1;
            } else {
                index += 1;
            }
        }
        removed
    }

    /// 指定区块内的存活生物数量（按三维区块）。
    #[allow(dead_code)]
    pub fn chunk_entity_count(&self, chunk: ChunkPos) -> usize {
        self.entities
            .iter()
            .filter(|creature| creature.is_alive() && creature.entity.chunk() == chunk)
            .count()
    }

    /// 指定水平区块列内的存活生物数量，用于自然生成上限判断。
    pub fn column_entity_count(&self, column: ChunkPos) -> usize {
        let column = ChunkPos::new(column.x, 0, column.z);
        self.entities
            .iter()
            .filter(|creature| {
                creature.is_alive()
                    && ChunkPos::new(creature.entity.chunk().x, 0, creature.entity.chunk().z)
                        == column
            })
            .count()
    }

    /// 指定区块内的生物迭代器。
    #[allow(dead_code)]
    pub fn entities_in_chunk(&self, chunk: ChunkPos) -> impl Iterator<Item = &Creature> {
        self.entities
            .iter()
            .filter(move |creature| creature.entity.chunk() == chunk)
    }

    /// 生成该区块的存档快照；死亡实体不写入。
    pub fn snapshot_chunk(&self, chunk: ChunkPos) -> Vec<EntityRecord> {
        self.entities
            .iter()
            .filter(|creature| creature.entity.chunk() == chunk)
            .filter_map(EntityRecord::from_creature)
            .collect()
    }

    /// 移除并返回该区块的实体记录，用于区块卸载。
    pub fn take_chunk_entities(&mut self, chunk: ChunkPos) -> Vec<EntityRecord> {
        let records = self.snapshot_chunk(chunk);
        self.entities
            .retain(|creature| creature.entity.chunk() != chunk);
        records
    }

    /// 恢复指定区块的实体；已存在的 ID 会跳过，返回实际恢复数量。
    pub fn restore_chunk(&mut self, chunk: ChunkPos, records: &[EntityRecord]) -> usize {
        let mut restored = 0;
        for record in records {
            if chunk_of(record.position()) != chunk {
                continue;
            }
            let creature = match record.to_creature() {
                Some(creature) => creature,
                None => continue,
            };
            if self.contains_id(creature.id()) {
                continue;
            }
            self.next_id = self.next_id.max(creature.id().raw().saturating_add(1));
            self.entities.push(creature);
            restored += 1;
        }
        restored
    }

    /// 是否存在指定 ID。
    pub fn contains_id(&self, id: EntityId) -> bool {
        self.entities.iter().any(|creature| creature.id() == id)
    }

    /// 下一个将被分配的 ID。
    #[allow(dead_code)]
    pub fn next_id(&self) -> EntityId {
        EntityId::new(self.next_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::entity::EntityDimensions;
    use crate::entity::entity_save::EntityRecord;

    fn spawn_pig(manager: &mut EntityManager, position: Vec3) -> EntityId {
        manager.spawn(EntityTypeId::PIG, position, 0.0).unwrap()
    }

    fn record_at(id: u64, position: Vec3, health: u32, state: u8) -> EntityRecord {
        EntityRecord {
            id,
            kind: EntityTypeId::PIG.raw(),
            x: position.x,
            y: position.y,
            z: position.z,
            vx: 0.0,
            vy: 0.0,
            vz: 0.0,
            health,
            state,
            state_timer: 1.0,
            anchor_x: position.x,
            anchor_z: position.z,
        }
    }

    #[test]
    fn ids_are_unique_and_monotonic() {
        let mut manager = EntityManager::new();
        let first = spawn_pig(&mut manager, Vec3::new(0.0, 10.0, 0.0));
        let second = spawn_pig(&mut manager, Vec3::new(2.0, 10.0, 0.0));
        assert_ne!(first, second);
        assert!(second.raw() > first.raw());
        assert_eq!(manager.len(), 2);
    }

    #[test]
    fn invalid_spawns_are_rejected() {
        let mut manager = EntityManager::new();
        assert!(manager
            .spawn(EntityTypeId::new(999), Vec3::ZERO, 0.0)
            .is_none());
        assert!(manager
            .spawn(EntityTypeId::PIG, Vec3::new(f32::NAN, 0.0, 0.0), 0.0)
            .is_none());
        assert!(manager.is_empty());
    }

    #[test]
    fn damage_kills_once_and_marks_brain_dead() {
        let mut manager = EntityManager::new();
        let id = spawn_pig(&mut manager, Vec3::new(0.0, 10.0, 0.0));
        assert_eq!(
            manager.apply_damage(id, 4, Vec3::ZERO, 3.0),
            DamageResult::Applied { remaining: 6 }
        );
        assert_eq!(
            manager.apply_damage(id, 20, Vec3::ZERO, 3.0),
            DamageResult::Killed
        );
        assert_eq!(
            manager.apply_damage(id, 20, Vec3::ZERO, 3.0),
            DamageResult::Ignored,
            "重复伤害不应再次触发击杀"
        );
        assert!(manager.get(id).unwrap().entity.is_dead());
        assert_eq!(
            manager.get(id).unwrap().brain.state(),
            super::super::ai::AiState::Dead
        );
    }

    #[test]
    fn damage_frightens_surviving_creature() {
        let mut manager = EntityManager::new();
        let id = spawn_pig(&mut manager, Vec3::new(0.0, 10.0, 0.0));
        manager.apply_damage(id, 2, Vec3::new(-1.0, 10.0, 0.0), 3.0);
        assert_eq!(
            manager.get(id).unwrap().brain.state(),
            super::super::ai::AiState::Fleeing
        );
    }

    #[test]
    fn deaths_are_cleaned_up_after_linger() {
        let mut manager = EntityManager::new();
        let id = spawn_pig(&mut manager, Vec3::ZERO);
        manager.apply_damage(id, 99, Vec3::ZERO, 3.0);
        assert_eq!(manager.update_deaths(0.3), 0);
        assert_eq!(manager.update_deaths(0.4), 1);
        assert!(manager.is_empty());
    }

    #[test]
    fn chunk_ownership_updates_with_position() {
        let mut manager = EntityManager::new();
        let id = spawn_pig(&mut manager, Vec3::new(0.5, 10.0, 0.5));
        assert_eq!(
            manager.get(id).unwrap().entity.chunk(),
            ChunkPos::new(0, 0, 0)
        );
        manager
            .get_mut(id)
            .unwrap()
            .entity
            .set_position(Vec3::new(17.0, 10.0, 0.5));
        assert_eq!(
            manager.get(id).unwrap().entity.chunk(),
            ChunkPos::new(1, 0, 0)
        );
        assert_eq!(manager.chunk_entity_count(ChunkPos::new(0, 0, 0)), 0);
        assert_eq!(manager.chunk_entity_count(ChunkPos::new(1, 0, 0)), 1);
        assert!(manager
            .entities_in_chunk(ChunkPos::new(0, 0, 0))
            .next()
            .is_none());
    }

    #[test]
    fn unloading_takes_records_and_reload_restores_them() {
        let mut manager = EntityManager::new();
        let position = Vec3::new(1.5, 11.0, 2.5);
        let id = spawn_pig(&mut manager, position);
        let records = manager.take_chunk_entities(ChunkPos::new(0, 0, 0));
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, id.raw());
        assert!(manager.is_empty());

        assert_eq!(manager.restore_chunk(ChunkPos::new(0, 0, 0), &records), 1);
        assert_eq!(manager.get(id).unwrap().entity.position(), position);

        // 再次恢复同一记录不会产生重复实体。
        assert_eq!(manager.restore_chunk(ChunkPos::new(0, 0, 0), &records), 0);
        assert_eq!(manager.len(), 1);
    }

    #[test]
    fn restore_bumps_next_id() {
        let mut manager = EntityManager::new();
        let records = vec![record_at(40, Vec3::new(1.0, 10.0, 1.0), 8, 1)];
        assert_eq!(manager.restore_chunk(ChunkPos::new(0, 0, 0), &records), 1);
        assert_eq!(manager.next_id(), EntityId::new(41));
        let new_id = spawn_pig(&mut manager, Vec3::new(2.0, 10.0, 2.0));
        assert_eq!(new_id, EntityId::new(41));
    }

    #[test]
    fn restore_skips_mismatched_chunk_and_dead_records() {
        let mut manager = EntityManager::new();
        let records = vec![
            record_at(1, Vec3::new(1.0, 10.0, 1.0), 8, 1),
            record_at(2, Vec3::new(40.0, 10.0, 1.0), 8, 1),
            record_at(3, Vec3::new(2.0, 10.0, 2.0), 0, 1),
            record_at(
                4,
                Vec3::new(3.0, 10.0, 3.0),
                8,
                super::super::ai::AiState::Dead.code(),
            ),
        ];
        assert_eq!(manager.restore_chunk(ChunkPos::new(0, 0, 0), &records), 1);
        assert!(manager.contains_id(EntityId::new(1)));
    }

    #[test]
    fn dead_creatures_are_not_snapshotted() {
        let mut manager = EntityManager::new();
        let id = spawn_pig(&mut manager, Vec3::new(1.0, 10.0, 1.0));
        manager.apply_damage(id, 99, Vec3::ZERO, 3.0);
        assert!(manager.snapshot_chunk(ChunkPos::new(0, 0, 0)).is_empty());
        assert_eq!(manager.len(), 1, "死亡实体仍在等待清理");
    }

    #[test]
    fn column_count_ignores_vertical_chunk_split() {
        let mut manager = EntityManager::new();
        spawn_pig(&mut manager, Vec3::new(1.0, 10.0, 1.0));
        spawn_pig(&mut manager, Vec3::new(1.0, 34.0, 1.0));
        assert_eq!(manager.column_entity_count(ChunkPos::new(0, 0, 0)), 2);
        assert_eq!(manager.column_entity_count(ChunkPos::new(1, 0, 0)), 0);
    }

    #[test]
    fn dimensions_come_from_registry() {
        let mut manager = EntityManager::new();
        let id = manager.spawn(EntityTypeId::COW, Vec3::ZERO, 0.0).unwrap();
        let dimensions = manager.get(id).unwrap().entity.dimensions();
        assert_eq!(dimensions, EntityDimensions::new(0.9, 1.4));
    }
}
