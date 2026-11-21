//! 准星模块：优先采样 `crosshair` UI 贴图，缺失时用十字线回退。

use crate::texture::UiTextureMap;
use crate::ui::core::draw::{push_quad, push_ui_view, UiVertex};
use crate::ui::core::layout::UiRect;
use crate::ui::core::theme;

/// 准星参数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrosshairConfig {
    /// 单臂长度（像素）。
    pub arm: f32,
    /// 线宽（像素）。
    pub thickness: f32,
    /// 回退十字的亮色。
    pub color: [f32; 4],
    /// 回退十字的深色底衬。
    pub shadow: [f32; 4],
}

impl Default for CrosshairConfig {
    fn default() -> Self {
        Self {
            arm: 9.0,
            thickness: 2.0,
            color: theme::CROSSHAIR,
            shadow: theme::CROSSHAIR_SHADOW,
        }
    }
}

/// 绘制准星。
pub fn draw(
    out: &mut Vec<UiVertex>,
    width: f32,
    height: f32,
    config: &CrosshairConfig,
    ui: &UiTextureMap,
) {
    let center_x = width * 0.5;
    let center_y = height * 0.5;

    if let Some(view) = ui.get("crosshair") {
        let size = (config.arm + 4.0) * 2.0;
        push_ui_view(
            out,
            (width, height),
            UiRect::new(center_x - size * 0.5, center_y - size * 0.5, size, size),
            view,
            [1.0; 4],
        );
        return;
    }

    // 先画深色底衬，再画亮色线条，保证在草地与石头上都清晰可见。
    push_layer(
        out,
        (width, height),
        (center_x, center_y),
        1.0,
        config.shadow,
        config,
    );
    push_layer(
        out,
        (width, height),
        (center_x, center_y),
        0.0,
        config.color,
        config,
    );
}

fn push_layer(
    out: &mut Vec<UiVertex>,
    viewport: (f32, f32),
    center: (f32, f32),
    grow: f32,
    color: [f32; 4],
    config: &CrosshairConfig,
) {
    let arm = config.arm + grow;
    let thickness = config.thickness + grow * 2.0;
    let (center_x, center_y) = center;
    push_quad(
        out,
        viewport,
        UiRect::new(
            center_x - arm,
            center_y - thickness * 0.5,
            arm * 2.0,
            thickness,
        ),
        color,
    );
    push_quad(
        out,
        viewport,
        UiRect::new(
            center_x - thickness * 0.5,
            center_y - arm,
            thickness,
            arm * 2.0,
        ),
        color,
    );
}
