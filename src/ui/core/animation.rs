//! UI 动画：短时反馈动画与基于时间的脉冲，避免各界面重复维护计时状态。
//!
//! 所有动画都是纯数据，不依赖系统时钟；由上层每帧传入 `delta_secs` 推进，
//! 因此可以稳定地在测试中验证。

/// 一次性短动画：从 0 到 1 后保持结束状态。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiAnimation {
    remaining: f32,
    duration: f32,
}

impl UiAnimation {
    /// 启动一个持续 `duration` 秒的动画；非法时长按 0 处理。
    pub fn start(&mut self, duration: f32) {
        self.duration = if duration.is_finite() && duration > 0.0 {
            duration
        } else {
            0.0
        };
        self.remaining = self.duration;
    }

    /// 推进动画；返回是否仍在播放。
    pub fn update(&mut self, delta_secs: f32) {
        if !delta_secs.is_finite() || delta_secs <= 0.0 {
            return;
        }
        self.remaining = (self.remaining - delta_secs).max(0.0);
    }

    /// 动画是否仍在播放。
    pub fn is_active(self) -> bool {
        self.remaining > 0.0
    }

    /// 从 1 衰减到 0 的强度，用于按压/成功闪光；未启动时返回 1。
    pub fn decay(self) -> f32 {
        if self.duration <= 0.0 {
            return 1.0;
        }
        (self.remaining / self.duration).clamp(0.0, 1.0)
    }
}

/// 持续计时器：用于选中格的循环脉冲等周期性表现。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiClock {
    elapsed: f32,
}

impl UiClock {
    /// 持续推进时钟。
    pub fn update(&mut self, delta_secs: f32) {
        if delta_secs.is_finite() && delta_secs > 0.0 {
            self.elapsed = (self.elapsed + delta_secs).rem_euclid(3600.0);
        }
    }

    /// 返回 `0..=1` 的平滑脉冲，周期由 `speed` 控制。
    pub fn pulse(self, speed: f32) -> f32 {
        let phase = self.elapsed * speed.max(0.0);
        0.5 + 0.5 * phase.sin()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animation_reaches_completion() {
        let mut animation = UiAnimation::default();
        animation.start(0.2);
        assert!(animation.is_active());
        assert_eq!(animation.decay(), 1.0);
        animation.update(0.1);
        assert!((animation.decay() - 0.5).abs() < 0.001);
        animation.update(0.2);
        assert!(!animation.is_active());
        assert_eq!(animation.decay(), 0.0);
    }

    #[test]
    fn invalid_durations_are_safe() {
        let mut animation = UiAnimation::default();
        animation.start(f32::NAN);
        animation.update(f32::NAN);
        assert!(!animation.is_active());
        assert_eq!(animation.decay(), 1.0);
    }

    #[test]
    fn clock_pulse_stays_in_unit_range() {
        let mut clock = UiClock::default();
        for _ in 0..100 {
            clock.update(0.017);
            let pulse = clock.pulse(6.0);
            assert!((0.0..=1.0).contains(&pulse));
        }
    }
}
