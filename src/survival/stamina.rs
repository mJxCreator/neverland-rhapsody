//! 体力：冲刺消耗、停止冲刺后的延迟恢复与低饥饿恢复减半。
//!
//! 体力上限固定为 100。冲刺期间持续消耗；停止冲刺后经过配置的延迟才开始
//! 恢复，静止恢复快于行走。体力耗尽时不允许继续冲刺。

use serde::{Deserialize, Serialize};

/// 体力上限。
pub const MAX_STAMINA: f32 = 100.0;

/// 体力数值配置。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StaminaConfig {
    pub max: f32,
    pub sprint_drain_per_second: f32,
    pub idle_regen_per_second: f32,
    pub walk_regen_per_second: f32,
    pub regen_delay: f32,
    pub sprint_speed_multiplier: f32,
}

impl Default for StaminaConfig {
    fn default() -> Self {
        Self {
            max: MAX_STAMINA,
            sprint_drain_per_second: 15.0,
            idle_regen_per_second: 20.0,
            walk_regen_per_second: 10.0,
            regen_delay: 0.8,
            sprint_speed_multiplier: 1.5,
        }
    }
}

/// 存档中的体力数据。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedStamina {
    #[serde(default = "default_stamina")]
    pub stamina: f32,
}

fn default_stamina() -> f32 {
    MAX_STAMINA
}

impl Default for SavedStamina {
    fn default() -> Self {
        Self {
            stamina: MAX_STAMINA,
        }
    }
}

/// 体力运行时状态。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stamina {
    value: f32,
    regen_delay_remaining: f32,
    config: StaminaConfig,
}

impl Default for Stamina {
    fn default() -> Self {
        Self {
            value: MAX_STAMINA,
            regen_delay_remaining: 0.0,
            config: StaminaConfig::default(),
        }
    }
}

impl Stamina {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_saved(saved: SavedStamina) -> Self {
        let mut stamina = Self::default();
        stamina.value = if saved.stamina.is_finite() {
            saved.stamina.clamp(0.0, MAX_STAMINA)
        } else {
            MAX_STAMINA
        };
        stamina
    }

    pub fn to_saved(self) -> SavedStamina {
        SavedStamina {
            stamina: self.value,
        }
    }

    pub fn value(self) -> f32 {
        self.value
    }

    #[allow(dead_code)]
    pub fn max(self) -> f32 {
        self.config.max
    }

    #[allow(dead_code)]
    pub fn ratio(self) -> f32 {
        (self.value / self.config.max).clamp(0.0, 1.0)
    }

    /// 体力是否足以开始或维持冲刺。
    pub fn can_sprint(self) -> bool {
        self.value > 0.0
    }

    /// 冲刺速度倍率。
    #[allow(dead_code)]
    pub fn sprint_multiplier(self) -> f32 {
        self.config.sprint_speed_multiplier
    }

    /// 恢复到满体力（睡眠使用）。
    pub fn refill(&mut self) {
        self.value = self.config.max;
        self.regen_delay_remaining = 0.0;
    }

    /// 扣减一次冲刺消耗；体力归零后由调用方停止冲刺。
    pub fn drain_sprint(&mut self, delta_seconds: f32) {
        if !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return;
        }
        self.value = (self.value - self.config.sprint_drain_per_second * delta_seconds).max(0.0);
        self.regen_delay_remaining = self.config.regen_delay;
    }

    /// 按本帧状态更新体力。
    ///
    /// - `sprinting`：本帧是否实际冲刺（消耗体力并重置恢复延迟）。
    /// - `moving`：是否在移动（决定行走或静止恢复速率）。
    /// - `regen_scale`：饥饿带来的恢复倍率。
    ///
    /// 界面阻塞、暂停与死亡期间调用方不应调用本方法，从而保持体力不变。
    pub fn update(&mut self, delta_seconds: f32, sprinting: bool, moving: bool, regen_scale: f32) {
        let mut delta_seconds = delta_seconds;
        if !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return;
        }
        if sprinting {
            self.drain_sprint(delta_seconds);
            return;
        }
        if self.regen_delay_remaining > 0.0 {
            let consumed = delta_seconds.min(self.regen_delay_remaining);
            self.regen_delay_remaining -= consumed;
            delta_seconds -= consumed;
            if delta_seconds <= 0.0 {
                return;
            }
        }
        let rate = if moving {
            self.config.walk_regen_per_second
        } else {
            self.config.idle_regen_per_second
        };
        let scale = if regen_scale.is_finite() {
            regen_scale.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.value = (self.value + rate * scale * delta_seconds).min(self.config.max);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprint_drains_and_resets_delay() {
        let mut stamina = Stamina::new();
        stamina.update(1.0, true, true, 1.0);
        assert_eq!(stamina.value(), MAX_STAMINA - 15.0);
        assert!(stamina.regen_delay_remaining > 0.0);
    }

    #[test]
    fn regen_waits_for_delay() {
        let mut stamina = Stamina::new();
        stamina.value = 50.0;
        stamina.drain_sprint(1.0);
        let after_drain = stamina.value();
        stamina.update(0.5, false, false, 1.0);
        assert_eq!(stamina.value(), after_drain);
        stamina.update(0.4, false, false, 1.0);
        assert!(stamina.value() > after_drain);
    }

    #[test]
    fn idle_regens_faster_than_walking() {
        let mut idle = Stamina::new();
        idle.value = 10.0;
        let mut walking = idle;
        idle.update(1.0, false, false, 1.0);
        walking.update(1.0, false, true, 1.0);
        assert!(idle.value() > walking.value());
    }

    #[test]
    fn low_hunger_scales_regen() {
        let mut stamina = Stamina::new();
        stamina.value = 10.0;
        let mut scaled = stamina;
        stamina.update(1.0, false, false, 1.0);
        scaled.update(1.0, false, false, 0.5);
        assert!((stamina.value() - scaled.value() - 10.0).abs() < 1e-4);
    }

    #[test]
    fn sprint_stops_at_zero() {
        let mut stamina = Stamina::new();
        stamina.value = 1.0;
        stamina.drain_sprint(1.0);
        assert_eq!(stamina.value(), 0.0);
        assert!(!stamina.can_sprint());
    }

    #[test]
    fn saved_value_is_sanitized() {
        let stamina = Stamina::from_saved(SavedStamina { stamina: 999.0 });
        assert_eq!(stamina.value(), MAX_STAMINA);
        let stamina = Stamina::from_saved(SavedStamina { stamina: f32::NAN });
        assert_eq!(stamina.value(), MAX_STAMINA);
    }
}
