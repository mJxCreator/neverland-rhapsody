//! 饥饿与饱食度：自然消耗、进食恢复、饥饿状态与恢复判定。
//!
//! 饥饿值（0-20）与饱食度（0-20）使用浮点保存；饱食度永远不会高于饥饿值。
//! 所有消耗优先扣除饱食度，饱食度不足时再扣除饥饿值。

use serde::{Deserialize, Serialize};

/// 饥饿值上限。
pub const MAX_HUNGER: f32 = 20.0;
/// 饱食度上限。
pub const MAX_SATURATION: f32 = 20.0;
/// 新存档的初始饱食度。
pub const DEFAULT_SATURATION: f32 = 5.0;
/// 允许自然恢复所需的最低饥饿值。
pub const REGEN_MIN_HUNGER: f32 = 10.0;
/// 低饥饿阈值：低于该值时体力恢复减慢。
pub const LOW_HUNGER_THRESHOLD: f32 = 5.0;

/// 玩家当前活动强度，决定饥饿自然消耗速率。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Idle,
    Walking,
    Sprinting,
}

impl Activity {
    /// 由移动状态推导活动强度；冲刺优先于普通移动。
    pub fn from_motion(sprinting: bool, moving: bool) -> Self {
        if sprinting {
            Self::Sprinting
        } else if moving {
            Self::Walking
        } else {
            Self::Idle
        }
    }
}

/// 饥饿自然消耗速率；单位均为每秒（跳跃为每次）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HungerConfig {
    pub walk_drain_per_second: f32,
    pub sprint_drain_per_second: f32,
    pub jump_cost: f32,
}

impl Default for HungerConfig {
    fn default() -> Self {
        Self {
            walk_drain_per_second: 0.01,
            sprint_drain_per_second: 0.05,
            jump_cost: 0.02,
        }
    }
}

/// 存档中的饥饿数据；缺失字段按默认值处理，异常值在载入时清洗。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedHunger {
    #[serde(default = "default_hunger")]
    pub hunger: f32,
    #[serde(default = "default_saturation")]
    pub saturation: f32,
}

fn default_hunger() -> f32 {
    MAX_HUNGER
}

fn default_saturation() -> f32 {
    DEFAULT_SATURATION
}

impl Default for SavedHunger {
    fn default() -> Self {
        Self {
            hunger: MAX_HUNGER,
            saturation: DEFAULT_SATURATION,
        }
    }
}

/// 饥饿与饱食度运行时状态。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hunger {
    value: f32,
    saturation: f32,
    effect_seconds: f32,
    config: HungerConfig,
}

impl Default for Hunger {
    fn default() -> Self {
        Self {
            value: MAX_HUNGER,
            saturation: DEFAULT_SATURATION,
            effect_seconds: 0.0,
            config: HungerConfig::default(),
        }
    }
}

impl Hunger {
    pub fn new() -> Self {
        Self::default()
    }

    /// 从存档恢复；NaN 与越界值会被清洗。
    pub fn from_saved(saved: SavedHunger) -> Self {
        let mut hunger = Self::default();
        hunger.value = sanitize(saved.hunger, MAX_HUNGER, MAX_HUNGER);
        hunger.saturation = sanitize(saved.saturation, DEFAULT_SATURATION, MAX_SATURATION);
        hunger.saturation = hunger.saturation.min(hunger.value);
        hunger
    }

    pub fn to_saved(self) -> SavedHunger {
        SavedHunger {
            hunger: self.value,
            saturation: self.saturation,
        }
    }

    pub fn value(self) -> f32 {
        self.value
    }

    #[allow(dead_code)]
    pub fn saturation(self) -> f32 {
        self.saturation
    }

    /// 饥饿值与饱食度同时为满。
    pub fn is_full(self) -> bool {
        self.value >= MAX_HUNGER && self.saturation >= MAX_SATURATION
    }

    /// 饥饿值是否足以自然恢复生命。
    pub fn can_regenerate(self) -> bool {
        self.value >= REGEN_MIN_HUNGER
    }

    /// 饥饿值是否允许冲刺。
    pub fn can_sprint(self) -> bool {
        self.value > 0.0
    }

    /// 体力恢复倍率；低饥饿时减半。
    pub fn regeneration_scale(self) -> f32 {
        if self.value < LOW_HUNGER_THRESHOLD {
            0.5
        } else {
            1.0
        }
    }

    /// 饥饿状态是否生效。
    pub fn hunger_effect_active(self) -> bool {
        self.effect_seconds > 0.0
    }

    #[allow(dead_code)]
    pub fn effect_remaining(self) -> f32 {
        self.effect_seconds
    }

    /// 施加饥饿状态；重复施加时取较长的剩余时间。
    pub fn apply_hunger_effect(&mut self, seconds: f32) {
        if seconds.is_finite() && seconds > 0.0 {
            self.effect_seconds = self.effect_seconds.max(seconds);
        }
    }

    /// 按活动强度扣除饥饿消耗；饥饿状态期间消耗翻倍。
    pub fn update(&mut self, delta_seconds: f32, activity: Activity) {
        if !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return;
        }
        let rate = match activity {
            Activity::Idle => 0.0,
            Activity::Walking => self.config.walk_drain_per_second,
            Activity::Sprinting => self.config.sprint_drain_per_second,
        };
        let multiplier = if self.hunger_effect_active() {
            2.0
        } else {
            1.0
        };
        self.drain(rate * delta_seconds * multiplier);
    }

    /// 跳跃的额外消耗。
    #[allow(dead_code)]
    pub fn on_jump(&mut self) {
        let multiplier = if self.hunger_effect_active() {
            2.0
        } else {
            1.0
        };
        self.drain(self.config.jump_cost * multiplier);
    }

    /// 更新饥饿状态剩余时间。
    pub fn tick_effect(&mut self, delta_seconds: f32) {
        if delta_seconds.is_finite() && delta_seconds > 0.0 {
            self.effect_seconds = (self.effect_seconds - delta_seconds).max(0.0);
        }
    }

    /// 直接扣除饥饿消耗：优先饱食度，再饥饿值。
    pub fn drain(&mut self, amount: f32) {
        if !amount.is_finite() || amount <= 0.0 {
            return;
        }
        let from_saturation = amount.min(self.saturation).min(self.value);
        self.saturation -= from_saturation;
        let remaining = amount - from_saturation;
        self.value = (self.value - remaining).max(0.0);
        self.saturation = self.saturation.min(self.value).max(0.0);
    }

    /// 进食恢复；饱食度不得超过饥饿值。
    pub fn eat(&mut self, hunger_restore: f32, saturation_restore: f32) {
        if hunger_restore.is_finite() && hunger_restore > 0.0 {
            self.value = (self.value + hunger_restore).clamp(0.0, MAX_HUNGER);
        }
        if saturation_restore.is_finite() && saturation_restore > 0.0 {
            self.saturation = (self.saturation + saturation_restore)
                .clamp(0.0, MAX_SATURATION)
                .min(self.value);
        }
    }
}

fn sanitize(value: f32, fallback: f32, max: f32) -> f32 {
    if !value.is_finite() {
        fallback
    } else {
        value.clamp(0.0, max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saturation_never_exceeds_hunger() {
        let mut hunger = Hunger::new();
        hunger.drain(18.0);
        assert!(hunger.saturation() <= hunger.value());
    }

    #[test]
    fn drain_prefers_saturation() {
        let mut hunger = Hunger::new();
        hunger.saturation = 5.0;
        hunger.drain(3.0);
        assert_eq!(hunger.saturation(), 2.0);
        assert_eq!(hunger.value(), MAX_HUNGER);
    }

    #[test]
    fn drain_spills_into_hunger() {
        let mut hunger = Hunger::new();
        hunger.saturation = 2.0;
        hunger.drain(5.0);
        assert_eq!(hunger.saturation(), 0.0);
        assert_eq!(hunger.value(), MAX_HUNGER - 3.0);
    }

    #[test]
    fn eating_restores_and_caps_saturation() {
        let mut hunger = Hunger::new();
        hunger.drain(20.0);
        assert_eq!(hunger.value(), 5.0);
        assert_eq!(hunger.saturation(), 0.0);
        hunger.eat(8.0, 20.0);
        assert_eq!(hunger.value(), 13.0);
        assert_eq!(hunger.saturation(), 13.0);
    }

    #[test]
    fn eating_full_is_still_bounded() {
        let mut hunger = Hunger::new();
        hunger.saturation = MAX_SATURATION;
        hunger.eat(5.0, 5.0);
        assert_eq!(hunger.value(), MAX_HUNGER);
        assert_eq!(hunger.saturation(), MAX_SATURATION);
        assert!(hunger.is_full());
    }

    #[test]
    fn walk_drains_and_idle_does_not() {
        let mut hunger = Hunger::new();
        hunger.update(10.0, Activity::Idle);
        assert_eq!(hunger.value(), MAX_HUNGER);
        assert_eq!(hunger.saturation(), DEFAULT_SATURATION);
        hunger.update(10.0, Activity::Walking);
        assert!(hunger.saturation() < DEFAULT_SATURATION, "行走应消耗饱食度");
    }

    #[test]
    fn hunger_effect_doubles_drain_and_expires() {
        let mut hunger = Hunger::new();
        hunger.drain(5.0);
        hunger.apply_hunger_effect(8.0);
        let before = hunger.value();
        hunger.update(1.0, Activity::Sprinting);
        assert!((hunger.value() - (before - 0.1)).abs() < 1e-5);
        hunger.tick_effect(8.0);
        assert!(!hunger.hunger_effect_active());
    }

    #[test]
    fn low_hunger_regen_scale_is_halved() {
        let mut hunger = Hunger::new();
        hunger.drain(21.0);
        assert!(hunger.value() < LOW_HUNGER_THRESHOLD);
        assert_eq!(hunger.regeneration_scale(), 0.5);
        assert!(!hunger.can_regenerate());
    }

    #[test]
    fn from_saved_sanitizes_values() {
        let hunger = Hunger::from_saved(SavedHunger {
            hunger: f32::NAN,
            saturation: 99.0,
        });
        assert_eq!(hunger.value(), MAX_HUNGER);
        assert_eq!(hunger.saturation(), MAX_HUNGER);
    }

    #[test]
    fn from_saved_clamps_saturation_below_hunger() {
        let hunger = Hunger::from_saved(SavedHunger {
            hunger: 3.0,
            saturation: 10.0,
        });
        assert_eq!(hunger.value(), 3.0);
        assert_eq!(hunger.saturation(), 3.0);
    }

    #[test]
    fn activity_mapping_priority() {
        assert_eq!(Activity::from_motion(true, false), Activity::Sprinting);
        assert_eq!(Activity::from_motion(false, true), Activity::Walking);
        assert_eq!(Activity::from_motion(false, false), Activity::Idle);
    }
}
