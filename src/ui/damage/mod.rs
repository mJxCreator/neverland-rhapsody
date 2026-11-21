//! 受伤反馈模块：屏幕边缘红色闪屏与受击方向指示。

use crate::texture::UiTextureMap;
use crate::ui::core::draw::{push_quad, push_ui_view, UiVertex};
use crate::ui::core::layout::UiRect;
use crate::ui::core::theme;

/// 受伤反馈参数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageConfig {
    /// 闪屏持续时间（秒）。
    pub flash_duration: f32,
    /// 回退闪屏的边缘厚度（像素）。
    pub flash_thickness: f32,
    /// 受击方向指示的显示时长（秒）。
    pub direction_duration: f32,
    /// 受击方向指示的单块边长（像素）。
    pub direction_size: f32,
    /// 闪屏颜色。
    pub flash_color: [f32; 4],
    /// 方向指示颜色。
    pub direction_color: [f32; 4],
}

impl Default for DamageConfig {
    fn default() -> Self {
        Self {
            flash_duration: 0.35,
            flash_thickness: 26.0,
            direction_duration: 0.8,
            direction_size: 18.0,
            flash_color: theme::DAMAGE_FLASH,
            direction_color: theme::DAMAGE_DIRECTION,
        }
    }
}

/// 受伤反馈运行时状态。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageFeedback {
    /// 闪屏剩余时间。
    pub flash_remaining: f32,
    /// 受击方向（弧度，0 为屏幕正前方，正值在右）。
    pub direction: Option<f32>,
    /// 方向指示剩余时间。
    pub direction_remaining: f32,
    /// 可调参数。
    pub config: DamageConfig,
}

impl Default for DamageFeedback {
    fn default() -> Self {
        Self {
            flash_remaining: 0.0,
            direction: None,
            direction_remaining: 0.0,
            config: DamageConfig::default(),
        }
    }
}

impl DamageFeedback {
    /// 触发闪屏。
    pub fn show_flash(&mut self) {
        self.flash_remaining = self.config.flash_duration;
    }

    /// 显示受击方向；非法角度忽略。
    pub fn show_direction(&mut self, angle: f32) {
        if !angle.is_finite() {
            return;
        }
        self.direction = Some(angle);
        self.direction_remaining = self.config.direction_duration;
    }

    /// 推进两个反馈计时。
    pub fn update(&mut self, delta_secs: f32) {
        if !delta_secs.is_finite() || delta_secs <= 0.0 {
            return;
        }
        self.flash_remaining = (self.flash_remaining - delta_secs).max(0.0);
        if self.direction_remaining > 0.0 {
            self.direction_remaining -= delta_secs;
            if self.direction_remaining <= 0.0 {
                self.direction = None;
            }
        }
    }

    /// 绘制方向指示与闪屏。
    pub fn draw(&self, out: &mut Vec<UiVertex>, width: f32, height: f32, ui: &UiTextureMap) {
        self.draw_direction(out, width, height);
        self.draw_flash(out, width, height, ui);
    }

    fn draw_direction(&self, out: &mut Vec<UiVertex>, width: f32, height: f32) {
        let Some(angle) = self.direction else {
            return;
        };
        if self.direction_remaining <= 0.0 {
            return;
        }
        let config = &self.config;
        let viewport = (width, height);
        let alpha = (self.direction_remaining / config.direction_duration).clamp(0.0, 1.0);
        let (sin, cos) = angle.sin_cos();
        let direction = (sin, -cos);
        let center = (
            width * 0.5 + direction.0 * width * 0.32,
            height * 0.5 + direction.1 * height * 0.32,
        );
        for step in 0..3 {
            let offset = step as f32 * config.direction_size * 0.55;
            let size = config.direction_size * (1.0 - step as f32 * 0.25);
            let mut color = config.direction_color;
            color[3] *= alpha * (1.0 - step as f32 * 0.25);
            push_quad(
                out,
                viewport,
                UiRect::new(
                    center.0 + direction.0 * offset - size * 0.5,
                    center.1 + direction.1 * offset - size * 0.5,
                    size,
                    size,
                ),
                color,
            );
        }
    }

    fn draw_flash(&self, out: &mut Vec<UiVertex>, width: f32, height: f32, ui: &UiTextureMap) {
        if self.flash_remaining <= 0.0 {
            return;
        }
        let config = &self.config;
        let alpha = (self.flash_remaining / config.flash_duration).clamp(0.0, 1.0);
        let viewport = (width, height);
        if let Some(view) = ui.get("damage_vignette") {
            push_ui_view(
                out,
                viewport,
                UiRect::new(0.0, 0.0, width, height),
                view,
                [1.0, 1.0, 1.0, alpha],
            );
            return;
        }
        let mut color = config.flash_color;
        color[3] *= alpha;
        let thickness = config.flash_thickness;
        push_quad(
            out,
            viewport,
            UiRect::new(0.0, 0.0, width, thickness),
            color,
        );
        push_quad(
            out,
            viewport,
            UiRect::new(0.0, height - thickness, width, thickness),
            color,
        );
        push_quad(
            out,
            viewport,
            UiRect::new(0.0, thickness, thickness, height - thickness * 2.0),
            color,
        );
        push_quad(
            out,
            viewport,
            UiRect::new(
                width - thickness,
                thickness,
                thickness,
                height - thickness * 2.0,
            ),
            color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flash_adds_geometry_and_fades_out() {
        let mut feedback = DamageFeedback::default();
        let mut plain = Vec::new();
        feedback.draw(&mut plain, 1280.0, 720.0, &UiTextureMap::empty());
        feedback.show_flash();
        let mut damaged = Vec::new();
        feedback.draw(&mut damaged, 1280.0, 720.0, &UiTextureMap::empty());
        assert!(damaged.len() > plain.len());
        feedback.update(feedback.config.flash_duration + 0.1);
        let mut recovered = Vec::new();
        feedback.draw(&mut recovered, 1280.0, 720.0, &UiTextureMap::empty());
        assert_eq!(recovered.len(), plain.len());
    }
}
