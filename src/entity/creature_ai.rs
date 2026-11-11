//! 生物行为：游荡、停留、避障、受伤逃离的状态机与地形探测。
//!
//! [`CreatureBrain::update`] 由实体管理器按较低频率调用，决策间隔随与玩家的距离变化；
//! 具体移动由 [`super::entity_physics`] 执行，本模块只输出期望方向。

use glam::Vec3;

use super::ai::{AiConfig, AiState};
use super::entity::{Entity, EntityLifecycle};
use super::entity_registry::{EntityDefinition, HostileCombat};
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
    attack_windup: f32,
    attack_cooldown: f32,
    attack_ready: bool,
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
            attack_windup: 0.0,
            attack_cooldown: 0.0,
            attack_ready: false,
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
        self.state.is_moving()
    }

    /// 攻击前摇是否已经完成、需要游戏层结算一次伤害。
    pub fn take_attack(&mut self) -> bool {
        std::mem::take(&mut self.attack_ready)
    }

    /// 受击进入短暂硬直；敌对生物不逃跑，避免被连续击退后永远背对玩家。
    pub fn hurt(&mut self, _source: Vec3, duration: f32) {
        if self.state == AiState::Dead {
            return;
        }
        self.state = AiState::Hurt;
        self.timer = duration.max(0.05);
        self.direction = Vec3::ZERO;
        self.attack_ready = false;
        self.attack_windup = 0.0;
        self.attack_cooldown = 0.0;
    }

    /// 直接设置状态与计时器，用于恢复存档或测试。
    pub fn set_state(&mut self, state: AiState, timer: f32) {
        self.state = state;
        self.timer = timer.max(0.0);
        self.blocked = false;
        self.direction = if state.is_moving() {
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
        if self.state.is_moving() {
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
        if self.attack_windup > 0.0 {
            self.attack_windup -= delta;
            if self.attack_windup <= 0.0 {
                self.attack_ready = true;
            }
        }
        if self.attack_cooldown > 0.0 {
            self.attack_cooldown = (self.attack_cooldown - delta).max(0.0);
        }
        if let Some(combat) = definition.combat {
            self.update_hostile(
                position,
                player_position,
                definition,
                combat,
                grounded,
                config,
                delta,
                is_solid,
                is_loaded,
            );
            return;
        }
        if self.decision_timer > 0.0 {
            return;
        }
        let interval = config.decision_interval(position.distance(player_position));
        self.decision_timer = interval.max(0.05);
        self.decide(position, definition, grounded, config, is_solid, is_loaded);
    }

    /// 敌对生物行为：巡逻 → 索敌 → 追击 → 攻击 → 脱战返回。
    #[allow(clippy::too_many_arguments)]
    fn update_hostile(
        &mut self,
        position: Vec3,
        player_position: Vec3,
        definition: &EntityDefinition,
        combat: HostileCombat,
        grounded: bool,
        config: &AiConfig,
        delta: f32,
        is_solid: &dyn Fn(i32, i32, i32) -> bool,
        is_loaded: &dyn Fn(i32, i32, i32) -> bool,
    ) {
        let distance = position.distance(player_position);
        let visible = has_line_of_sight(
            position + Vec3::Y * (definition.height * 0.8),
            player_position + Vec3::Y * 1.6,
            is_solid,
            is_loaded,
        );
        let detected = visible && distance <= combat.detection_range;

        match self.state {
            AiState::Dead => {}
            AiState::Hurt => {
                self.direction = Vec3::ZERO;
                if self.timer <= 0.0 {
                    self.begin_chase_if_visible(detected, position, player_position, config);
                }
            }
            AiState::Idle => {
                self.direction = Vec3::ZERO;
                if detected {
                    self.begin_chase(position, player_position, config);
                } else if self.timer <= 0.0 {
                    self.begin_patrol(config);
                }
            }
            AiState::Patrol | AiState::Wandering | AiState::Avoiding => {
                if detected {
                    self.begin_chase(position, player_position, config);
                } else if self.blocked
                    || self
                        .ahead_blocked(position, definition, grounded, config, is_solid, is_loaded)
                {
                    self.direction = self.random_direction();
                    self.timer = self.timer.max(0.4);
                } else if self.timer <= 0.0 {
                    self.begin_idle_hostile(config);
                } else {
                    self.steer_hostile_patrol(position, config);
                }
            }
            AiState::Chase => {
                if distance > combat.lose_range || !visible {
                    self.begin_return(config);
                    return;
                }
                if distance <= combat.attack_range {
                    self.begin_attack(combat, config);
                    return;
                }
                self.direction = horizontal_towards(position, player_position);
                if self.blocked
                    || self
                        .ahead_blocked(position, definition, grounded, config, is_solid, is_loaded)
                {
                    // 侧向绕行，避免被一格宽障碍永久卡住。
                    let side = Vec3::new(-self.direction.z, 0.0, self.direction.x);
                    self.direction = side;
                }
                self.timer = 0.5;
            }
            AiState::Attack => {
                self.direction = Vec3::ZERO;
                if self.attack_cooldown <= 0.0 && self.attack_windup <= 0.0 {
                    if distance <= combat.attack_range {
                        self.begin_attack(combat, config);
                    } else {
                        self.begin_chase(position, player_position, config);
                    }
                }
            }
            AiState::Return => {
                if detected {
                    self.begin_chase(position, player_position, config);
                    return;
                }
                let to_anchor =
                    Vec3::new(self.anchor.x - position.x, 0.0, self.anchor.z - position.z);
                if to_anchor.length() <= 0.5 || self.timer <= 0.0 {
                    self.begin_idle_hostile(config);
                } else {
                    self.direction = to_anchor.normalize();
                }
            }
            AiState::Fleeing => {
                // 敌对生物不使用逃跑状态；受击走 Hurt。
                self.begin_chase_if_visible(detected, position, player_position, config);
            }
        }
        let _ = delta;
        self.blocked = false;
    }

    fn begin_chase_if_visible(
        &mut self,
        visible: bool,
        position: Vec3,
        player_position: Vec3,
        config: &AiConfig,
    ) {
        if visible {
            self.begin_chase(position, player_position, config);
        } else {
            self.begin_idle_hostile(config);
        }
    }

    fn begin_chase(&mut self, position: Vec3, player_position: Vec3, _config: &AiConfig) {
        self.state = AiState::Chase;
        self.timer = 0.5;
        self.direction = horizontal_towards(position, player_position);
    }

    fn begin_attack(&mut self, combat: HostileCombat, config: &AiConfig) {
        self.state = AiState::Attack;
        self.direction = Vec3::ZERO;
        self.attack_windup = config.attack_windup.max(0.05);
        self.attack_cooldown = combat.attack_cooldown.max(0.1);
        self.attack_ready = false;
        self.timer = self.attack_cooldown;
    }

    fn begin_return(&mut self, _config: &AiConfig) {
        self.state = AiState::Return;
        self.timer = 20.0;
        self.direction = Vec3::ZERO;
    }

    fn begin_patrol(&mut self, config: &AiConfig) {
        self.state = AiState::Patrol;
        self.timer = self.range(config.wander_seconds_min, config.wander_seconds_max);
        self.direction = self.random_direction();
    }

    fn begin_idle_hostile(&mut self, config: &AiConfig) {
        self.state = AiState::Idle;
        self.timer = self.range(config.idle_seconds_min, config.idle_seconds_max);
        self.direction = Vec3::ZERO;
    }

    fn steer_hostile_patrol(&mut self, position: Vec3, config: &AiConfig) {
        let to_anchor = Vec3::new(self.anchor.x - position.x, 0.0, self.anchor.z - position.z);
        if to_anchor.length() > config.hostile_activity_radius {
            self.direction = to_anchor.normalize();
            self.timer = self.timer.max(0.8);
        }
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
            // 被动生物不会进入敌对状态；若存档中残留这些状态则回到停留。
            AiState::Patrol
            | AiState::Chase
            | AiState::Attack
            | AiState::Return
            | AiState::Hurt => self.begin_idle(config),
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

/// 水平方向单位向量；重合时返回零向量。
fn horizontal_towards(from: Vec3, to: Vec3) -> Vec3 {
    let delta = Vec3::new(to.x - from.x, 0.0, to.z - from.z);
    delta.normalize_or_zero()
}

/// 以 0.25 格步长采样判断视线是否被方块遮挡。
fn has_line_of_sight(
    from: Vec3,
    to: Vec3,
    is_solid: &dyn Fn(i32, i32, i32) -> bool,
    is_loaded: &dyn Fn(i32, i32, i32) -> bool,
) -> bool {
    let delta = to - from;
    let distance = delta.length();
    if distance <= 1e-4 {
        return true;
    }
    let direction = delta / distance;
    let step = 0.25_f32;
    let mut travelled = step;
    // 预留 0.3 格，避免把目标所在的空气方块误判为遮挡。
    while travelled < distance - 0.3 {
        let point = from + direction * travelled;
        let x = point.x.floor() as i32;
        let y = point.y.floor() as i32;
        let z = point.z.floor() as i32;
        if is_loaded(x, y, z) && is_solid(x, y, z) {
            return false;
        }
        travelled += step;
    }
    true
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

    fn tick_zombie(
        brain: &mut CreatureBrain,
        position: Vec3,
        player: Vec3,
        config: &AiConfig,
        seconds: f32,
    ) -> bool {
        let definition = &creature_registry::ZOMBIE;
        let step = 0.1;
        let mut elapsed = 0.0;
        let mut attacked = false;
        while elapsed < seconds {
            brain.update_with(
                position,
                player,
                definition,
                true,
                config,
                step,
                &solid_floor,
                &loaded,
            );
            attacked |= brain.take_attack();
            elapsed += step;
        }
        attacked
    }

    #[test]
    fn zombie_chases_visible_player() {
        let config = AiConfig::default();
        let mut brain = CreatureBrain::new(Vec3::ZERO, 11);
        tick_zombie(
            &mut brain,
            Vec3::ZERO,
            Vec3::new(0.0, 0.0, 6.0),
            &config,
            0.3,
        );
        assert_eq!(brain.state(), AiState::Chase);
        assert!(brain.steer().z > 0.9, "应朝玩家方向水平移动");
    }

    #[test]
    fn zombie_attacks_once_per_windup() {
        let config = AiConfig::default();
        let mut brain = CreatureBrain::new(Vec3::ZERO, 12);
        let player = Vec3::new(0.0, 0.0, 1.5);
        let attacked = tick_zombie(&mut brain, Vec3::ZERO, player, &config, 1.0);
        assert_eq!(brain.state(), AiState::Attack);
        assert!(attacked, "前摇结束后应产生一次攻击判定");
        assert!(!brain.take_attack(), "同一次攻击只结算一次");
        assert!(!tick_zombie(&mut brain, Vec3::ZERO, player, &config, 0.3));
    }

    #[test]
    fn zombie_returns_when_player_escapes() {
        let config = AiConfig::default();
        let mut brain = CreatureBrain::new(Vec3::ZERO, 13);
        tick_zombie(
            &mut brain,
            Vec3::ZERO,
            Vec3::new(0.0, 0.0, 6.0),
            &config,
            0.3,
        );
        assert_eq!(brain.state(), AiState::Chase);
        let combat = creature_registry::ZOMBIE.combat.expect("僵尸应有战斗参数");
        let far = Vec3::new(0.0, 0.0, combat.lose_range + 5.0);
        // 与锚点保持一点距离，避免返回状态因“已到家”立刻转入待机。
        tick_zombie(&mut brain, Vec3::new(0.0, 0.0, 1.0), far, &config, 0.2);
        assert_eq!(brain.state(), AiState::Return);
    }

    #[test]
    fn hurt_interrupts_zombie_before_it_resumes_chasing() {
        let config = AiConfig::default();
        let mut brain = CreatureBrain::new(Vec3::ZERO, 14);
        brain.hurt(Vec3::new(0.0, 0.0, 3.0), config.hurt_seconds);
        assert_eq!(brain.state(), AiState::Hurt);
        tick_zombie(
            &mut brain,
            Vec3::ZERO,
            Vec3::new(0.0, 0.0, 3.0),
            &config,
            config.hurt_seconds + 0.2,
        );
        assert_eq!(brain.state(), AiState::Chase);
    }
}
