//! 敌对生物名牌与血条模块。

use crate::ui::core::draw::{push_quad, UiVertex};
use crate::ui::core::font;
use crate::ui::core::layout::UiRect;
use crate::ui::core::theme;

/// 名牌与血条参数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TargetConfig {
    /// 停止命中后继续显示的时长（秒）。
    pub duration: f32,
    /// 血条宽度（像素）。
    pub bar_width: f32,
    /// 血条高度（像素）。
    pub bar_height: f32,
    /// 名牌与血条相对状态条的垂直间距（像素）。
    pub gap: f32,
    /// 名牌字号（像素格）。
    pub name_pixel: f32,
    /// 名牌文字颜色。
    pub name_color: [f32; 4],
    /// 血条底衬颜色。
    pub background: [f32; 4],
    /// 血条填充颜色。
    pub fill: [f32; 4],
}

impl Default for TargetConfig {
    fn default() -> Self {
        Self {
            duration: 2.5,
            bar_width: 180.0,
            bar_height: 9.0,
            gap: 34.0,
            name_pixel: 1.3,
            name_color: theme::TEXT,
            background: theme::TARGET_BAR_BACKGROUND,
            fill: theme::TARGET_BAR_FILL,
        }
    }
}

/// 当前显示血量的敌对生物。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TargetInfo {
    pub name: &'static str,
    pub health: u32,
    pub max_health: u32,
}

/// 名牌运行时状态。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TargetHud {
    /// 当前目标；`None` 时不绘制。
    pub target: Option<TargetInfo>,
    /// 剩余显示时间（秒）。
    pub remaining: f32,
    /// 可调参数。
    pub config: TargetConfig,
}

impl Default for TargetHud {
    fn default() -> Self {
        Self {
            target: None,
            remaining: 0.0,
            config: TargetConfig::default(),
        }
    }
}

impl TargetHud {
    /// 显示敌对生物血量；最大值为 0 时忽略。
    pub fn show(&mut self, name: &'static str, health: u32, max_health: u32) {
        if max_health == 0 {
            return;
        }
        self.target = Some(TargetInfo {
            name,
            health: health.min(max_health),
            max_health,
        });
        self.remaining = self.config.duration;
    }

    /// 推进倒计时。
    pub fn update(&mut self, delta_secs: f32) {
        if !delta_secs.is_finite() || delta_secs <= 0.0 {
            return;
        }
        if self.remaining > 0.0 {
            self.remaining -= delta_secs;
            if self.remaining <= 0.0 {
                self.target = None;
            }
        }
    }

    /// 绘制名牌与血条；`name_y` 为名牌顶边坐标。
    pub fn draw(&self, out: &mut Vec<UiVertex>, width: f32, height: f32, name_y: f32) {
        let Some(target) = self.target else {
            return;
        };
        if self.remaining <= 0.0 {
            return;
        }
        let viewport = (width, height);
        let config = &self.config;
        let alpha = (self.remaining / 0.35).clamp(0.0, 1.0);

        let label = format!("{} {}/{}", target.name, target.health, target.max_health);
        let label_width = font::text_width(&label, config.name_pixel);
        let mut label_color = config.name_color;
        label_color[3] *= alpha;
        font::push_text(
            out,
            viewport,
            (width - label_width) * 0.5,
            name_y,
            config.name_pixel,
            label_color,
            &label,
        );

        let x = (width - config.bar_width) * 0.5;
        let y = name_y + 14.0;
        let mut background = config.background;
        background[3] *= alpha;
        push_quad(
            out,
            viewport,
            UiRect::new(x, y, config.bar_width, config.bar_height),
            background,
        );
        let ratio = target.health.min(target.max_health) as f32 / target.max_health as f32;
        if ratio > 0.0 {
            let mut fill = config.fill;
            fill[3] *= alpha;
            push_quad(
                out,
                viewport,
                UiRect::new(x, y, config.bar_width * ratio, config.bar_height),
                fill,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_expires_after_duration() {
        let mut hud = TargetHud::default();
        hud.show("ZOMBIE", 12, 20);
        assert!(hud.target.is_some());
        hud.update(hud.config.duration + 0.1);
        assert!(hud.target.is_none());
    }

    #[test]
    fn invalid_max_health_is_ignored() {
        let mut hud = TargetHud::default();
        hud.show("ZOMBIE", 1, 0);
        assert!(hud.target.is_none());
    }
}
