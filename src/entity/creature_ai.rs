//! 生物行为：游荡、停留、避障、受伤逃离的状态机与地形探测。
//!
//! [`CreatureBrain::update`] 由实体管理器按较低频率调用，决策间隔随与玩家的距离变化；
//! 具体移动由 [`super::entity_physics`] 执行，本模块只输出期望方向。

use glam::Vec3;

use super::ai::{AiConfig, AiState};
use super::entity::{Entity, EntityLifecycle};
use super::entity_registry::EntityDefinition;
use crate::world::World;

/// 单个生物的 AI 状态与计时器。
#[derive(Clone, Debug)]
pub struct CreatureBrain {
    state: AiState,
    timer: f32,
    decision_timer: f32,
    direction: Vec3,
    anchor: Vec3,
    blocked: bool,
    rng: u32,
}

impl CreatureBrain {
    /// 以出生点附近的锚点创建大脑；初始停留时间随机错开。
    pub fn new(anchor: Vec3, seed: u32) -> Self {
        let mut brain = Self {
            state: AiState::Idle,
            timer: 0.0,
            decision_timer: 0.0,
            direction: Vec3::ZERO,
            anchor,
            blocked: false,
            rng: seed.wrapping_mul(2_654_435_761).wrapping_add(1),
        };
        brain.timer = brain.range(0.2, 1.0);
        brain
    }

    /// 从存档恢复状态与计时器。
    pub fn restore(state: AiState, timer: f32, anchor: Vec3, seed: u32) -> Self {
        let mut brain = Self::new(anchor, seed);
        brain.set_state(state, timer);
        brain
    }

    /// 当前状态。
    pub fn state(&self) -> AiState {
        self.state
    }

    /// 当前状态剩余时间。
    pub fn timer(&self) -> f32 {
        self.timer
    }

    /// 出生锚点。
    pub fn anchor(&self) -> Vec3 {
        self.anchor
    }

    /// 是否处于活动状态。
    pub fn is_active_behavior(&self) -> bool {
        matches!(self.state, AiState::Wandering | AiState::Fleeing)
    }

    /// 直接设置状态与计时器，用于恢复存档或测试。
    pub fn set_state(&mut self, state: AiState, timer: f32) {
        self.state = state;
        self.timer = timer.max(0.0);
        self.blocked = false;
        self.direction = if matches!(state, AiState::Wandering | AiState::Fleeing) {
            self.random_direction()
        } else {
            Vec3::ZERO
        };
    }

    /// 受击后逃离伤害来源，方向为远离来源的水平向量。
    pub fn flee(&mut self, position: Vec3, source: Vec3, duration: f32) {
        if self.state == AiState::Dead {
            return;
        }
        let away = Vec3::new(position.x - source.x, 0.0, position.z - source.z);
        self.direction = if away.length_squared() > 1e-6 {
            away.normalize()
        } else {
            self.random_direction()
        };
        self.state = AiState::Fleeing;
        self.timer = duration.max(0.5);
        self.blocked = false;
        self.decision_timer = 0.0;
    }

    /// 物理层报告水平方向被阻挡。
    pub fn note_obstacle(&mut self) {
        if matches!(self.state, AiState::Wandering | AiState::Fleeing) {
            self.blocked = true;
        }
    }

    /// 让 AI 标记为死亡。
    pub fn mark_dead(&mut self) {
        self.state = AiState::Dead;
        self.direction = Vec3::ZERO;
    }

    /// 期望的水平移动方向；停留/调整/死亡时为零。
    pub fn steer(&self) -> Vec3 {
        if self.is_active_behavior() {
            self.direction
        } else {
            Vec3::ZERO
        }
    }

    /// 用世界方块信息驱动 AI；`delta` 为经过的时间。
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        position: Vec3,
        player_position: Vec3,
        world: &World,
        definition: &EntityDefinition,
        grounded: bool,
        config: &AiConfig,
        delta: f32,
    ) {
        self.update_with(
            position,
            player_position,
            definition,
            grounded,
            config,
            delta,
            &|x, y, z| world.contains(x, y, z) && world.block_at(x, y, z).is_solid(),
            &|x, y, z| world.contains(x, y, z),
        );
    }

    /// 可测试版本：方块查询由调用方提供。
    #[allow(clippy::too_many_arguments)]
    pub fn update_with(
        &mut self,
        position: Vec3,
        player_position: Vec3,
        definition: &EntityDefinition,
        grounded: bool,
        config: &AiConfig,
        delta: f32,
        is_solid: &dyn Fn(i32, i32, i32) -> bool,
        is_loaded: &dyn Fn(i32, i32, i32) -> bool,
    ) {
        if self.state == AiState::Dead {
            return;
        }
        let delta = delta.clamp(0.0, 1.0);
        self.timer -= delta;
        self.decision_timer -= delta;
        if self.decision_timer > 0.0 {
            return;
        }
        let interval = config.decision_interval(position.distance(player_position));
        self.decision_timer = interval.max(0.05);
        self.decide(position, definition, grounded, config, is_solid, is_loaded);
    }

    fn decide(
        &mut self,
        position: Vec3,
        definition: &EntityDefinition,
        grounded: bool,
        config: &AiConfig,
        is_solid: &dyn Fn(i32, i32, i32) -> bool,
        is_loaded: &dyn Fn(i32, i32, i32) -> bool,
    ) {
        match self.state {
            AiState::Dead => {}
            AiState::Idle => {
                if self.timer <= 0.0 {
                    self.begin_wandering(config);
                }
            }
            AiState::Avoiding => {
                if self.timer <= 0.0 {
                    self.begin_wandering(config);
                }
            }
            AiState::Fleeing => {
                if self.timer <= 0.0 {
                    self.begin_idle(config);
                } else if self.blocked {
                    self.direction = self.random_direction();
                }
            }
            AiState::Wandering => {
                if self.blocked
                    || self
                        .ahead_blocked(position, definition, grounded, config, is_solid, is_loaded)
                {
                    self.begin_avoiding(config);
                } else if self.timer <= 0.0 {
                    self.begin_idle(config);
                } else {
                    self.steer_towards_anchor(position, config);
                }
            }
        }
        self.blocked = false;
    }

    fn steer_towards_anchor(&mut self, position: Vec3, config: &AiConfig) {
        let to_anchor = Vec3::new(self.anchor.x - position.x, 0.0, self.anchor.z - position.z);
        if to_anchor.length() > config.activity_radius {
            self.direction = to_anchor.normalize();
            self.timer = self.timer.max(0.8);
        }
    }

    fn ahead_blocked(
        &self,
        position: Vec3,
        definition: &EntityDefinition,
        grounded: bool,
        config: &AiConfig,
        is_solid: &dyn Fn(i32, i32, i32) -> bool,
        is_loaded: &dyn Fn(i32, i32, i32) -> bool,
    ) -> bool {
        let ahead = position + self.direction * (config.obstacle_probe + definition.width * 0.5);
        let x = ahead.x.floor() as i32;
        let z = ahead.z.floor() as i32;
        let feet_y = position.y.floor() as i32;
        let head_y = (position.y + definition.height * 0.5).floor() as i32;
        if head_y != feet_y && is_loaded(x, head_y, z) && is_solid(x, head_y, z) {
            return true;
        }
        if is_loaded(x, feet_y, z) && is_solid(x, feet_y, z) {
            return true;
        }
        if grounded {
            let ground_y = (position.y - config.cliff_probe_depth).floor() as i32;
            if is_loaded(x, ground_y, z) && !is_solid(x, ground_y, z) {
                return true;
            }
        }
        false
    }

    fn begin_idle(&mut self, config: &AiConfig) {
        self.state = AiState::Idle;
        self.timer = self.range(config.idle_seconds_min, config.idle_seconds_max);
        self.direction = Vec3::ZERO;
    }

    fn begin_wandering(&mut self, config: &AiConfig) {
        self.state = AiState::Wandering;
        self.timer = self.range(config.wander_seconds_min, config.wander_seconds_max);
        self.direction = self.random_direction();
    }

    fn begin_avoiding(&mut self, config: &AiConfig) {
        self.state = AiState::Avoiding;
        self.timer = config.avoid_seconds;
        self.direction = Vec3::ZERO;
    }

    fn range(&mut self, min: f32, max: f32) -> f32 {
        if max <= min {
            return min;
        }
        min + self.next_unit() * (max - min)
    }

    fn random_direction(&mut self) -> Vec3 {
        let angle = self.next_unit() * std::f32::consts::TAU;
        Vec3::new(angle.cos(), 0.0, angle.sin())
    }

    fn next_unit(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        ((self.rng >> 8) as f32) / ((1u32 << 24) as f32)
    }
}

impl From<&Entity> for CreatureBrain {
    fn from(entity: &Entity) -> Self {
        let anchor = entity.position();
        let state = if entity.lifecycle() == EntityLifecycle::Dead {
            AiState::Dead
        } else {
            AiState::Idle
        };
        Self::restore(state, 0.0, anchor, entity.id().raw() as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::creature_registry;
    use crate::entity::entity::{EntityDimensions, EntityId, EntityTypeId};

    fn solid_none(_x: i32, _y: i32, _z: i32) -> bool {
        false
    }
    /// 只把 y < 0 视为实体地面，模拟平坦地面。
    fn solid_floor(_x: i32, y: i32, _z: i32) -> bool {
        y < 0
    }
    fn loaded(_x: i32, _y: i32, _z: i32) -> bool {
        true
    }
    fn solid_all(_x: i32, _y: i32, _z: i32) -> bool {
        true
    }

    fn tick(brain: &mut CreatureBrain, position: Vec3, config: &AiConfig, seconds: f32) {
        let step = 0.1;
        let mut elapsed = 0.0;
        while elapsed < seconds {
            brain.update_with(
                position,
                Vec3::new(0.0, 0.0, 0.0),
                &creature_registry::PIG,
                true,
                config,
                step,
                &solid_floor,
                &loaded,
            );
            elapsed += step;
        }
    }

    #[test]
    fn idle_transitions_into_wandering() {
        let config = AiConfig::default();
        let mut brain = CreatureBrain::new(Vec3::ZERO, 42);
        tick(&mut brain, Vec3::ZERO, &config, 1.5);
        assert_eq!(brain.state(), AiState::Wandering);
        assert!((brain.steer().length() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn obstacle_switches_to_avoiding() {
        let config = AiConfig::default();
        let mut brain = CreatureBrain::new(Vec3::ZERO, 7);
        brain.set_state(AiState::Wandering, 3.0);
        brain.note_obstacle();
        brain.update_with(
            Vec3::ZERO,
            Vec3::new(100.0, 0.0, 0.0),
            &creature_registry::PIG,
            true,
            &config,
            0.1,
            &solid_none,
            &loaded,
        );
        assert_eq!(brain.state(), AiState::Avoiding);
        assert_eq!(brain.steer(), Vec3::ZERO);
    }

    #[test]
    fn fleeing_points_away_and_expires() {
        let config = AiConfig::default();
        let mut brain = CreatureBrain::new(Vec3::ZERO, 3);
        brain.flee(
            Vec3::new(10.0, 1.0, 0.0),
            Vec3::new(8.0, 1.0, 0.0),
            config.flee_seconds,
        );
        assert_eq!(brain.state(), AiState::Fleeing);
        assert!(brain.steer().x > 0.0, "逃离方向应远离伤害来源");
        tick(
            &mut brain,
            Vec3::new(10.0, 1.0, 0.0),
            &config,
            config.flee_seconds + 0.3,
        );
        assert_eq!(brain.state(), AiState::Idle);
    }

    #[test]
    fn wandering_returns_towards_anchor_outside_radius() {
        let config = AiConfig::default();
        let mut brain = CreatureBrain::new(Vec3::ZERO, 11);
        brain.set_state(AiState::Wandering, 2.0);
        let far = Vec3::new(20.0, 0.0, 0.0);
        brain.update_with(
            far,
            Vec3::new(100.0, 0.0, 0.0),
            &creature_registry::PIG,
            true,
            &config,
            0.1,
            &solid_floor,
            &loaded,
        );
        assert!(brain.steer().x < 0.0, "超出活动半径后应朝锚点返回");
    }

    #[test]
    fn wall_ahead_triggers_avoiding() {
        let config = AiConfig::default();
        let mut brain = CreatureBrain::new(Vec3::ZERO, 5);
        brain.set_state(AiState::Wandering, 2.0);
        brain.direction = Vec3::new(1.0, 0.0, 0.0);
        brain.update_with(
            Vec3::ZERO,
            Vec3::new(100.0, 0.0, 0.0),
            &creature_registry::PIG,
            true,
            &config,
            0.1,
            &solid_all,
            &loaded,
        );
        assert_eq!(brain.state(), AiState::Avoiding);
    }

    #[test]
    fn dead_brain_does_not_move() {
        let config = AiConfig::default();
        let mut brain = CreatureBrain::new(Vec3::ZERO, 1);
        brain.mark_dead();
        brain.update_with(
            Vec3::ZERO,
            Vec3::ZERO,
            &creature_registry::PIG,
            true,
            &config,
            5.0,
            &solid_none,
            &loaded,
        );
        assert_eq!(brain.state(), AiState::Dead);
        assert_eq!(brain.steer(), Vec3::ZERO);
    }

    #[test]
    fn brain_from_entity_starts_idle() {
        let entity = Entity::new(
            EntityId::new(5),
            EntityTypeId::PIG,
            Vec3::new(3.0, 4.0, 5.0),
            0.0,
            EntityDimensions::new(0.9, 0.9),
            10,
        );
        let brain = CreatureBrain::from(&entity);
        assert_eq!(brain.state(), AiState::Idle);
        assert_eq!(brain.anchor(), Vec3::new(3.0, 4.0, 5.0));
    }
}
