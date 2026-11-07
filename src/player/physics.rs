//! 物理系统：固定时间步长内的重力、跳跃、地面/空中移动与碰撞修正。

use glam::Vec3;

use super::collision::move_box;
use super::controller::MovementIntent;
use super::physics_world::PhysicsWorld;
use super::player::{Player, PlayerDimensions};

/// 推荐物理更新频率为 60 Hz。
pub const FIXED_TIMESTEP: f32 = 1.0 / 60.0;
/// 单帧最多执行的物理补算次数，避免低帧率时无限追赶。
pub const MAX_STEPS_PER_FRAME: u32 = 5;

/// 集中管理的玩家物理参数；允许后续调整，但物理逻辑不随测试改动。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicsConfig {
    pub dimensions: PlayerDimensions,
    /// 地面行走速度（格/秒）。
    pub walk_speed: f32,
    /// 重力加速度（格/秒²）。
    pub gravity: f32,
    /// 跳跃初速度（格/秒）。
    pub jump_speed: f32,
    /// 地面水平加速度。
    pub ground_acceleration: f32,
    /// 地面摩擦减速度。
    pub ground_friction: f32,
    /// 空中水平加速度，弱于地面。
    pub air_acceleration: f32,
    /// 空中水平阻力系数（每秒衰减比例）。
    pub air_resistance: f32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            dimensions: PlayerDimensions {
                width: 0.6,
                height: 1.8,
                eye_height: 1.62,
            },
            walk_speed: 4.3,
            gravity: 24.0,
            jump_speed: 8.0,
            ground_acceleration: 40.0,
            ground_friction: 30.0,
            air_acceleration: 12.0,
            air_resistance: 0.25,
        }
    }
}

impl PhysicsConfig {
    /// 把非法参数替换为默认值或合理下界，避免 NaN/无穷进入物理计算。
    fn normalized(self) -> Self {
        let defaults = Self::default();
        let width = positive_or(self.dimensions.width, defaults.dimensions.width);
        let height = positive_or(self.dimensions.height, defaults.dimensions.height);
        let eye_height = if self.dimensions.eye_height.is_finite() {
            self.dimensions.eye_height.clamp(0.0, height)
        } else {
            defaults.dimensions.eye_height.min(height)
        };

        Self {
            dimensions: PlayerDimensions {
                width,
                height,
                eye_height,
            },
            walk_speed: non_negative_or(self.walk_speed, defaults.walk_speed),
            gravity: positive_or(self.gravity, defaults.gravity),
            jump_speed: non_negative_or(self.jump_speed, defaults.jump_speed),
            ground_acceleration: non_negative_or(
                self.ground_acceleration,
                defaults.ground_acceleration,
            ),
            ground_friction: non_negative_or(self.ground_friction, defaults.ground_friction),
            air_acceleration: non_negative_or(self.air_acceleration, defaults.air_acceleration),
            air_resistance: non_negative_or(self.air_resistance, defaults.air_resistance),
        }
    }
}

fn positive_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

fn non_negative_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        fallback
    }
}

/// 一帧物理推进的结果。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicsUpdate {
    /// 本帧实际执行的固定物理步数。
    pub steps: u32,
    /// 摄像机插值系数，范围 0..1。
    pub alpha: f32,
    /// 本帧发生落地时的实际下落距离（格）；未落地为 `None`。
    pub landing_fall: Option<f32>,
}

/// 固定时间步长物理系统。
pub struct PhysicsSystem {
    config: PhysicsConfig,
    /// 尚未消耗的物理时间。
    accumulator: f32,
}

impl PhysicsSystem {
    pub fn new(config: PhysicsConfig) -> Self {
        Self {
            config: config.normalized(),
            accumulator: 0.0,
        }
    }

    /// 集中管理后的物理参数。
    pub fn config(&self) -> &PhysicsConfig {
        &self.config
    }

    /// 当前摄像机插值系数。
    pub fn interpolation_alpha(&self) -> f32 {
        (self.accumulator / FIXED_TIMESTEP).clamp(0.0, 1.0)
    }

    /// 按固定步长推进物理；单帧最多补算 [`MAX_STEPS_PER_FRAME`] 次。
    ///
    /// 实际帧时间先裁剪到可补算范围，避免严重卡顿后累积无限追赶。
    pub fn update(
        &mut self,
        player: &mut Player,
        intent: &MovementIntent,
        world: &PhysicsWorld,
        frame_delta: f32,
    ) -> PhysicsUpdate {
        let max_frame = FIXED_TIMESTEP * MAX_STEPS_PER_FRAME as f32;
        let delta = if frame_delta.is_finite() {
            frame_delta.clamp(0.0, max_frame)
        } else {
            0.0
        };
        self.accumulator += delta;

        let mut steps = 0;
        let mut landing_fall: Option<f32> = None;
        while self.accumulator >= FIXED_TIMESTEP && steps < MAX_STEPS_PER_FRAME {
            if steps == 0 {
                player.snapshot_previous_position();
            }
            if let Some(distance) = step_player(player, intent, &self.config, world, FIXED_TIMESTEP)
            {
                landing_fall = Some(landing_fall.map_or(distance, |current| current.max(distance)));
            }
            self.accumulator -= FIXED_TIMESTEP;
            steps += 1;
        }

        PhysicsUpdate {
            steps,
            alpha: self.interpolation_alpha(),
            landing_fall,
        }
    }

    /// 清空未消费的物理时间；传送或重生后调用，避免旧时间片继续推进。
    pub fn reset(&mut self) {
        self.accumulator = 0.0;
    }
}

/// 单个固定物理步：水平加速/摩擦、跳跃、重力与碰撞。
fn step_player(
    player: &mut Player,
    intent: &MovementIntent,
    config: &PhysicsConfig,
    world: &PhysicsWorld,
    delta_secs: f32,
) -> Option<f32> {
    let was_grounded = player.is_grounded();
    let previous_position = player.position();
    let previous_velocity = player.velocity();
    if !was_grounded {
        player.observe_air_position(previous_position.y);
    }

    // 水平方向移动意图，斜向输入归一化，避免对角线更快。
    let forward_axis = finite_axis(intent.forward);
    let right_axis = finite_axis(intent.right);
    let mut wish =
        player.horizontal_forward() * forward_axis + player.horizontal_right() * right_axis;
    let wish_length = wish.length();
    if wish_length > 1.0 {
        wish /= wish_length;
    }
    let has_input = wish.length_squared() > 0.0;

    let velocity = player.velocity();
    let mut horizontal = Vec3::new(velocity.x, 0.0, velocity.z);
    if has_input {
        let target = wish * config.walk_speed;
        let rate = if was_grounded {
            config.ground_acceleration
        } else {
            config.air_acceleration
        };
        horizontal += (target - horizontal).clamp_length_max(rate * delta_secs);
    } else if was_grounded {
        let speed = horizontal.length();
        horizontal = if speed > 0.0 {
            let remaining = (speed - config.ground_friction * delta_secs).max(0.0);
            horizontal * (remaining / speed)
        } else {
            Vec3::ZERO
        };
    } else {
        // 空中无输入时只有很弱的水平阻力，不能抵消重力。
        horizontal *= (1.0 - config.air_resistance * delta_secs).max(0.0);
    }

    let jumped = intent.jump && was_grounded;
    let vertical = if jumped {
        config.jump_speed
    } else {
        velocity.y - config.gravity * delta_secs
    };

    let mut new_velocity = Vec3::new(horizontal.x, vertical, horizontal.z);
    let mut position = previous_position;
    let dimensions = player.dimensions();
    let flags = move_box(
        &mut position,
        &mut new_velocity,
        dimensions.width,
        dimensions.height,
        delta_secs,
        &|x, y, z| world.is_solid(x, y, z),
    );

    player.set_position(position);
    player.set_velocity(new_velocity);
    // 地面状态完全由碰撞结果决定。
    player.set_grounded(flags.ground);
    let acceleration = if delta_secs > 0.0 {
        (new_velocity - previous_velocity) / delta_secs
    } else {
        Vec3::ZERO
    };
    player.set_acceleration(acceleration);

    // 兜底：任何非有限值都回到本步开始前的位置，避免 NaN 扩散。
    if !player.is_finite() {
        player.reset_motion(previous_position);
        return None;
    }

    // 只在“真正从空中落到地面”的这一步记录下落距离；跳跃与超短腾空由伤害
    // 公式中的安全高度过滤。下落距离以本次腾空最高点为基准，而非逐帧高度差。
    let landing_fall = if flags.ground && !was_grounded {
        Some((player.fall_peak_y() - position.y).max(0.0))
    } else {
        None
    };
    if flags.ground {
        player.reset_fall_peak(position.y);
    } else {
        player.observe_air_position(position.y);
    }
    landing_fall
}

fn finite_axis(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Block, Chunk, ChunkPos, World, CHUNK_SIZE, DEFAULT_SEED};
    use glam::Vec3;

    const FLOOR_Y: i32 = 10;
    const FEET_Y: f32 = 11.0;

    fn build_world(positions: &[ChunkPos]) -> World {
        let mut world = World::new(DEFAULT_SEED);
        for &pos in positions {
            assert!(world.insert_chunk(Chunk::new(pos)));
        }
        world
    }

    fn add_floor(world: &mut World, positions: &[ChunkPos], y: i32) {
        for &pos in positions {
            if !((pos.y * CHUNK_SIZE)..((pos.y + 1) * CHUNK_SIZE)).contains(&y) {
                continue;
            }
            let base_x = pos.x * CHUNK_SIZE;
            let base_z = pos.z * CHUNK_SIZE;
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let _ = world.set_block(base_x + x, y, base_z + z, Block::Stone);
                }
            }
        }
    }

    fn flat_world(positions: &[ChunkPos]) -> World {
        let mut world = build_world(positions);
        add_floor(&mut world, positions, FLOOR_Y);
        world
    }

    fn player_at(x: f32, z: f32) -> Player {
        Player::new(Vec3::new(x, FEET_Y, z), PhysicsConfig::default().dimensions)
    }

    fn intent(forward: f32) -> MovementIntent {
        MovementIntent {
            forward,
            right: 0.0,
            jump: false,
        }
    }

    /// 面朝 +X 的意图，用于跨区块行走测试。
    fn facing_positive_x(player: &mut Player) {
        player.rotate_look(-std::f32::consts::FRAC_PI_2, 0.0);
    }

    fn simulate(
        system: &mut PhysicsSystem,
        player: &mut Player,
        world: &World,
        intent: &MovementIntent,
        steps: usize,
    ) {
        let physics_world = PhysicsWorld::new(world);
        for _ in 0..steps {
            system.update(player, intent, &physics_world, FIXED_TIMESTEP);
        }
    }

    #[test]
    fn falling_player_lands_without_jitter() {
        let positions = [ChunkPos::new(0, 0, 0), ChunkPos::new(0, 1, 0)];
        let world = flat_world(&positions);
        let mut player = Player::new(
            Vec3::new(8.0, 20.0, 8.0),
            PhysicsConfig::default().dimensions,
        );
        let mut system = PhysicsSystem::new(PhysicsConfig::default());
        let idle = intent(0.0);

        simulate(&mut system, &mut player, &world, &idle, 120);

        assert!(player.is_grounded());
        assert!(
            (player.position().y - (FEET_Y + crate::player::collision::AABB_EPSILON)).abs() < 1e-3
        );

        let settled = player.position().y;
        simulate(&mut system, &mut player, &world, &idle, 120);
        assert!(player.is_grounded());
        assert!(
            (player.position().y - settled).abs() < 1e-4,
            "落地后出现抖动"
        );
    }

    #[test]
    fn jump_reaches_a_stable_height() {
        let world = flat_world(&[ChunkPos::new(0, 0, 0)]);
        let mut player = player_at(8.0, 8.0);
        let mut system = PhysicsSystem::new(PhysicsConfig::default());
        let idle = intent(0.0);

        simulate(&mut system, &mut player, &world, &idle, 20);
        assert!(player.is_grounded());
        let start = player.position().y;

        let jump = MovementIntent {
            forward: 0.0,
            right: 0.0,
            jump: true,
        };
        let physics_world = PhysicsWorld::new(&world);
        let mut peak = start;
        system.update(&mut player, &jump, &physics_world, FIXED_TIMESTEP);
        for _ in 0..200 {
            system.update(&mut player, &idle, &physics_world, FIXED_TIMESTEP);
            peak = peak.max(player.position().y);
        }

        assert!(peak - start > 1.2, "跳跃高度过低: {}", peak - start);
        assert!(peak - start < 1.5, "跳跃高度过高: {}", peak - start);
        assert!(player.is_grounded(), "跳跃后应重新落地");
    }

    #[test]
    fn airborne_players_cannot_jump_again() {
        let positions = [ChunkPos::new(0, 0, 0), ChunkPos::new(0, 1, 0)];
        let world = flat_world(&positions);
        let mut player = Player::new(
            Vec3::new(8.0, 20.0, 8.0),
            PhysicsConfig::default().dimensions,
        );
        let mut system = PhysicsSystem::new(PhysicsConfig::default());
        let jump = MovementIntent {
            forward: 0.0,
            right: 0.0,
            jump: true,
        };
        let physics_world = PhysicsWorld::new(&world);

        for _ in 0..40 {
            system.update(&mut player, &jump, &physics_world, FIXED_TIMESTEP);
            assert!(player.position().y < 20.0, "空中不应被跳跃抬高");
            assert!(player.velocity().y <= 0.0);
        }
        assert!(!player.is_grounded());
    }

    #[test]
    fn removing_the_floor_makes_the_player_fall() {
        let mut world = flat_world(&[ChunkPos::new(0, 0, 0)]);
        let mut player = player_at(10.0, 8.0);
        let mut system = PhysicsSystem::new(PhysicsConfig::default());
        let idle = intent(0.0);
        simulate(&mut system, &mut player, &world, &idle, 10);
        assert!(player.is_grounded());

        // 挖掉 AABB 脚下的整片支撑。
        for x in 9..=10 {
            for z in 7..=8 {
                let _ = world.set_block(x, FLOOR_Y, z, Block::Air);
            }
        }
        simulate(&mut system, &mut player, &world, &idle, 40);

        assert!(!player.is_grounded());
        assert!(player.position().y < FEET_Y - 0.5, "破坏脚下后应下落");
    }

    #[test]
    fn walking_crosses_chunk_boundaries_without_sinking() {
        let positions = [ChunkPos::new(0, 0, 0), ChunkPos::new(1, 0, 0)];
        let world = flat_world(&positions);
        let mut player = player_at(8.0, 8.0);
        facing_positive_x(&mut player);
        let mut system = PhysicsSystem::new(PhysicsConfig::default());
        let idle = intent(0.0);
        simulate(&mut system, &mut player, &world, &idle, 10);

        simulate(&mut system, &mut player, &world, &intent(1.0), 220);

        assert!(player.position().x > 17.0, "玩家未能跨过区块边界");
        assert!((player.position().y - 11.0).abs() < 1e-3, "区块接缝处下陷");
        assert!(player.is_grounded());
    }

    #[test]
    fn negative_coordinates_collide_correctly() {
        let positions = [ChunkPos::new(-2, 0, 0), ChunkPos::new(-1, 0, 0)];
        let world = flat_world(&positions);
        let mut player = player_at(-20.0, 8.0);
        facing_positive_x(&mut player);
        let mut system = PhysicsSystem::new(PhysicsConfig::default());
        let idle = intent(0.0);
        simulate(&mut system, &mut player, &world, &idle, 10);

        simulate(&mut system, &mut player, &world, &intent(1.0), 220);

        assert!(player.position().x > -15.0, "负坐标移动异常");
        assert!((player.position().y - 11.0).abs() < 1e-3);
    }

    #[test]
    fn unloaded_chunks_block_the_player() {
        let positions = [ChunkPos::new(0, 0, 0)];
        let world = flat_world(&positions);
        let mut player = player_at(10.0, 8.0);
        facing_positive_x(&mut player);
        let mut system = PhysicsSystem::new(PhysicsConfig::default());
        let idle = intent(0.0);
        simulate(&mut system, &mut player, &world, &idle, 10);

        simulate(&mut system, &mut player, &world, &intent(1.0), 220);

        assert!(
            player.position().x <= 16.0 - 0.3 + 1e-3,
            "玩家进入了未加载区块: {}",
            player.position().x
        );
    }

    #[test]
    fn walls_stop_horizontal_motion() {
        let positions = [ChunkPos::new(0, 0, 0), ChunkPos::new(1, 0, 0)];
        let mut world = flat_world(&positions);
        for z in 6..=10 {
            let _ = world.set_block(20, FLOOR_Y + 1, z, Block::Stone);
        }
        let mut player = player_at(10.0, 8.0);
        facing_positive_x(&mut player);
        let mut system = PhysicsSystem::new(PhysicsConfig::default());
        let idle = intent(0.0);
        simulate(&mut system, &mut player, &world, &idle, 10);

        simulate(&mut system, &mut player, &world, &intent(1.0), 220);

        assert!(
            player.position().x < 20.0 - 0.3 + 1e-3,
            "玩家穿过了墙壁: {}",
            player.position().x
        );
        assert!(player.position().x > 19.0);
    }

    #[test]
    fn ceilings_stop_jumps() {
        let mut world = flat_world(&[ChunkPos::new(0, 0, 0)]);
        for x in 9..=10 {
            let _ = world.set_block(x, 14, 8, Block::Stone);
        }
        let mut player = player_at(10.0, 8.0);
        let mut system = PhysicsSystem::new(PhysicsConfig::default());
        let idle = intent(0.0);
        simulate(&mut system, &mut player, &world, &idle, 10);

        let jump = MovementIntent {
            forward: 0.0,
            right: 0.0,
            jump: true,
        };
        let physics_world = PhysicsWorld::new(&world);
        let mut peak = player.position().y;
        system.update(&mut player, &jump, &physics_world, FIXED_TIMESTEP);
        for _ in 0..120 {
            system.update(&mut player, &idle, &physics_world, FIXED_TIMESTEP);
            peak = peak.max(player.position().y);
        }

        assert!(peak <= 14.0 - 1.8 + 1e-3, "玩家穿过了天花板: {}", peak);
    }

    #[test]
    fn fast_movement_does_not_tunnel_through_walls() {
        let positions = [
            ChunkPos::new(0, 0, 0),
            ChunkPos::new(1, 0, 0),
            ChunkPos::new(2, 0, 0),
        ];
        let mut world = flat_world(&positions);
        for z in 6..=10 {
            let _ = world.set_block(20, FLOOR_Y + 1, z, Block::Stone);
        }

        let config = PhysicsConfig {
            walk_speed: 100.0,
            ground_acceleration: 1000.0,
            ..PhysicsConfig::default()
        };
        let mut player = player_at(10.0, 8.0);
        facing_positive_x(&mut player);
        let mut system = PhysicsSystem::new(config);
        let idle = intent(0.0);
        simulate(&mut system, &mut player, &world, &idle, 10);

        simulate(&mut system, &mut player, &world, &intent(1.0), 100);

        assert!(
            player.position().x < 20.0,
            "高速移动穿过了墙壁: {}",
            player.position().x
        );
    }

    #[test]
    fn ground_friction_brings_the_player_to_rest() {
        let positions = [ChunkPos::new(0, 0, 0), ChunkPos::new(1, 0, 0)];
        let world = flat_world(&positions);
        let mut player = player_at(8.0, 8.0);
        facing_positive_x(&mut player);
        let mut system = PhysicsSystem::new(PhysicsConfig::default());
        let idle = intent(0.0);
        simulate(&mut system, &mut player, &world, &idle, 10);

        simulate(&mut system, &mut player, &world, &intent(1.0), 60);
        let moving = Vec3::new(player.velocity().x, 0.0, player.velocity().z).length();
        assert!(moving > 3.0, "未能加速到行走速度");

        simulate(&mut system, &mut player, &world, &idle, 120);
        let speed = Vec3::new(player.velocity().x, 0.0, player.velocity().z).length();
        assert!(speed < 0.01, "停止输入后未停下: {speed}");
    }

    #[test]
    fn fixed_timestep_caps_catch_up_steps() {
        let world = flat_world(&[ChunkPos::new(0, 0, 0)]);
        let mut player = player_at(8.0, 8.0);
        let mut system = PhysicsSystem::new(PhysicsConfig::default());
        let idle = intent(0.0);
        let physics_world = PhysicsWorld::new(&world);

        let update = system.update(&mut player, &idle, &physics_world, 0.5);
        assert_eq!(update.steps, MAX_STEPS_PER_FRAME);

        let before = player.position();
        let update = system.update(&mut player, &idle, &physics_world, f32::NAN);
        assert_eq!(update.steps, 0);
        assert!(player.position().is_finite());
        assert!((player.position() - before).length() < 1e-6);
    }

    #[test]
    fn invalid_config_is_normalized() {
        let config = PhysicsConfig {
            dimensions: PlayerDimensions {
                width: 0.0,
                height: -1.0,
                eye_height: f32::INFINITY,
            },
            walk_speed: f32::NAN,
            gravity: 0.0,
            ..PhysicsConfig::default()
        };
        let system = PhysicsSystem::new(config);
        let config = system.config();

        assert!(config.dimensions.width > 0.0);
        assert!(config.dimensions.height > 0.0);
        assert!(config.dimensions.eye_height.is_finite());
        assert!(config.dimensions.eye_height <= config.dimensions.height);
        assert!(config.walk_speed.is_finite() && config.walk_speed > 0.0);
        assert!(config.gravity > 0.0);
    }
}
