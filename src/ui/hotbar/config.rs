//! 快捷栏模块的可调参数。
//!
//! 槽位、选中框与底板的比例由 [`super::geometry`] 固化；本文件只放程序员需要
//! 经常调整的尺寸、间距与配色。

use crate::ui::core::theme;

/// 快捷栏参数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HotbarConfig {
    /// 单个槽位的屏幕边长（像素）；贴图按 16n 设计，默认 48 即 n = 3。
    pub slot_size: f32,
    /// 快捷栏底边距屏幕底部的距离（像素）。
    pub bottom_margin: f32,
    /// 选中格呼吸光晕速度。
    pub selected_pulse_speed: f32,
    /// 选中格光晕基础颜色（透明度会按脉冲动态调整）。
    pub selected_glow: [f32; 4],
    /// 快捷栏缺失底板贴图时的回退底色。
    pub background: [f32; 4],
    /// 选中物品名称与快捷栏顶部的距离（像素）。
    pub label_offset: f32,
    /// 选中物品名称的字号（像素格）。
    pub label_pixel: f32,
    /// 普通物品名称颜色。
    pub label_color: [f32; 4],
    /// 损坏工具名称的警示色。
    pub broken_color: [f32; 4],
    /// 名称阴影颜色。
    pub shadow_color: [f32; 4],
}

impl Default for HotbarConfig {
    fn default() -> Self {
        Self {
            slot_size: 48.0,
            bottom_margin: 18.0,
            selected_pulse_speed: 4.0,
            selected_glow: theme::SLOT_GLOW,
            background: theme::HOTBAR_BACKGROUND,
            label_offset: 108.0,
            label_pixel: 2.0,
            label_color: theme::TOOL_LABEL,
            broken_color: theme::TOOL_BROKEN,
            shadow_color: theme::TEXT_SHADOW,
        }
    }
}

impl HotbarConfig {
    /// 快捷栏顶边距屏幕底部的距离；其它 HUD 模块用它做垂直锚点。
    pub fn top_offset(&self) -> f32 {
        self.bottom_margin + self.slot_size
    }
}
