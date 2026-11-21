//! 昼夜阶段文字模块：右上角显示 `TIME <阶段>`。

use crate::ui::core::draw::UiVertex;
use crate::ui::core::font;
use crate::ui::core::theme;

/// 昼夜文字参数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimePhaseConfig {
    /// 距屏幕右边缘的距离（像素）。
    pub margin_right: f32,
    /// 距屏幕顶部的距离（像素）。
    pub margin_top: f32,
    /// 字号（像素格）。
    pub pixel: f32,
    /// 文字颜色。
    pub color: [f32; 4],
}

impl Default for TimePhaseConfig {
    fn default() -> Self {
        Self {
            margin_right: 18.0,
            margin_top: 14.0,
            pixel: 1.8,
            color: theme::TIME_TEXT,
        }
    }
}

/// 昼夜阶段文字状态。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimePhase {
    /// 当前阶段标签。
    pub label: &'static str,
    /// 可调参数。
    pub config: TimePhaseConfig,
}

impl Default for TimePhase {
    fn default() -> Self {
        Self {
            label: "DAY",
            config: TimePhaseConfig::default(),
        }
    }
}

impl TimePhase {
    /// 绘制阶段文字。
    pub fn draw(&self, out: &mut Vec<UiVertex>, width: f32, height: f32) {
        let config = &self.config;
        let label = format!("TIME {}", self.label);
        let text_width = font::text_width(&label, config.pixel);
        font::push_text(
            out,
            (width, height),
            width - text_width - config.margin_right,
            config.margin_top,
            config.pixel,
            config.color,
            &label,
        );
    }
}
