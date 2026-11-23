//! 生命/体力/饥饿状态条模块。
//!
//! 三行填充与 106×19 底板属于同一组固定相对位置：内边距 2、行间距 0、从上到下
//! 依次为生命、体力、饥饿。填充按百分比从右侧裁剪源图像素列。

pub mod config;

use crate::texture::UiTextureMap;
use crate::ui::core::draw::{push_quad, push_ui_view, UiVertex};
use crate::ui::core::layout::UiRect;

use self::config::StateBarConfig;

/// 三行状态条的数值与参数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StateBars {
    /// 当前生命值（0–100）。
    pub health: u32,
    /// 最大生命值（默认 100）。
    pub max_health: u32,
    /// 当前饥饿值（0 到 `hunger_max`）。
    pub hunger: f32,
    /// 当前饥饿上限（默认 100，可被加成抬升）。
    pub hunger_max: f32,
    /// 当前体力值（0 到 `stamina_max`）。
    pub stamina: f32,
    /// 当前体力上限（默认 100，跟随饥饿变化）。
    pub stamina_max: f32,
    /// 可调参数。
    pub config: StateBarConfig,
}

impl Default for StateBars {
    fn default() -> Self {
        Self {
            health: 100,
            max_health: 100,
            hunger: 100.0,
            hunger_max: 100.0,
            stamina: 100.0,
            stamina_max: 100.0,
            config: StateBarConfig::default(),
        }
    }
}

impl StateBars {
    /// 写入生命值；最大值为 0 时回落到默认 100 点。
    pub fn set_health(&mut self, health: u32, max_health: u32) {
        self.max_health = if max_health == 0 { 100 } else { max_health };
        self.health = health.min(self.max_health);
    }

    /// 写入饥饿与体力及其当前上限；数值裁剪到 `0..=上限`。
    pub fn set_survival(
        &mut self,
        hunger: f32,
        hunger_max: f32,
        stamina: f32,
        stamina_max: f32,
    ) {
        self.hunger_max = valid_max(hunger_max);
        self.stamina_max = valid_max(stamina_max);
        self.hunger = clamp_to_max(hunger, self.hunger_max);
        self.stamina = clamp_to_max(stamina, self.stamina_max);
    }

    /// 面板顶边坐标；`bottom_anchor` 为快捷栏顶边。
    pub fn panel_top(&self, bottom_anchor: f32) -> f32 {
        bottom_anchor - self.config.bottom_offset - self.config.panel_height()
    }

    /// 绘制底板与三条填充。
    pub fn draw(
        &self,
        out: &mut Vec<UiVertex>,
        width: f32,
        height: f32,
        bottom_anchor: f32,
        ui: &UiTextureMap,
    ) {
        let viewport = (width, height);
        let config = &self.config;
        let panel = UiRect::new(
            (width - config.panel_width()) * 0.5,
            self.panel_top(bottom_anchor),
            config.panel_width(),
            config.panel_height(),
        );
        if let Some(view) = ui.get("state_bg") {
            push_ui_view(out, viewport, panel, view, [1.0; 4]);
        } else {
            push_quad(out, viewport, panel, config.background);
        }

        let health_ratio = if self.max_health == 0 {
            0.0
        } else {
            self.health as f32 / self.max_health as f32
        };
        let rows = [
            ("state_heart_fill", health_ratio, config.heart),
            (
                "state_stamina_fill",
                ratio_to_max(self.stamina, self.stamina_max),
                config.stamina,
            ),
            (
                "state_hunger_fill",
                ratio_to_max(self.hunger, self.hunger_max),
                config.hunger,
            ),
        ];
        let fill_x = panel.x + config.padding * config.scale;
        for (index, (name, ratio, fallback)) in rows.into_iter().enumerate() {
            let row_y =
                panel.y + (config.padding + index as f32 * config.fill_height) * config.scale;
            push_state_fill(
                out,
                viewport,
                (fill_x, row_y),
                (name, ratio, fallback),
                ui,
                config,
            );
        }
    }
}

/// 把上限清洗为有限正数；非法值回落到默认 100。
fn valid_max(max: f32) -> f32 {
    if max.is_finite() && max > 0.0 {
        max
    } else {
        100.0
    }
}

/// 把当前值裁剪到 `0..=上限`；非有限值按满值处理。
fn clamp_to_max(value: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, max)
    } else {
        max
    }
}

/// 按当前上限归一，得到 0–1 的填充比例。
fn ratio_to_max(value: f32, max: f32) -> f32 {
    if max > 0.0 && value.is_finite() {
        (value / max).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// 按百分比裁剪右侧像素绘制一条状态填充。
///
/// 默认 102 像素对应 100 个 1% 档位：中间每档 1 列，首尾两档各占 2 列。
fn push_state_fill(
    out: &mut Vec<UiVertex>,
    viewport: (f32, f32),
    anchor: (f32, f32),
    row: (&str, f32, [f32; 4]),
    ui: &UiTextureMap,
    config: &StateBarConfig,
) {
    let (name, ratio, fallback) = row;
    let columns = fill_columns(ratio, config.fill_width);
    if columns <= 0.0 {
        return;
    }
    let rect = UiRect::new(
        anchor.0,
        anchor.1,
        columns * config.scale,
        config.fill_height * config.scale,
    );
    if let Some(view) = ui.get(name) {
        let mut cropped = view;
        cropped.draw = crate::texture::ui_texture::UiDrawMode::Stretch;
        cropped.aspect = None;
        cropped.uv.max[0] = cropped.uv.min[0] + cropped.uv.width() * (columns / config.fill_width);
        push_ui_view(out, viewport, rect, cropped, [1.0; 4]);
    } else {
        push_quad(out, viewport, rect, fallback);
    }
}

/// 百分比对应的原图像素列数：0 为 0 列，100% 为满宽，中间每 1% 增加 1 列。
fn fill_columns(ratio: f32, full_width: f32) -> f32 {
    let ratio = if ratio.is_finite() {
        ratio.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let percent = (ratio * 100.0).round();
    if percent <= 0.0 {
        0.0
    } else if percent >= 100.0 {
        full_width
    } else {
        percent + 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_columns_map_first_and_last_percent_to_two_columns() {
        assert_eq!(fill_columns(0.0, 102.0), 0.0);
        assert_eq!(fill_columns(0.01, 102.0), 2.0, "首个 1% 占两列");
        assert_eq!(fill_columns(0.5, 102.0), 51.0);
        assert_eq!(fill_columns(0.99, 102.0), 100.0, "最后一个 1% 占两列");
        assert_eq!(fill_columns(1.0, 102.0), 102.0);
        assert_eq!(fill_columns(f32::NAN, 102.0), 0.0);
    }

    #[test]
    fn values_are_clamped_and_defaults_are_safe() {
        let mut bars = StateBars::default();
        bars.set_health(150, 100);
        assert_eq!(bars.health, 100);
        bars.set_health(5, 0);
        assert_eq!(bars.max_health, 100);
        assert_eq!(bars.health, 5);
        bars.set_survival(120.0, 100.0, -3.0, 100.0);
        assert_eq!(bars.hunger, 100.0);
        assert_eq!(bars.stamina, 0.0);
        bars.set_survival(30.0, f32::NAN, 10.0, 0.0);
        assert_eq!(bars.hunger_max, 100.0);
        assert_eq!(bars.stamina_max, 100.0);
        assert_eq!(bars.hunger, 30.0);
        assert_eq!(bars.stamina, 10.0);
    }

    #[test]
    fn fill_ratio_normalizes_by_current_max() {
        let mut bars = StateBars::default();
        bars.set_survival(150.0, 150.0, 75.0, 150.0);
        assert_eq!(ratio_to_max(bars.hunger, bars.hunger_max), 1.0);
        assert_eq!(ratio_to_max(bars.stamina, bars.stamina_max), 0.5);
        // 上限回落后当前值被裁到新上限，比例保持满条。
        bars.set_survival(150.0, 100.0, 75.0, 100.0);
        assert_eq!(bars.hunger, 100.0);
        assert_eq!(ratio_to_max(bars.hunger, bars.hunger_max), 1.0);
        assert_eq!(ratio_to_max(bars.stamina, bars.stamina_max), 0.75);
    }

    #[test]
    fn ratio_to_max_guards_zero_and_non_finite() {
        assert_eq!(ratio_to_max(10.0, 0.0), 0.0);
        assert_eq!(ratio_to_max(f32::NAN, 100.0), 0.0);
        assert_eq!(ratio_to_max(50.0, 200.0), 0.25);
    }

    #[test]
    fn panel_sits_above_the_hotbar_anchor() {
        let bars = StateBars::default();
        let top = bars.panel_top(600.0);
        assert_eq!(top, 600.0 - 28.0 - 57.0);
    }
}
