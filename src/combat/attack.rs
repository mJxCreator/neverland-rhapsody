//! 攻击控制器：前摇 → 判定窗口 → 后摇/冷却。
//!
//! 控制器只负责时序，不访问世界或实体；游戏层在 [`AttackController::tick`]
//! 返回 `true` 的那一帧执行一次命中判定，保证同一次挥击不会重复伤害。

use super::combat_state::{AttackState, CombatConfig};
use super::weapon::WeaponStats;

/// 发起攻击失败的原因。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AttackError {
    /// 上一次攻击尚未结束。
    Cooldown { remaining: f32 },
}

/// 玩家攻击状态机。
#[derive(Clone, Debug, PartialEq)]
pub struct AttackController {
    state: AttackState,
    timer: f32,
    stats: WeaponStats,
    config: CombatConfig,
}

impl Default for AttackController {
    fn default() -> Self {
        Self::new(CombatConfig::default())
    }
}

impl AttackController {
    pub fn new(config: CombatConfig) -> Self {
        Self {
            state: AttackState::Idle,
            timer: 0.0,
            stats: WeaponStats::unarmed(),
            config,
        }
    }

    pub fn state(&self) -> AttackState {
        self.state
    }

    /// 本次攻击锁定下来的武器参数。
    pub fn stats(&self) -> WeaponStats {
        self.stats
    }

    pub fn can_attack(&self) -> bool {
        self.state.is_idle()
    }

    /// 开始一次攻击；冷却中返回剩余时间。
    pub fn try_start(&mut self, stats: WeaponStats) -> Result<(), AttackError> {
        if !self.can_attack() {
            return Err(AttackError::Cooldown {
                remaining: self.timer.max(0.0),
            });
        }
        self.stats = stats;
        self.state = AttackState::Windup;
        self.timer = self.config.windup.max(0.0);
        // 前摇为 0 时立即进入判定窗口。
        if self.timer == 0.0 {
            self.state = AttackState::Active;
            self.timer = self.config.active;
        }
        Ok(())
    }

    /// 推进攻击状态；返回 `true` 表示本帧刚进入判定窗口，应结算一次命中。
    pub fn tick(&mut self, delta: f32) -> bool {
        if self.state.is_idle() || !delta.is_finite() || delta <= 0.0 {
            return false;
        }
        let mut remaining = delta;
        let mut window_opened = false;
        loop {
            if remaining <= 0.0 {
                break;
            }
            match self.state {
                AttackState::Idle => break,
                AttackState::Windup => {
                    if self.timer > remaining {
                        self.timer -= remaining;
                        break;
                    }
                    remaining -= self.timer;
                    self.state = AttackState::Active;
                    self.timer = self.config.active;
                    window_opened = true;
                }
                AttackState::Active => {
                    if self.timer > remaining {
                        self.timer -= remaining;
                        break;
                    }
                    remaining -= self.timer;
                    self.state = AttackState::Recovery;
                    self.timer = self.recovery_duration();
                }
                AttackState::Recovery => {
                    if self.timer > remaining {
                        self.timer -= remaining;
                        break;
                    }
                    self.state = AttackState::Idle;
                    self.timer = 0.0;
                    break;
                }
            }
        }
        window_opened
    }

    fn recovery_duration(&self) -> f32 {
        (self.stats.attack_interval - self.config.windup - self.config.active).max(0.0)
    }

    /// 回到空闲状态；用于重生或切换世界。
    pub fn reset(&mut self) {
        self.state = AttackState::Idle;
        self.timer = 0.0;
        self.stats = WeaponStats::unarmed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::ItemId;

    fn sword_stack() -> crate::item::ItemStack {
        crate::item::ItemStack::new(ItemId::IRON_SWORD, 1).expect("剑合法")
    }

    fn tick_until_started(controller: &mut AttackController, config: CombatConfig) -> bool {
        let mut elapsed = 0.0;
        while elapsed < config.windup + 0.01 {
            if controller.tick(0.01) {
                return true;
            }
            elapsed += 0.01;
        }
        false
    }

    #[test]
    fn attack_opens_exactly_one_hit_window() {
        let config = CombatConfig::default();
        let mut controller = AttackController::new(config);
        controller
            .try_start(WeaponStats::for_stack(Some(sword_stack())))
            .expect("空闲可攻击");
        assert_eq!(controller.state(), AttackState::Windup);

        let mut windows = 0;
        let mut elapsed = 0.0;
        while elapsed < 1.0 {
            if controller.tick(0.01) {
                windows += 1;
            }
            elapsed += 0.01;
        }
        assert_eq!(windows, 1);
        assert_eq!(controller.state(), AttackState::Idle);
    }

    #[test]
    fn cannot_start_while_attacking() {
        let mut controller = AttackController::new(CombatConfig::default());
        controller
            .try_start(WeaponStats::for_stack(Some(sword_stack())))
            .unwrap();
        assert!(controller
            .try_start(WeaponStats::for_stack(Some(sword_stack())))
            .is_err());
    }

    #[test]
    fn zero_windup_opens_window_immediately() {
        let config = CombatConfig {
            windup: 0.0,
            active: 0.1,
        };
        let mut controller = AttackController::new(config);
        controller
            .try_start(WeaponStats::for_stack(Some(sword_stack())))
            .expect("空闲可攻击");
        assert_eq!(controller.state(), AttackState::Active);
        assert!(!controller.tick(0.05), "窗口已经打开，不应再次返回 true");
        assert!(!controller.tick(0.06), "窗口结束后不应重复结算");
    }

    #[test]
    fn full_cycle_respects_weapon_interval() {
        let config = CombatConfig::default();
        let mut controller = AttackController::new(config);
        controller
            .try_start(WeaponStats::for_stack(Some(sword_stack())))
            .unwrap();
        assert!(tick_until_started(&mut controller, config));
        // 0.8 秒间隔内无法再次攻击。
        assert!(!controller.can_attack());
        controller.tick(0.5);
        assert!(!controller.can_attack());
        controller.tick(0.2);
        assert!(controller.can_attack());
    }
}
