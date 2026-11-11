//! 世界时间：昼夜阶段与只读展示信息。
//!
//! 时间以“一天中的比例”保存到 `[0, 1)`，与实际秒数解耦；游戏暂停、死亡或
//! 窗口失焦时调用方停止推进，因此不会因掉帧或暂停异常流逝。

use serde::{Deserialize, Serialize};

/// 一个完整昼夜的时长（秒）。
pub const DAY_LENGTH_SECONDS: f32 = 600.0;

/// 黎明起点；之后到周期结束为黎明。
const DAWN_START: f32 = 0.90;
/// 夜晚起点。
const NIGHT_START: f32 = 0.55;
/// 黄昏起点。
const DUSK_START: f32 = 0.45;

/// 昼夜阶段。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimePhase {
    Day,
    Dusk,
    Night,
    Dawn,
}

impl TimePhase {
    /// 展示用英文标签；字体仅覆盖 ASCII。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Day => "DAY",
            Self::Dusk => "DUSK",
            Self::Night => "NIGHT",
            Self::Dawn => "DAWN",
        }
    }

    /// 是否允许睡眠。
    pub const fn allows_sleep(self) -> bool {
        matches!(self, Self::Night)
    }
}

/// 存档中的世界时间。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedWorldTime {
    #[serde(default)]
    pub time_of_day: f32,
}

impl Default for SavedWorldTime {
    fn default() -> Self {
        Self { time_of_day: 0.0 }
    }
}

/// 世界时间运行时状态。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldTime {
    fraction: f32,
}

impl Default for WorldTime {
    fn default() -> Self {
        Self { fraction: 0.0 }
    }
}

impl WorldTime {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_saved(saved: SavedWorldTime) -> Self {
        Self {
            fraction: normalize(saved.time_of_day),
        }
    }

    pub fn to_saved(self) -> SavedWorldTime {
        SavedWorldTime {
            time_of_day: self.fraction,
        }
    }

    /// 一天中的比例，范围 `[0, 1)`。
    #[allow(dead_code)]
    pub fn fraction(self) -> f32 {
        self.fraction
    }

    pub fn phase(self) -> TimePhase {
        if self.fraction < DUSK_START {
            TimePhase::Day
        } else if self.fraction < NIGHT_START {
            TimePhase::Dusk
        } else if self.fraction < DAWN_START {
            TimePhase::Night
        } else {
            TimePhase::Dawn
        }
    }

    pub fn is_night(self) -> bool {
        self.phase().allows_sleep()
    }

    /// 推进时间；每秒推进 `1 / DAY_LENGTH_SECONDS`。
    pub fn advance(&mut self, delta_seconds: f32) {
        if !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return;
        }
        self.fraction = normalize(self.fraction + delta_seconds / DAY_LENGTH_SECONDS);
    }

    /// 睡眠后推进到次日清晨。
    pub fn advance_to_morning(&mut self) {
        self.fraction = 0.0;
    }
}

fn normalize(value: f32) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    value.rem_euclid(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_day() {
        let time = WorldTime::new();
        assert_eq!(time.phase(), TimePhase::Day);
        assert_eq!(time.fraction(), 0.0);
    }

    #[test]
    fn phases_follow_fraction_boundaries() {
        let mut time = WorldTime::new();
        time.fraction = 0.45;
        assert_eq!(time.phase(), TimePhase::Dusk);
        time.fraction = 0.55;
        assert_eq!(time.phase(), TimePhase::Night);
        assert!(time.is_night());
        time.fraction = 0.90;
        assert_eq!(time.phase(), TimePhase::Dawn);
        assert!(!time.is_night());
    }

    #[test]
    fn advance_wraps_within_one_day() {
        let mut time = WorldTime::new();
        time.fraction = 0.99;
        time.advance(DAY_LENGTH_SECONDS * 0.02);
        assert!((time.fraction() - 0.01).abs() < 1e-4);
    }

    #[test]
    fn sleep_jumps_to_morning() {
        let mut time = WorldTime::new();
        time.fraction = 0.7;
        time.advance_to_morning();
        assert_eq!(time.phase(), TimePhase::Day);
        assert_eq!(time.fraction(), 0.0);
    }

    #[test]
    fn saved_time_is_normalized() {
        let time = WorldTime::from_saved(SavedWorldTime { time_of_day: -0.25 });
        assert!((time.fraction() - 0.75).abs() < 1e-4);
        let time = WorldTime::from_saved(SavedWorldTime {
            time_of_day: f32::NAN,
        });
        assert_eq!(time.fraction(), 0.0);
    }

    #[test]
    fn day_length_is_ten_minutes() {
        let mut time = WorldTime::new();
        time.advance(DAY_LENGTH_SECONDS);
        assert_eq!(time.fraction(), 0.0);
    }

    #[test]
    fn labels_are_ascii() {
        for phase in [
            TimePhase::Day,
            TimePhase::Dusk,
            TimePhase::Night,
            TimePhase::Dawn,
        ] {
            assert!(phase.label().is_ascii());
        }
    }
}
