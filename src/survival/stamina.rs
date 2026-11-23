//! 体力：上限随当前饥饿值变化，行走/奔跑/静止持续消耗，耗尽后由饥饿补充。
//!
//! 体力不再自动恢复：行走每秒 1 点、奔跑每秒 2 点、静止每 2 分钟 1 点、
//! 每 2 次跳跃 1 点、攻击造成伤害 1 点、破坏方块每秒 1 点、自然恢复生命 20 点。
//! 体力归零时由 [`super::survival::Vitals`] 扣除 1 点饥饿并以新的上限补满。

use serde::{Deserialize, Serialize};

/// 默认体力上限；实际上限始终跟随当前饥饿值，可突破 100。
pub const DEFAULT_MAX_STAMINA: f32 = 100.0;
/// 行走每秒消耗。
pub const WALK_DRAIN_PER_SECOND: f32 = 1.0;
/// 奔跑每秒消耗。
pub const SPRINT_DRAIN_PER_SECOND: f32 = 2.0;
/// 静止每秒消耗：每 2 分钟 1 点。
pub const IDLE_DRAIN_PER_SECOND: f32 = 1.0 / 120.0;
/// 每次跳跃消耗：每 2 次跳跃 1 点。
pub const JUMP_COST: f32 = 0.5;
/// 攻击且造成伤害后的消耗。
pub const ATTACK_COST: f32 = 1.0;
/// 破坏方块时每累计 1 秒的消耗。
pub const MINING_DRAIN_PER_SECOND: f32 = 1.0;

/// 存档中的体力数据。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedStamina {
    #[serde(default = "default_stamina")]
    pub stamina: f32,
}

fn default_stamina() -> f32 {
    DEFAULT_MAX_STAMINA
}

impl Default for SavedStamina {
    fn default() -> Self {
        Self {
            stamina: DEFAULT_MAX_STAMINA,
        }
    }
}

impl SavedStamina {
    /// 清洗为 0–上限内的有限值；上界同时受调用方传入的当前上限约束。
    pub fn sanitized(self, max: f32) -> Self {
        let max = finite_max(max);
        let stamina = if self.stamina.is_finite() {
            self.stamina.clamp(0.0, max)
        } else {
            max
        };
        Self { stamina }
    }
}

/// 体力运行时状态。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stamina {
    value: f32,
    max: f32,
}

impl Default for Stamina {
    fn default() -> Self {
        Self {
            value: DEFAULT_MAX_STAMINA,
            max: DEFAULT_MAX_STAMINA,
        }
    }
}

impl Stamina {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_saved(saved: SavedStamina, max: f32) -> Self {
        let max = finite_max(max);
        Self {
            value: saved.sanitized(max).stamina,
            max,
        }
    }

    pub fn to_saved(self) -> SavedStamina {
        SavedStamina {
            stamina: self.value,
        }
    }

    pub fn value(self) -> f32 {
        self.value
    }

    pub fn max(self) -> f32 {
        self.max
    }

    /// 体力是否足以开始或维持冲刺。
    pub fn can_sprint(self) -> bool {
        self.value > 0.0
    }

    /// 同步体力上限；当前值会被裁剪到新上限内。
    pub fn set_max(&mut self, max: f32) {
        self.max = finite_max(max);
        self.value = self.value.clamp(0.0, self.max);
    }

    /// 恢复到当前上限（睡眠使用）。
    pub fn refill(&mut self) {
        self.value = self.max;
    }

    /// 按本帧状态消耗体力；返回本帧是否刚好耗尽。
    ///
    /// - `walking`：非奔跑的移动状态。
    /// - `sprinting`：奔跑状态，优先于行走。
    /// - `multiplier`：饥饿状态的消耗倍率（通常为 1 或 2）。
    pub fn update(
        &mut self,
        delta_seconds: f32,
        walking: bool,
        sprinting: bool,
        multiplier: f32,
    ) -> bool {
        if !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return false;
        }
        let rate = if sprinting {
            SPRINT_DRAIN_PER_SECOND
        } else if walking {
            WALK_DRAIN_PER_SECOND
        } else {
            IDLE_DRAIN_PER_SECOND
        };
        self.drain(rate * delta_seconds * finite_multiplier(multiplier))
    }

    /// 直接扣除体力；返回是否从有到无。
    pub fn drain(&mut self, amount: f32) -> bool {
        if !amount.is_finite() || amount <= 0.0 || self.value <= 0.0 {
            return false;
        }
        let before = self.value;
        self.value = (self.value - amount).max(0.0);
        before > 0.0 && self.value <= 0.0
    }
}

fn finite_max(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        DEFAULT_MAX_STAMINA
    }
}

fn finite_multiplier(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walking_drains_one_per_second() {
        let mut stamina = Stamina::new();
        assert!(!stamina.update(1.0, true, false, 1.0));
        assert!((stamina.value() - 99.0).abs() < 1e-6);
    }

    #[test]
    fn sprinting_drains_two_per_second() {
        let mut stamina = Stamina::new();
        stamina.update(1.0, true, true, 1.0);
        assert!((stamina.value() - 98.0).abs() < 1e-6);
    }

    #[test]
    fn idle_drains_one_per_two_minutes() {
        let mut stamina = Stamina::new();
        stamina.update(120.0, false, false, 1.0);
        assert!(stamina.value() < DEFAULT_MAX_STAMINA);
        assert!((stamina.value() - 99.0).abs() < 1e-4);
    }

    #[test]
    fn hunger_effect_doubles_drain() {
        let mut normal = Stamina::new();
        let mut hungry = Stamina::new();
        normal.update(1.0, true, false, 1.0);
        hungry.update(1.0, true, false, 2.0);
        assert!((normal.value() - hungry.value() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn depletion_is_reported_only_on_crossing() {
        let mut stamina = Stamina::new();
        stamina.set_max(1.0);
        stamina.value = 1.0;
        assert!(stamina.drain(1.0));
        assert!(!stamina.drain(1.0), "已经在零时不应重复报告耗尽");
        assert_eq!(stamina.value(), 0.0);
        assert!(!stamina.can_sprint());
    }

    #[test]
    fn max_tracks_hunger_and_clamps_value() {
        let mut stamina = Stamina::new();
        stamina.set_max(40.0);
        assert_eq!(stamina.max(), 40.0);
        assert_eq!(stamina.value(), 40.0);
        stamina.value = 10.0;
        stamina.set_max(0.0);
        assert_eq!(stamina.value(), 0.0);
    }

    #[test]
    fn two_jumps_cost_one_stamina() {
        assert!((JUMP_COST - 0.5).abs() < 1e-6);
        let mut stamina = Stamina::new();
        stamina.drain(JUMP_COST);
        assert!((stamina.value() - 99.5).abs() < 1e-6);
    }

    #[test]
    fn jump_cost_can_deplete_on_second_jump() {
        let mut stamina = Stamina::new();
        stamina.set_max(1.0);
        stamina.value = 1.0;
        assert!(!stamina.drain(JUMP_COST));
        assert!(stamina.drain(JUMP_COST));
    }

    #[test]
    fn saved_value_is_sanitized() {
        let stamina = Stamina::from_saved(SavedStamina { stamina: 999.0 }, DEFAULT_MAX_STAMINA);
        assert_eq!(stamina.value(), DEFAULT_MAX_STAMINA);
        let stamina = Stamina::from_saved(SavedStamina { stamina: f32::NAN }, DEFAULT_MAX_STAMINA);
        assert_eq!(stamina.value(), DEFAULT_MAX_STAMINA);
        let stamina = Stamina::from_saved(SavedStamina { stamina: 140.0 }, 150.0);
        assert_eq!(stamina.value(), 140.0);
        assert_eq!(stamina.max(), 150.0);
    }
}
