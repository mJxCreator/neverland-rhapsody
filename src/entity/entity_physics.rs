//! 实体物理：重力、水平移动控制、方块碰撞与固定步长推进。
//!
//! 与玩家物理共用 [`crate::player::collision::move_box`]，但使用实体自身的
//! 尺寸、速度与 AI 期望方向；物理不接受输入，只读取 AI 输出的朝向。

use glam::Vec3;

use super::creature::Creature;
use super::entity_collision;
use super::entity_registry;
use crate::player::collision::{move_box, Aabb};
use crate::player::physics::{FIXED_TIMESTEP, MAX_STEPS_PER_FRAME};
use crate::player::PhysicsWorld;
use crate::world::World;

/// 一次实体物理推进的结果。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EntityPhysicsUpdate {
    /// 本次实际执行的固定步数。
    pub steps: u32,
    /// 渲染插值系数，取值 0–1。
    pub alpha: f32,
}

/// 实体物理参数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityPhysicsConfig {
    /// 重力加速度（格/秒²）。
    pub gravity: f32,
    /// 有移动意图时的水平加速度（格/秒²）。
    pub horizontal_acceleration: f32,
    /// 无移动意图时的水平减速度（格/秒²）。
    pub horizontal_friction: f32,
}

impl Default for EntityPhysicsConfig {
    fn default() -> Self {
        Self {
            gravity: 24.0,
            horizontal_acceleration: 8.0,
            horizontal_friction: 12.0,
        }
    }
}

/// 固定步长实体物理系统。
pub struct EntityPhysicsSystem {
    config: EntityPhysicsConfig,
    accumulator: f32,
}

impl EntityPhysicsSystem {
    /// 创建物理系统。
    pub fn new(config: EntityPhysicsConfig) -> Self {
        Self {
            config,
            accumulator: 0.0,
        }
    }

    /// 当前渲染插值系数。
    pub fn alpha(&self) -> f32 {
        (self.accumulator / FIXED_TIMESTEP).clamp(0.0, 1.0)
    }

    /// 按固定步长推进所有生物；单帧最多补算 [`MAX_STEPS_PER_FRAME`] 次。
    pub fn update(
        &mut self,
        creatures: &mut [Creature],
        world: &World,
        player_aabb: Option<Aabb>,
        frame_delta: f32,
    ) -> EntityPhysicsUpdate {
        if !frame_delta.is_finite() || frame_delta <= 0.0 {
            return EntityPhysicsUpdate {
                steps: 0,
                alpha: self.alpha(),
            };
        }
        let max_frame = FIXED_TIMESTEP * MAX_STEPS_PER_FRAME as f32;
        self.accumulator += frame_delta.min(max_frame);

        let physics_world = PhysicsWorld::new(world);
        let is_solid = |x: i32, y: i32, z: i32| physics_world.is_solid(x, y, z);

        let mut steps = 0;
        while self.accumulator >= FIXED_TIMESTEP && steps < MAX_STEPS_PER_FRAME {
            for creature in creatures.iter_mut() {
                creature.entity.snapshot_position();
                if creature.is_alive() {
                    step_creature(creature, &self.config, FIXED_TIMESTEP, &is_solid);
                }
            }
            if let Some(player_box) = player_aabb {
                for creature in creatures.iter_mut() {
                    entity_collision::push_creature_out(
                        &mut creature.entity,
                        player_box,
                        &is_solid,
                    );
                }
            }
            self.accumulator -= FIXED_TIMESTEP;
            steps += 1;
        }

        EntityPhysicsUpdate {
            steps,
            alpha: self.alpha(),
        }
    }
}

/// 推进单个生物的固定物理步。
fn step_creature(
    creature: &mut Creature,
    config: &EntityPhysicsConfig,
    delta: f32,
    is_solid: &dyn Fn(i32, i32, i32) -> bool,
) {
    let definition = match entity_registry::definition(creature.kind()) {
        Some(definition) => definition,
        None => return,
    };
    let steer = creature.brain.steer();
    let has_intent = steer.length_squared() > 1e-6;
    let speed = definition.move_speed;

    let mut velocity = creature.entity.velocity();
    let accel = if has_intent {
        config.horizontal_acceleration
    } else {
        config.horizontal_friction
    } * delta;
    velocity.x = approach(velocity.x, steer.x * speed, accel);
    velocity.z = approach(velocity.z, steer.z * speed, accel);
    velocity.y -= config.gravity * delta;

    let mut position = creature.entity.position();
    let flags = move_box(
        &mut position,
        &mut velocity,
        definition.width,
        definition.height,
        delta,
        is_solid,
    );

    creature.entity.set_velocity(velocity);
    creature.entity.set_position(position);
    creature.entity.set_grounded(flags.ground);
    if flags.horizontal {
        creature.brain.note_obstacle();
    }

    let horizontal = Vec3::new(velocity.x, 0.0, velocity.z);
    if horizontal.length_squared() > 0.05 * 0.05 {
        // 模型本地 +Z 为正面，yaw 取速度方向与 +Z 的夹角。
        creature.entity.set_yaw(velocity.x.atan2(velocity.z));
    }
}

fn approach(current: f32, target: f32, max_delta: f32) -> f32 {
    if (target - current).abs() <= max_delta {
        target
    } else if target > current {
        current + max_delta
    } else {
        current - max_delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::entity::{Entity, EntityDimensions, EntityId, EntityTypeId};
    use crate::world::{Block, Chunk, ChunkPos, DEFAULT_SEED};

    fn flat_world() -> World {
        let mut world = World::new(DEFAULT_SEED);
        for cx in 0..=1 {
            assert!(world.insert_chunk(Chunk::new(ChunkPos::new(cx, 0, 0))));
        }
        for x in 0..32 {
            for z in 0..16 {
                world.set_block(x, 9, z, Block::Stone);
            }
        }
        world
    }

    fn creature_at(x: f32) -> Creature {
        Creature::new(Entity::new(
            EntityId::new(1),
            EntityTypeId::PIG,
            Vec3::new(x, 12.0, 8.0),
            0.0,
            EntityDimensions::new(0.9, 0.9),
            10,
        ))
    }

    #[test]
    fn entity_falls_and_lands_on_solid_block() {
        let world = flat_world();
        let mut creatures = vec![creature_at(8.0)];
        let mut physics = EntityPhysicsSystem::new(EntityPhysicsConfig::default());
        for _ in 0..120 {
            physics.update(&mut creatures, &world, None, FIXED_TIMESTEP);
        }
        let y = creatures[0].entity.position().y;
        assert!((y - 10.0).abs() < 1e-3, "应停在石块顶部，实际 y={y}");
        assert!(creatures[0].entity.is_grounded());
    }

    #[test]
    fn wandering_entity_moves_horizontally() {
        let world = flat_world();
        let mut creatures = vec![creature_at(8.0)];
        creatures[0].entity.set_position(Vec3::new(8.0, 10.0, 8.0));
        creatures[0].entity.set_grounded(true);
        creatures[0]
            .brain
            .set_state(crate::entity::ai::AiState::Wandering, 3.0);
        let mut physics = EntityPhysicsSystem::new(EntityPhysicsConfig::default());
        let start = creatures[0].entity.position();
        for _ in 0..30 {
            physics.update(&mut creatures, &world, None, FIXED_TIMESTEP);
        }
        let end = creatures[0].entity.position();
        assert!(
            (end.x - start.x).abs() > 0.01 || (end.z - start.z).abs() > 0.01,
            "游荡状态应产生水平位移"
        );
    }

    #[test]
    fn wall_blocks_horizontal_motion() {
        let mut world = flat_world();
        for y in 10..=12 {
            world.set_block(10, y, 8, Block::Stone);
        }
        let mut creatures = vec![creature_at(9.0)];
        creatures[0].entity.set_position(Vec3::new(9.0, 10.0, 8.0));
        creatures[0].entity.set_grounded(true);
        // 以逃离状态强制朝 +X 移动。
        creatures[0]
            .brain
            .flee(Vec3::new(9.0, 10.0, 8.0), Vec3::new(8.0, 10.0, 8.0), 3.0);
        let mut physics = EntityPhysicsSystem::new(EntityPhysicsConfig::default());
        for _ in 0..120 {
            physics.update(&mut creatures, &world, None, FIXED_TIMESTEP);
        }
        assert!(
            creatures[0].entity.position().x < 9.6,
            "墙体应阻止实体穿过，实际 x={}",
            creatures[0].entity.position().x
        );
    }

    #[test]
    fn negative_coordinate_chunks_are_supported() {
        let mut world = World::new(DEFAULT_SEED);
        for cx in -1..=0 {
            for cz in -1..=0 {
                assert!(world.insert_chunk(Chunk::new(ChunkPos::new(cx, 0, cz))));
            }
        }
        for x in -16..16 {
            for z in -16..0 {
                world.set_block(x, 9, z, Block::Stone);
            }
        }
        let mut creatures = vec![Creature::new(Entity::new(
            EntityId::new(1),
            EntityTypeId::PIG,
            Vec3::new(-2.0, 12.0, -8.0),
            0.0,
            EntityDimensions::new(0.9, 0.9),
            10,
        ))];
        let mut physics = EntityPhysicsSystem::new(EntityPhysicsConfig::default());
        for _ in 0..120 {
            physics.update(&mut creatures, &world, None, FIXED_TIMESTEP);
        }
        let position = creatures[0].entity.position();
        assert!((position.y - 10.0).abs() < 1e-3);
        assert_eq!(creatures[0].entity.chunk(), ChunkPos::new(-1, 0, -1));
    }

    #[test]
    fn frame_delta_is_clamped_to_max_steps() {
        let world = flat_world();
        let mut creatures = vec![creature_at(8.0)];
        let mut physics = EntityPhysicsSystem::new(EntityPhysicsConfig::default());
        let update = physics.update(&mut creatures, &world, None, 5.0);
        assert_eq!(update.steps, MAX_STEPS_PER_FRAME);
        assert!(update.alpha < 1.0);
    }

    #[test]
    fn zero_delta_is_ignored() {
        let world = flat_world();
        let mut creatures = vec![creature_at(8.0)];
        let mut physics = EntityPhysicsSystem::new(EntityPhysicsConfig::default());
        let update = physics.update(&mut creatures, &world, None, 0.0);
        assert_eq!(update.steps, 0);
    }

    #[test]
    fn creature_leaves_player_collision_box() {
        let world = flat_world();
        let mut creatures = vec![creature_at(8.0)];
        creatures[0].entity.set_position(Vec3::new(8.0, 10.0, 8.0));
        let player_box = Aabb::from_feet(Vec3::new(8.0, 10.0, 8.0), 0.6, 1.8);
        let mut physics = EntityPhysicsSystem::new(EntityPhysicsConfig::default());
        physics.update(&mut creatures, &world, Some(player_box), FIXED_TIMESTEP);
        assert!(
            !entity_collision::overlaps(creatures[0].entity.aabb(), player_box),
            "实体应被推出玩家碰撞箱"
        );
    }
}
