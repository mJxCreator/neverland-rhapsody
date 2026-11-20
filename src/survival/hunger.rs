//! 饥饿值（0–100）：进食恢复、体力耗尽时的饥饿消耗与基于饥饿的比例派生。
//!
//! 饱和度机制已移除。饥饿只通过“体力耗尽一轮扣 1 点”下降，进食按食物定义
//! 直接恢复。行走/奔跑速度与跳跃高度都由当前饥饿比例派生。

use serde::{Deserialize, Serialize};

/// 饥饿值上限，同时是速度与跳跃高度的基准量程。
pub const MAX_HUNGER: f32 = 100.0;
/// 基础行走速度（格/秒）：实际速度 = 基础值 + 饥饿比例。
pub const BASE_WALK_SPEED: f32 = 3.0;
/// 基础跳跃高度（格）：实际高度 = 基础值 + 饥饿比例。
pub const BASE_JUMP_HEIGHT: f32 = 0.5;
/// 自然恢复生命所需的最低饥饿比例（严格大于 50%）。
pub const REGEN_HUNGER_RATIO: f32 = 0.5;

/// 存档中的饥饿数据；异常值在载入时清洗。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedHunger {
    #[serde(default = "default_hunger")]
    pub hunger: f32,
}

fn default_hunger() -> f32 {
    MAX_HUNGER
}

impl Default for SavedHunger {
    fn default() -> Self {
        Self { hunger: MAX_HUNGER }
    }
}

impl SavedHunger {
    /// 清洗为 0–上限内的有限值。
    pub fn sanitized(self) -> Self {
        Self {
            hunger: sanitize(self.hunger),
        }
    }
}

/// 饥饿运行时状态。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hunger {
    value: f32,
    effect_seconds: f32,
}

impl Default for Hunger {
    fn default() -> Self {
        Self {
            value: MAX_HUNGER,
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
        Self {
            value: saved.sanitized().hunger,
            effect_seconds: 0.0,
        }
    }

    pub fn to_saved(self) -> SavedHunger {
        SavedHunger { hunger: self.value }
    }

    pub fn value(self) -> f32 {
        self.value
    }

    /// 0–1 的饥饿比例。
    pub fn ratio(self) -> f32 {
        (self.value / MAX_HUNGER).clamp(0.0, 1.0)
    }

    /// 饥饿值已满。
    pub fn is_full(self) -> bool {
        self.value >= MAX_HUNGER
    }

    /// 是否允许自然恢复生命：严格高于 50%。
    pub fn can_regenerate(self) -> bool {
        self.ratio() > REGEN_HUNGER_RATIO
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
        self.value = (self.value - amount).clamp(0.0, MAX_HUNGER);
    }

    /// 进食恢复饥饿值，不会超过上限。
    pub fn eat(&mut self, hunger_restore: f32) {
        if hunger_restore.is_finite() && hunger_restore > 0.0 {
            self.value = (self.value + hunger_restore).clamp(0.0, MAX_HUNGER);
        }
    }
}

fn sanitize(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, MAX_HUNGER)
    } else {
        MAX_HUNGER
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_hunger_is_full() {
        let hunger = Hunger::new();
        assert_eq!(hunger.value(), MAX_HUNGER);
        assert!(hunger.is_full());
    }

    #[test]
    fn drain_and_eat_stay_within_bounds() {
        let mut hunger = Hunger::new();
        hunger.drain(30.0);
        assert_eq!(hunger.value(), 70.0);
        hunger.eat(50.0);
        assert_eq!(hunger.value(), MAX_HUNGER);
        hunger.drain(500.0);
        assert_eq!(hunger.value(), 0.0);
    }

    #[test]
    fn regeneration_requires_more_than_half_hunger() {
        let mut hunger = Hunger::new();
        assert!(hunger.can_regenerate());
        hunger.drain(50.0);
        assert!(!hunger.can_regenerate(), "恰好 50% 不应恢复生命");
        hunger.eat(1.0);
        assert!(hunger.can_regenerate());
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
    fn hunger_effect_expires() {
        let mut hunger = Hunger::new();
        hunger.apply_hunger_effect(8.0);
        assert!(hunger.hunger_effect_active());
        hunger.tick_effect(8.0);
        assert!(!hunger.hunger_effect_active());
    }

    #[test]
    fn from_saved_sanitizes_values() {
        let hunger = Hunger::from_saved(SavedHunger { hunger: f32::NAN });
        assert_eq!(hunger.value(), MAX_HUNGER);
        let hunger = Hunger::from_saved(SavedHunger { hunger: 999.0 });
        assert_eq!(hunger.value(), MAX_HUNGER);
        let hunger = Hunger::from_saved(SavedHunger { hunger: -4.0 });
        assert_eq!(hunger.value(), 0.0);
    }

    #[test]
    fn saved_round_trip() {
        let mut hunger = Hunger::new();
        hunger.drain(42.0);
        assert_eq!(Hunger::from_saved(hunger.to_saved()).value(), 58.0);
    }
}
