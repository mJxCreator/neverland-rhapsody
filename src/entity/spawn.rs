//! 自然生成：在玩家周围已加载且地形合适的区块中生成生物。
//!
//! 生成器只决定“在哪里生成、生成什么”，实际创建由 [`super::entity_manager`] 完成；
//! 已经生成过的区块由存档层记录，重新加载后不会重复自然生成。

use glam::Vec3;

use super::creature_registry;
use super::entity::chunk_of;
use super::entity_collision;
use super::entity_manager::EntityManager;
use super::entity_registry::EntityDefinition;
use crate::player::collision::Aabb;
use crate::world::{Block, ChunkPos, World, CHUNK_SIZE, WORLD_HEIGHT};

/// 自然生成参数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnConfig {
    /// 玩家周围允许生成的水平半径（格）。
    pub radius: i32,
    /// 全局存活数量上限。
    pub global_limit: usize,
    /// 单个水平区块列的存活数量上限。
    pub chunk_limit: usize,
    /// 两次生成检查之间的冷却时间。
    pub cooldown_seconds: f32,
    /// 每次检查的最大尝试次数。
    pub max_attempts: u32,
    /// 与玩家的最小水平距离。
    pub min_player_distance: f32,
}

impl Default for SpawnConfig {
    fn default() -> Self {
        Self {
            radius: 32,
            global_limit: 20,
            chunk_limit: 5,
            cooldown_seconds: 2.0,
            max_attempts: 8,
            min_player_distance: 6.0,
        }
    }
}

/// 自然生成器。
pub struct Spawner {
    config: SpawnConfig,
    cooldown: f32,
    rng: u32,
}

impl Spawner {
    /// 创建生成器；种子决定随机序列，便于测试复现。
    pub fn new(config: SpawnConfig, seed: u32) -> Self {
        Self {
            config,
            cooldown: 0.0,
            rng: seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223),
        }
    }

    /// 推进生成逻辑；返回本次新标记为“已生成”的水平区块列。
    pub fn update(
        &mut self,
        manager: &mut EntityManager,
        world: &World,
        player_position: Vec3,
        player_aabb: Aabb,
        is_populated: &dyn Fn(ChunkPos) -> bool,
        delta: f32,
    ) -> Vec<ChunkPos> {
        let mut populated_now = Vec::new();
        if !delta.is_finite() || delta <= 0.0 {
            return populated_now;
        }
        self.cooldown -= delta;
        if self.cooldown > 0.0 {
            return populated_now;
        }
        self.cooldown = self.config.cooldown_seconds;
        if manager.alive_count() >= self.config.global_limit {
            return populated_now;
        }

        let player_column = check_column(chunk_of(player_position));
        let chunk_radius = ((self.config.radius as f32 / CHUNK_SIZE as f32).ceil() as i32).max(0);

        for _ in 0..self.config.max_attempts {
            if manager.alive_count() >= self.config.global_limit {
                break;
            }
            let column = self.random_column(player_column, chunk_radius);
            if populated_now.contains(&column) || is_populated(column) {
                continue;
            }
            if manager.column_entity_count(column) >= self.config.chunk_limit {
                continue;
            }
            let spawn = self
                .try_spawn(manager, world, column, player_position, player_aabb)
                .is_some();
            if spawn {
                populated_now.push(column);
            }
        }
        populated_now
    }

    fn random_column(&mut self, center: ChunkPos, radius: i32) -> ChunkPos {
        let dx = self.random_int(-radius, radius);
        let dz = self.random_int(-radius, radius);
        ChunkPos::new(center.x + dx, 0, center.z + dz)
    }

    fn try_spawn(
        &mut self,
        manager: &mut EntityManager,
        world: &World,
        column: ChunkPos,
        player_position: Vec3,
        player_aabb: Aabb,
    ) -> Option<super::entity::EntityId> {
        let x = column.x * CHUNK_SIZE + self.random_int(0, CHUNK_SIZE - 1);
        let z = column.z * CHUNK_SIZE + self.random_int(0, CHUNK_SIZE - 1);
        let surface_y = self.find_surface(world, x, z)?;
        let feet = Vec3::new(x as f32 + 0.5, surface_y as f32 + 1.0, z as f32 + 0.5);

        let horizontal =
            Vec3::new(feet.x - player_position.x, 0.0, feet.z - player_position.z).length();
        if horizontal < self.config.min_player_distance {
            return None;
        }

        let definition = self.pick_definition()?;
        let entity_aabb = Aabb::from_feet(feet, definition.width, definition.height);
        if entity_collision::overlaps(entity_aabb, player_aabb) {
            return None;
        }
        if !self.has_clear_space(world, feet, definition) {
            return None;
        }
        let yaw = self.random_unit() * std::f32::consts::TAU;
        manager.spawn(definition.kind, feet, yaw)
    }

    /// 自上而下寻找最高实体方块；要求顶面为草方块且上方有足够空间。
    fn find_surface(&self, world: &World, x: i32, z: i32) -> Option<i32> {
        for y in (1..WORLD_HEIGHT).rev() {
            if !world.contains_chunk(ChunkPos::from_world(x, y, z)) {
                // 未加载的垂直层级直接跳过，避免把上层未加载当成“没有地表”。
                continue;
            }
            let block = world.block_at(x, y, z);
            if block.is_solid() {
                if block != Block::Grass {
                    return None;
                }
                return Some(y);
            }
        }
        None
    }

    fn has_clear_space(&self, world: &World, feet: Vec3, definition: &EntityDefinition) -> bool {
        let x = feet.x.floor() as i32;
        let z = feet.z.floor() as i32;
        let start_y = feet.y.floor() as i32;
        let blocks = definition.height.ceil() as i32;
        for offset in 0..blocks {
            let y = start_y + offset;
            if !world.contains_chunk(ChunkPos::from_world(x, y, z))
                || world.block_at(x, y, z) != Block::Air
            {
                return false;
            }
        }
        true
    }

    fn pick_definition(&mut self) -> Option<&'static EntityDefinition> {
        let total: u32 = creature_registry::spawnable()
            .map(|definition| definition.spawn_weight)
            .sum();
        if total == 0 {
            return None;
        }
        let mut roll = self.random_int(0, total as i32 - 1) as u32;
        for definition in creature_registry::spawnable() {
            if roll < definition.spawn_weight {
                return Some(definition);
            }
            roll -= definition.spawn_weight;
        }
        None
    }

    fn random_int(&mut self, min: i32, max: i32) -> i32 {
        if max <= min {
            return min;
        }
        min + (self.next_u32() % ((max - min + 1) as u32)) as i32
    }

    fn random_unit(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / ((1u32 << 24) as f32)
    }

    fn next_u32(&mut self) -> u32 {
        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.rng
    }
}

fn check_column(chunk: ChunkPos) -> ChunkPos {
    ChunkPos::new(chunk.x, 0, chunk.z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::entity::EntityTypeId;
    use crate::entity::entity_manager::EntityManager;
    use crate::world::{Chunk, DEFAULT_SEED};

    fn grass_world() -> World {
        let mut world = World::new(DEFAULT_SEED);
        assert!(world.insert_chunk(Chunk::new(ChunkPos::new(0, 0, 0))));
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                world.set_block(x, 9, z, Block::Grass);
            }
        }
        world
    }

    fn test_config() -> SpawnConfig {
        SpawnConfig {
            radius: 0,
            global_limit: 20,
            chunk_limit: 5,
            cooldown_seconds: 0.0,
            max_attempts: 8,
            min_player_distance: 4.0,
        }
    }

    fn near_player() -> (Vec3, Aabb) {
        let position = Vec3::new(8.0, 10.0, 8.0);
        (position, Aabb::from_feet(position, 0.6, 1.8))
    }

    #[test]
    fn spawns_on_grass_when_conditions_are_met() {
        let world = grass_world();
        let mut manager = EntityManager::new();
        let (player_pos, player_box) = near_player();
        let mut spawner = Spawner::new(test_config(), 7);
        let populated = spawner.update(
            &mut manager,
            &world,
            player_pos,
            player_box,
            &|_| false,
            1.0,
        );
        assert_eq!(manager.len(), 1, "应生成一只生物");
        assert_eq!(populated.len(), 1);
        let creature = &manager.creatures()[0];
        assert!((creature.entity.position().y - 10.0).abs() < 1e-5);
        assert!(matches!(
            creature.kind(),
            EntityTypeId::PIG | EntityTypeId::COW
        ));
    }

    #[test]
    fn already_populated_column_blocks_spawning() {
        let world = grass_world();
        let mut manager = EntityManager::new();
        let (player_pos, player_box) = near_player();
        let mut spawner = Spawner::new(test_config(), 7);
        assert_eq!(
            spawner
                .update(&mut manager, &world, player_pos, player_box, &|_| true, 1.0)
                .len(),
            0
        );
        assert!(manager.is_empty());
    }

    #[test]
    fn global_limit_is_respected() {
        let world = grass_world();
        let mut manager = EntityManager::new();
        manager.spawn(EntityTypeId::PIG, Vec3::new(1.0, 10.0, 1.0), 0.0);
        let (player_pos, player_box) = near_player();
        let mut config = test_config();
        config.global_limit = 1;
        let mut spawner = Spawner::new(config, 7);
        assert_eq!(
            spawner
                .update(
                    &mut manager,
                    &world,
                    player_pos,
                    player_box,
                    &|_| false,
                    1.0
                )
                .len(),
            0
        );
        assert_eq!(manager.len(), 1);
    }

    #[test]
    fn chunk_limit_is_respected() {
        let world = grass_world();
        let mut manager = EntityManager::new();
        manager.spawn(EntityTypeId::PIG, Vec3::new(1.0, 10.0, 1.0), 0.0);
        let (player_pos, player_box) = near_player();
        let mut config = test_config();
        config.chunk_limit = 1;
        let mut spawner = Spawner::new(config, 7);
        assert_eq!(
            spawner
                .update(
                    &mut manager,
                    &world,
                    player_pos,
                    player_box,
                    &|_| false,
                    1.0
                )
                .len(),
            0
        );
    }

    #[test]
    fn player_collision_box_blocks_spawning() {
        let world = grass_world();
        let mut manager = EntityManager::new();
        let position = Vec3::new(8.0, 10.0, 8.0);
        let player_box = Aabb::from_feet(position, CHUNK_SIZE as f32 * 2.0, 2.0);
        let mut spawner = Spawner::new(test_config(), 7);
        assert_eq!(
            spawner
                .update(&mut manager, &world, position, player_box, &|_| false, 1.0)
                .len(),
            0
        );
        assert!(manager.is_empty());
    }

    #[test]
    fn non_grass_surface_blocks_spawning() {
        let mut world = World::new(DEFAULT_SEED);
        assert!(world.insert_chunk(Chunk::new(ChunkPos::new(0, 0, 0))));
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                world.set_block(x, 9, z, Block::Stone);
            }
        }
        let mut manager = EntityManager::new();
        let (player_pos, player_box) = near_player();
        let mut spawner = Spawner::new(test_config(), 7);
        assert_eq!(
            spawner
                .update(
                    &mut manager,
                    &world,
                    player_pos,
                    player_box,
                    &|_| false,
                    1.0
                )
                .len(),
            0
        );
    }

    #[test]
    fn cooldown_prevents_repeated_checks() {
        let world = grass_world();
        let mut manager = EntityManager::new();
        let (player_pos, player_box) = near_player();
        let mut config = test_config();
        config.cooldown_seconds = 2.0;
        let mut spawner = Spawner::new(config, 7);
        let first = spawner.update(
            &mut manager,
            &world,
            player_pos,
            player_box,
            &|_| false,
            0.1,
        );
        assert_eq!(first.len(), 1);
        let second = spawner.update(&mut manager, &world, player_pos, player_box, &|_| true, 0.5);
        assert_eq!(second.len(), 0, "冷却期间不应再次尝试生成");
    }

    #[test]
    fn no_spawn_without_loaded_chunks() {
        let world = World::new(DEFAULT_SEED);
        let mut manager = EntityManager::new();
        let (player_pos, player_box) = near_player();
        let mut spawner = Spawner::new(test_config(), 7);
        assert_eq!(
            spawner
                .update(
                    &mut manager,
                    &world,
                    player_pos,
                    player_box,
                    &|_| false,
                    1.0
                )
                .len(),
            0
        );
    }
}
