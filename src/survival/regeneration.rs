//! 自然恢复：饥饿值充足且未满血时，按固定间隔恢复生命并消耗饥饿值。

/// 两次自然恢复之间的间隔（秒）。
pub const REGEN_INTERVAL: f32 = 4.0;
/// 每次自然恢复消耗的饥饿值。
pub const REGEN_HUNGER_COST: f32 = 1.0;

/// 自然恢复参数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegenConfig {
    pub interval: f32,
    pub hunger_cost: f32,
}

impl Default for RegenConfig {
    fn default() -> Self {
        Self {
            interval: REGEN_INTERVAL,
            hunger_cost: REGEN_HUNGER_COST,
        }
    }
}

/// 恢复计时器；只有满足条件时才累计时间。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Regenerator {
    timer: f32,
    config: RegenConfig,
}

impl Default for Regenerator {
    fn default() -> Self {
        Self {
            timer: 0.0,
            config: RegenConfig::default(),
        }
    }
}

impl Regenerator {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)]
    pub fn timer(self) -> f32 {
        self.timer
    }

    pub fn hunger_cost(self) -> f32 {
        self.config.hunger_cost
    }

    /// 推进计时；满足条件且到达间隔时返回 `true`，并保留余数。
    pub fn tick(&mut self, delta_seconds: f32, eligible: bool) -> bool {
        if !eligible {
            self.timer = 0.0;
            return false;
        }
        if !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return false;
        }
        self.timer += delta_seconds;
        if self.timer >= self.config.interval {
            self.timer -= self.config.interval;
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
    fn heals_only_after_interval() {
        let mut regen = Regenerator::new();
        assert!(!regen.tick(3.9, true));
        assert!(regen.tick(0.2, true));
    }

    #[test]
    fn ineligible_resets_timer() {
        let mut regen = Regenerator::new();
        assert!(!regen.tick(3.0, true));
        assert!(!regen.tick(1.0, false));
        assert_eq!(regen.timer(), 0.0);
        assert!(!regen.tick(3.0, true));
    }

    #[test]
    fn large_delta_keeps_remainder() {
        let mut regen = Regenerator::new();
        assert!(regen.tick(9.0, true));
        assert!((regen.timer() - 5.0).abs() < 1e-5);
    }
}
