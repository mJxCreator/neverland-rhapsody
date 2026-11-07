//! 生命值与死亡状态。
//!
//! `Survival` 只保存生命值、存活状态与无敌帧计时；伤害必须经
//! [`super::damage::apply`] 进入，避免多个系统绕过统一入口。

use serde::{Deserialize, Serialize};

use super::damage::Damage;

/// 默认最大生命值：20 点，即 10 颗心。
pub const DEFAULT_MAX_HEALTH: u32 = 20;
/// 允许通过配置设置的最大生命值上限，防止异常存档造成不可控数值。
pub const MAX_HEALTH_LIMIT: u32 = 200;
/// 常规伤害后的无敌帧时长（秒）。
const INVULNERABILITY_SECONDS: f32 = 0.5;

/// 玩家存活状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurvivalState {
    Alive,
    Dead,
}

/// 统一伤害结算结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageOutcome {
    /// 未造成伤害：死亡状态、无敌帧或无效数值。
    Ignored,
    /// 已扣血但仍存活。
    Applied { health: u32 },
    /// 本次伤害导致死亡。
    Killed,
}

/// 需要写入存档的生存状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedSurvival {
    #[serde(default = "default_health")]
    pub max_health: u32,
    #[serde(default = "default_health")]
    pub current_health: u32,
}

const fn default_health() -> u32 {
    DEFAULT_MAX_HEALTH
}

impl Default for SavedSurvival {
    fn default() -> Self {
        Self {
            max_health: DEFAULT_MAX_HEALTH,
            current_health: DEFAULT_MAX_HEALTH,
        }
    }
}

impl SavedSurvival {
    /// 修正异常存档：非法最大生命值回落到默认值；归零生命值按“崩溃恢复规则”
    /// 恢复为满血，避免玩家被永久保存在无法操作的死亡状态。
    pub fn sanitized(self) -> Self {
        let max_health = normalize_max_health(self.max_health);
        let current_health = if self.current_health == 0 {
            max_health
        } else {
            self.current_health.min(max_health)
        };
        Self {
            max_health,
            current_health,
        }
    }
}

fn normalize_max_health(value: u32) -> u32 {
    if value == 0 || value > MAX_HEALTH_LIMIT {
        DEFAULT_MAX_HEALTH
    } else {
        value
    }
}

/// 运行时生存状态。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Survival {
    max_health: u32,
    health: u32,
    state: SurvivalState,
    invulnerability_seconds: f32,
}

impl Default for Survival {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_HEALTH)
    }
}

impl Survival {
    /// 用给定最大生命值构造满血存活状态；非法值回落到默认值。
    pub fn new(max_health: u32) -> Self {
        let max_health = normalize_max_health(max_health);
        Self {
            max_health,
            health: max_health,
            state: SurvivalState::Alive,
            invulnerability_seconds: 0.0,
        }
    }

    /// 从存档恢复；归零生命值按崩溃恢复规则满血复活。
    pub fn from_saved(saved: SavedSurvival) -> Self {
        let saved = saved.sanitized();
        Self {
            max_health: saved.max_health,
            health: saved.current_health,
            state: SurvivalState::Alive,
            invulnerability_seconds: 0.0,
        }
    }

    /// 写入存档；死亡状态不持久化，按满血保存。
    pub fn to_saved(self) -> SavedSurvival {
        let current = match self.state {
            SurvivalState::Alive => self.health,
            SurvivalState::Dead => self.max_health,
        };
        SavedSurvival {
            max_health: self.max_health,
            current_health: current,
        }
    }

    pub const fn max_health(self) -> u32 {
        self.max_health
    }

    pub const fn health(self) -> u32 {
        self.health
    }

    pub fn is_dead(self) -> bool {
        self.state == SurvivalState::Dead
    }

    /// 剩余无敌帧时长（秒）。
    pub fn invulnerability_remaining(self) -> f32 {
        self.invulnerability_seconds
    }

    /// 每帧推进无敌帧计时。
    pub fn update(&mut self, delta_seconds: f32) {
        if delta_seconds.is_finite() && delta_seconds > 0.0 {
            self.invulnerability_seconds = (self.invulnerability_seconds - delta_seconds).max(0.0);
        }
    }

    /// 重生：恢复满血、清除死亡状态与无敌帧。
    pub fn respawn(&mut self) {
        self.health = self.max_health;
        self.state = SurvivalState::Alive;
        self.invulnerability_seconds = 0.0;
    }

    /// 统一伤害结算入口；仅允许 [`super::damage`] 调用。
    pub(crate) fn receive_damage(&mut self, damage: Damage) -> DamageOutcome {
        if self.state == SurvivalState::Dead || damage.amount() == 0 {
            return DamageOutcome::Ignored;
        }
        if self.invulnerability_seconds > 0.0 && !damage.bypasses_invulnerability() {
            return DamageOutcome::Ignored;
        }

        if !damage.bypasses_invulnerability() {
            self.invulnerability_seconds = INVULNERABILITY_SECONDS;
        }

        self.health = self.health.saturating_sub(damage.amount());
        if self.health == 0 {
            self.state = SurvivalState::Dead;
            DamageOutcome::Killed
        } else {
            DamageOutcome::Applied {
                health: self.health,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::damage::{self, Damage, DamageSource};

    fn fall(amount: u32) -> Damage {
        Damage::new(amount, DamageSource::Fall).expect("有效伤害")
    }

    #[test]
    fn new_player_starts_full_health_and_alive() {
        let survival = Survival::default();
        assert_eq!(survival.max_health(), DEFAULT_MAX_HEALTH);
        assert_eq!(survival.health(), DEFAULT_MAX_HEALTH);
        assert!(!survival.is_dead());
    }

    #[test]
    fn invalid_max_health_falls_back_to_default() {
        assert_eq!(Survival::new(0).max_health(), DEFAULT_MAX_HEALTH);
        assert_eq!(
            Survival::new(MAX_HEALTH_LIMIT + 1).max_health(),
            DEFAULT_MAX_HEALTH
        );
        assert_eq!(Survival::new(12).health(), 12);
    }

    #[test]
    fn damage_reduces_health_and_applies_invulnerability() {
        let mut survival = Survival::default();
        assert_eq!(
            damage::apply(&mut survival, fall(4)),
            DamageOutcome::Applied { health: 16 }
        );
        assert!(survival.invulnerability_remaining() > 0.0);
        assert_eq!(
            damage::apply(&mut survival, fall(4)),
            DamageOutcome::Ignored,
            "无敌帧内不得重复扣血"
        );
    }

    #[test]
    fn invulnerability_expires_after_half_a_second() {
        let mut survival = Survival::default();
        let _ = damage::apply(&mut survival, fall(4));
        survival.update(0.6);
        assert_eq!(
            damage::apply(&mut survival, fall(4)),
            DamageOutcome::Applied { health: 12 }
        );
    }

    #[test]
    fn killing_damage_transitions_to_dead_and_ignores_further_hits() {
        let mut survival = Survival::new(4);
        assert_eq!(damage::apply(&mut survival, fall(5)), DamageOutcome::Killed);
        assert!(survival.is_dead());
        assert_eq!(survival.health(), 0);
        assert_eq!(
            damage::apply(&mut survival, fall(1)),
            DamageOutcome::Ignored
        );
    }

    #[test]
    fn critical_damage_bypasses_invulnerability() {
        let mut survival = Survival::default();
        let _ = damage::apply(&mut survival, fall(1));
        let critical = fall(2).ignoring_invulnerability();
        assert_eq!(
            damage::apply(&mut survival, critical),
            DamageOutcome::Applied { health: 17 }
        );
    }

    #[test]
    fn respawn_restores_full_health_and_clears_death() {
        let mut survival = Survival::new(4);
        let _ = damage::apply(&mut survival, fall(5));
        assert!(survival.is_dead());
        survival.respawn();
        assert!(!survival.is_dead());
        assert_eq!(survival.health(), 4);
        assert_eq!(survival.invulnerability_remaining(), 0.0);
    }

    #[test]
    fn saved_dead_state_is_normalized_to_full_health() {
        let mut survival = Survival::new(6);
        let _ = damage::apply(&mut survival, fall(7));
        let saved = survival.to_saved();
        assert_eq!(saved.current_health, 6);
        let restored = Survival::from_saved(saved);
        assert!(!restored.is_dead());
        assert_eq!(restored.health(), 6);
    }

    #[test]
    fn corrupted_saves_are_recovered() {
        let corrupted = SavedSurvival {
            max_health: 0,
            current_health: 0,
        }
        .sanitized();
        assert_eq!(corrupted.max_health, DEFAULT_MAX_HEALTH);
        assert_eq!(corrupted.current_health, DEFAULT_MAX_HEALTH);

        let restored = Survival::from_saved(SavedSurvival {
            max_health: 10,
            current_health: 0,
        });
        assert_eq!(restored.max_health(), 10);
        assert_eq!(restored.health(), 10);
        assert!(!restored.is_dead(), "归零生命值必须恢复为可操作状态");
    }

    #[test]
    fn current_health_is_clamped_to_max() {
        let restored = Survival::from_saved(SavedSurvival {
            max_health: 10,
            current_health: 25,
        });
        assert_eq!(restored.health(), 10);
    }
}
