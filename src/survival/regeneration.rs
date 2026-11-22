//! 自然恢复：饥饿值不低于 50% 且未满血时缓慢恢复生命。
//!
//! 恢复间隔随饥饿值变化：每 `1000 / hunger` 秒恢复 1 点生命值。饥饿不低于
//! 50% 时饥饿越多恢复越快；每次恢复额外消耗 20 点体力。

/// 每次自然恢复的生命值。
pub const NAT_HEART_RECOVER: u32 = 1;
/// 恢复间隔公式的分子。
pub const REGEN_SECONDS_NUMERATOR: f32 = 1000.0;
/// 自然恢复 1 点生命所消耗的体力。
pub const HEAL_STAMINA_COST: f32 = 20.0;

/// 按当前饥饿值计算恢复 1 点生命所需的秒数；饥饿为 0 时视为无穷大。
pub fn interval_for(hunger: f32) -> f32 {
    if hunger.is_finite() && hunger > 0.0 {
        REGEN_SECONDS_NUMERATOR / hunger.min(crate::survival::hunger::MAX_HUNGER)
    } else {
        f32::INFINITY
    }
}

/// 恢复计时器；只有满足条件时才累计时间。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Regenerator {
    timer: f32,
}

impl Regenerator {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)]
    pub fn timer(self) -> f32 {
        self.timer
    }

    /// 推进计时；满足条件且到达当前间隔时返回 `true`，并保留余数。
    pub fn tick(&mut self, delta_seconds: f32, eligible: bool, hunger: f32) -> bool {
        if !eligible {
            self.timer = 0.0;
            return false;
        }
        if !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return false;
        }
        let interval = interval_for(hunger);
        if !interval.is_finite() {
            self.timer = 0.0;
            return false;
        }
        self.timer += delta_seconds;
        if self.timer >= interval {
            self.timer -= interval;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_shrinks_as_hunger_rises() {
        assert!((interval_for(100.0) - 10.0).abs() < 1e-6);
        assert!((interval_for(50.0) - 20.0).abs() < 1e-6);
        assert!(!interval_for(0.0).is_finite());
    }

    #[test]
    fn heals_only_after_the_current_interval() {
        let mut regen = Regenerator::new();
        assert!(!regen.tick(9.9, true, 100.0));
        assert!(regen.tick(0.2, true, 100.0));
    }

    #[test]
    fn higher_hunger_heals_faster() {
        let mut slow = Regenerator::new();
        let mut fast = Regenerator::new();
        assert!(!slow.tick(10.0, true, 51.0));
        assert!(fast.tick(10.0, true, 100.0));
    }

    #[test]
    fn ineligible_resets_timer() {
        let mut regen = Regenerator::new();
        assert!(!regen.tick(9.0, true, 100.0));
        assert!(!regen.tick(1.0, false, 100.0));
        assert_eq!(regen.timer(), 0.0);
        assert!(!regen.tick(9.0, true, 100.0));
    }
}
