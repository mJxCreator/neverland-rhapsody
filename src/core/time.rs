//! 帧计时：帧间隔与每秒平均帧率。

use std::time::{Duration, Instant};

/// 一次帧推进的统计结果。
#[derive(Debug, Clone, Copy)]
pub struct FrameStats {
    /// 与上一帧的时间间隔。
    pub delta: Duration,
    /// 最近一秒的平均帧率，仅在完成一次秒级统计时为 `Some`。
    pub fps: Option<f32>,
}

/// 记录帧边界并统计帧率。
pub struct FrameTimer {
    last_frame: Instant,
    window_start: Instant,
    frames_in_window: u32,
}

impl FrameTimer {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            last_frame: now,
            window_start: now,
            frames_in_window: 0,
        }
    }

    /// 记录一帧并返回帧间隔；跨过一秒边界时同时给出平均帧率。
    pub fn tick(&mut self) -> FrameStats {
        let now = Instant::now();
        let delta = now - self.last_frame;
        self.last_frame = now;
        self.frames_in_window += 1;

        let elapsed = now - self.window_start;
        let fps = if elapsed >= Duration::from_secs(1) {
            let fps = self.frames_in_window as f32 / elapsed.as_secs_f32();
            self.frames_in_window = 0;
            self.window_start = now;
            Some(fps)
        } else {
            None
        };

        FrameStats { delta, fps }
    }
}

impl Default for FrameTimer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_tick_has_delta_but_no_fps() {
        let mut timer = FrameTimer::new();
        let stats = timer.tick();
        assert!(stats.delta < Duration::from_secs(1));
        assert_eq!(stats.fps, None);
    }

    #[test]
    fn fps_is_reported_after_one_second_of_frames() {
        let mut timer = FrameTimer::new();
        timer.window_start = Instant::now() - Duration::from_secs(1);
        timer.frames_in_window = 59;
        let stats = timer.tick();
        let fps = stats.fps.expect("跨过一秒边界后应给出帧率");
        assert!(fps > 0.0);
        assert_eq!(timer.frames_in_window, 0);
    }
}
