//! 饥饿值（默认上限 100，上限本身可在运行时调整）：进食恢复、体力耗尽时的
//! 饥饿消耗与基于饥饿的比例派生。
//!
//! 饱和度机制已移除。饥饿只通过“体力耗尽一轮扣 1 点”下降，进食按食物定义
//! 直接恢复。行走/奔跑速度与跳跃高度都由当前饥饿比例派生。

use serde::{Deserialize, Serialize};

/// 默认饥饿上限；上限本身是运行时状态，可被道具或效果改变。
pub const DEFAULT_MAX_HUNGER: f32 = 100.0;
/// 饥饿百分比的基准量程：100 点对应 100%，用于派生速度、跳跃与基础伤害。
pub const HUNGER_PERCENT_UNIT: f32 = 100.0;
/// 基础行走速度（格/秒）：实际速度 = 基础值 + 饥饿比例。
pub const BASE_WALK_SPEED: f32 = 3.0;
/// 基础跳跃高度（格）：实际高度 = 基础值 + 饥饿比例。
pub const BASE_JUMP_HEIGHT: f32 = 0.5;
/// 基础伤害基准值：空手伤害 = 基础值 + 饥饿比例。
pub const BASE_DAMAGE: f32 = 2.0;
/// 自然恢复生命所需的最低饥饿比例（不低于 50%）。
pub const REGEN_HUNGER_RATIO: f32 = 0.5;

/// 存档中的饥饿数据；异常值在载入时清洗。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedHunger {
    #[serde(default = "default_hunger")]
    pub hunger: f32,
    /// 饥饿上限；旧档缺少该字段时回落到默认值。
    #[serde(default = "default_max_hunger")]
    pub max: f32,
}

fn default_hunger() -> f32 {
    DEFAULT_MAX_HUNGER
}

fn default_max_hunger() -> f32 {
    DEFAULT_MAX_HUNGER
}

impl Default for SavedHunger {
    fn default() -> Self {
        Self {
            hunger: DEFAULT_MAX_HUNGER,
            max: DEFAULT_MAX_HUNGER,
        }
    }
}

impl SavedHunger {
    /// 清洗为 0–上限内的有限值；上限自身非法时回落到默认值。
    pub fn sanitized(self) -> Self {
        let max = sanitize_max(self.max);
        let hunger = if self.hunger.is_finite() {
            self.hunger.clamp(0.0, max)
        } else {
            max
        };
        Self { hunger, max }
    }
}

/// 饥饿运行时状态。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hunger {
    value: f32,
    max: f32,
    effect_seconds: f32,
}

impl Default for Hunger {
    fn default() -> Self {
        Self {
            value: DEFAULT_MAX_HUNGER,
            max: DEFAULT_MAX_HUNGER,
            effect_seconds: 0.0,
        }
    }
}

impl Hunger {
    pub fn new() -> Self {
        Self::default()
    }

    /// 从存档恢复；NaN 与越界值会被清洗。
    pub fn from_saved(saved: SavedHunger) -> Self {
        let saved = saved.sanitized();
        Self {
            value: saved.hunger,
            max: saved.max,
            effect_seconds: 0.0,
        }
    }

    pub fn to_saved(self) -> SavedHunger {
        SavedHunger {
            hunger: self.value,
            max: self.max,
        }
    }

    pub fn value(self) -> f32 {
        self.value
    }

    /// 当前饥饿上限。
    #[allow(dead_code)]
    pub fn max(self) -> f32 {
        self.max
    }

    /// 调整饥饿上限；当前值会被裁剪到新上限内。
    #[allow(dead_code)]
    pub fn set_max(&mut self, max: f32) {
        self.max = sanitize_max(max);
        self.value = self.value.clamp(0.0, self.max);
    }

    /// 饥饿百分比：以 100 点为 100%，超过 100 点时继续线性增长。
    pub fn ratio(self) -> f32 {
        (self.value / HUNGER_PERCENT_UNIT).max(0.0)
    }

    /// 饥饿值已达到当前上限。
    pub fn is_full(self) -> bool {
        self.value >= self.max
    }

    /// 是否允许自然恢复生命：不低于 50%。
    pub fn can_regenerate(self) -> bool {
        self.ratio() >= REGEN_HUNGER_RATIO
    }

    /// 基础行走速度（格/秒）：3 + 饥饿比例。
    pub fn walk_speed(self) -> f32 {
        BASE_WALK_SPEED + self.ratio()
    }

    /// 基础奔跑速度（格/秒）：3 + 3 × 饥饿比例。
    pub fn run_speed(self) -> f32 {
        BASE_WALK_SPEED + 3.0 * self.ratio()
    }

    /// 基础跳跃高度（格）：0.5 + 饥饿比例。
    pub fn jump_height(self) -> f32 {
        BASE_JUMP_HEIGHT + self.ratio()
    }

    /// 基础伤害：2 + 饥饿比例（2–3），空手攻击使用。
    pub fn base_damage(self) -> f32 {
        BASE_DAMAGE + self.ratio()
    }

    /// 饥饿状态是否生效；生效时体力消耗翻倍。
    pub fn hunger_effect_active(self) -> bool {
        self.effect_seconds > 0.0
    }

    /// 施加饥饿状态；重复施加时取较长的剩余时间。
    pub fn apply_hunger_effect(&mut self, seconds: f32) {
        if seconds.is_finite() && seconds > 0.0 {
            self.effect_seconds = self.effect_seconds.max(seconds);
        }
    }

    /// 更新饥饿状态剩余时间。
    pub fn tick_effect(&mut self, delta_seconds: f32) {
        if delta_seconds.is_finite() && delta_seconds > 0.0 {
            self.effect_seconds = (self.effect_seconds - delta_seconds).max(0.0);
        }
    }

    /// 直接扣除饥饿值。
    pub fn drain(&mut self, amount: f32) {
        if !amount.is_finite() || amount <= 0.0 {
            return;
        }
        self.value = (self.value - amount).clamp(0.0, self.max);
    }

    /// 进食恢复饥饿值，不会超过当前上限。
    pub fn eat(&mut self, hunger_restore: f32) {
        if hunger_restore.is_finite() && hunger_restore > 0.0 {
            self.value = (self.value + hunger_restore).clamp(0.0, self.max);
        }
    }
}

fn sanitize_max(value: f32) -> f32 {
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        DEFAULT_MAX_HUNGER
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_hunger_is_full() {
        let hunger = Hunger::new();
        assert_eq!(hunger.value(), DEFAULT_MAX_HUNGER);
        assert_eq!(hunger.max(), DEFAULT_MAX_HUNGER);
        assert!(hunger.is_full());
    }

    #[test]
    fn drain_and_eat_stay_within_bounds() {
        let mut hunger = Hunger::new();
        hunger.drain(30.0);
        assert_eq!(hunger.value(), 70.0);
        hunger.eat(50.0);
        assert_eq!(hunger.value(), DEFAULT_MAX_HUNGER);
        hunger.drain(500.0);
        assert_eq!(hunger.value(), 0.0);
    }

    #[test]
    fn regeneration_requires_at_least_half_hunger() {
        let mut hunger = Hunger::new();
        assert!(hunger.can_regenerate());
        hunger.drain(50.0);
        assert!(hunger.can_regenerate(), "恰好 50% 应允许恢复生命");
        hunger.drain(0.5);
        assert!(!hunger.can_regenerate());
    }

    #[test]
    fn speeds_and_jump_follow_hunger_ratio() {
        let mut hunger = Hunger::new();
        assert!((hunger.walk_speed() - 4.0).abs() < 1e-6);
        assert!((hunger.run_speed() - 6.0).abs() < 1e-6);
        assert!((hunger.jump_height() - 1.5).abs() < 1e-6);

        hunger.drain(50.0);
        assert!((hunger.walk_speed() - 3.5).abs() < 1e-6);
        assert!((hunger.run_speed() - 4.5).abs() < 1e-6);
        assert!((hunger.jump_height() - 1.0).abs() < 1e-6);

        hunger.drain(50.0);
        assert!((hunger.walk_speed() - 3.0).abs() < 1e-6);
        assert!((hunger.run_speed() - 3.0).abs() < 1e-6);
        assert!((hunger.jump_height() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn base_damage_follows_hunger_ratio() {
        let mut hunger = Hunger::new();
        assert!((hunger.base_damage() - 3.0).abs() < 1e-6);
        hunger.drain(50.0);
        assert!((hunger.base_damage() - 2.5).abs() < 1e-6);
        hunger.drain(50.0);
        assert!((hunger.base_damage() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn hunger_effect_expires() {
        let mut hunger = Hunger::new();
        hunger.apply_hunger_effect(8.0);
        assert!(hunger.hunger_effect_active());
        hunger.tick_effect(8.0);
        assert!(!hunger.hunger_effect_active());
    }

    #[test]
    fn from_saved_sanitizes_values() {
        let hunger = Hunger::from_saved(SavedHunger {
            hunger: f32::NAN,
            max: DEFAULT_MAX_HUNGER,
        });
        assert_eq!(hunger.value(), DEFAULT_MAX_HUNGER);
        let hunger = Hunger::from_saved(SavedHunger {
            hunger: 999.0,
            max: DEFAULT_MAX_HUNGER,
        });
        assert_eq!(hunger.value(), DEFAULT_MAX_HUNGER);
        let hunger = Hunger::from_saved(SavedHunger {
            hunger: -4.0,
            max: DEFAULT_MAX_HUNGER,
        });
        assert_eq!(hunger.value(), 0.0);
    }

    #[test]
    fn variable_max_allows_hunger_above_one_hundred() {
        let mut hunger = Hunger::new();
        hunger.set_max(150.0);
        hunger.eat(200.0);
        assert_eq!(hunger.value(), 150.0);
        assert_eq!(hunger.max(), 150.0);
        assert!((hunger.ratio() - 1.5).abs() < 1e-6);
        assert!((hunger.walk_speed() - 4.5).abs() < 1e-6);
        assert!((hunger.run_speed() - 7.5).abs() < 1e-6);
        assert!((hunger.jump_height() - 2.0).abs() < 1e-6);
        assert!((hunger.base_damage() - 3.5).abs() < 1e-6);
    }

    #[test]
    fn lowering_max_clamps_current_value() {
        let mut hunger = Hunger::new();
        hunger.set_max(40.0);
        assert_eq!(hunger.value(), 40.0);
        assert!(hunger.is_full());
    }

    #[test]
    fn saved_round_trip() {
        let mut hunger = Hunger::new();
        hunger.set_max(150.0);
        hunger.eat(50.0);
        hunger.drain(42.0);
        let restored = Hunger::from_saved(hunger.to_saved());
        assert_eq!(restored.value(), 108.0);
        assert_eq!(restored.max(), 150.0);
    }
}
