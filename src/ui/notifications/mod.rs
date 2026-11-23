//! 短提示模块：屏幕中部状态消息与快捷栏上方的拾取/工具损坏提示。

use crate::item::{item_name, ItemId};
use crate::texture::ItemTextureMap;
use crate::ui::core::draw::{push_item_icon, push_quad, UiVertex};
use crate::ui::core::font;
use crate::ui::core::layout::UiRect;
use crate::ui::core::theme;

/// 提示模块参数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NotificationConfig {
    /// 屏幕中部消息的显示时长（秒）。
    pub message_duration: f32,
    /// 中部消息字号（像素格）。
    pub message_pixel: f32,
    /// 中部消息的垂直位置（屏幕高度比例）。
    pub message_y_ratio: f32,
    /// 中部消息背景色。
    pub message_background: [f32; 4],
    /// 中部消息文字颜色。
    pub message_text: [f32; 4],
    /// 拾取/损坏提示的显示时长（秒）。
    pub toast_duration: f32,
    /// 提示淡出时长（秒）。
    pub toast_fade: f32,
    /// 提示字号（像素格）。
    pub toast_pixel: f32,
    /// 提示图标边长（像素）。
    pub toast_icon_size: f32,
    /// 提示与快捷栏顶部的距离（像素）。
    pub toast_offset: f32,
    /// 拾取提示文字颜色。
    pub toast_text: [f32; 4],
    /// 工具损坏提示文字颜色。
    pub broken_text: [f32; 4],
    /// 文字阴影颜色。
    pub shadow: [f32; 4],
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            message_duration: 2.5,
            message_pixel: 2.0,
            message_y_ratio: 0.62,
            message_background: theme::OVERLAY_DARK,
            message_text: [1.0, 0.86, 0.44, 1.0],
            toast_duration: 1.2,
            toast_fade: 0.3,
            toast_pixel: 2.0,
            toast_icon_size: 22.0,
            toast_offset: 132.0,
            toast_text: theme::TOAST_TEXT,
            broken_text: theme::TOOL_BROKEN,
            shadow: theme::TEXT_SHADOW,
        }
    }
}

/// 提示类型；决定绘制时的文案与颜色。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToastKind {
    Pickup,
    ToolBroken,
}

/// 一条提示：物品、数量、类型与剩余显示时间。
#[derive(Clone, Copy, Debug, PartialEq)]
struct Toast {
    item: ItemId,
    count: u32,
    kind: ToastKind,
    remaining: f32,
}

/// 提示运行时状态。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Notifications {
    message: Option<(&'static str, f32)>,
    toast: Option<Toast>,
    /// 可调参数。
    pub config: NotificationConfig,
}

impl Notifications {
    /// 显示一条短状态提示，覆盖旧提示并重新计时。
    pub fn show_message(&mut self, text: &'static str) {
        self.message = Some((text, self.config.message_duration));
    }

    /// 显示一条拾取提示；同一物品重复拾取会合并数量并刷新计时。
    pub fn show_pickup(&mut self, item: ItemId, count: u32) {
        if count == 0 || !item.is_valid() {
            return;
        }
        if let Some(toast) = &mut self.toast {
            if toast.kind == ToastKind::Pickup && toast.item == item {
                toast.count = toast.count.saturating_add(count);
                toast.remaining = self.config.toast_duration;
                return;
            }
        }
        self.toast = Some(Toast {
            item,
            count,
            kind: ToastKind::Pickup,
            remaining: self.config.toast_duration,
        });
    }

    /// 显示工具损坏提示。
    pub fn show_tool_broken(&mut self, item: ItemId) {
        if !item.is_valid() {
            return;
        }
        self.toast = Some(Toast {
            item,
            count: 0,
            kind: ToastKind::ToolBroken,
            remaining: self.config.toast_duration,
        });
    }

    /// 推进消息与提示计时。
    pub fn update(&mut self, delta_secs: f32) {
        if !delta_secs.is_finite() || delta_secs <= 0.0 {
            return;
        }
        if let Some((_, remaining)) = &mut self.message {
            *remaining -= delta_secs;
            if *remaining <= 0.0 {
                self.message = None;
            }
        }
        let Some(toast) = &mut self.toast else {
            return;
        };
        toast.remaining -= delta_secs;
        if toast.remaining <= 0.0 {
            self.toast = None;
        }
    }

    /// 绘制中部消息与快捷栏上方提示。
    pub fn draw(
        &self,
        out: &mut Vec<UiVertex>,
        width: f32,
        height: f32,
        hotbar_top: f32,
        icons: &ItemTextureMap,
    ) {
        self.draw_message(out, width, height);
        self.draw_toast(out, width, height, hotbar_top, icons);
    }

    fn draw_message(&self, out: &mut Vec<UiVertex>, width: f32, height: f32) {
        let Some((text, remaining)) = self.message else {
            return;
        };
        if remaining <= 0.0 {
            return;
        }
        let config = &self.config;
        let viewport = (width, height);
        let alpha = (remaining / config.message_duration).clamp(0.0, 1.0);
        let text_width = font::text_width(text, config.message_pixel);
        let x = (width - text_width) * 0.5;
        let y = height * config.message_y_ratio;
        let mut background = config.message_background;
        background[3] *= alpha;
        push_quad(
            out,
            viewport,
            UiRect::new(x - 10.0, y - 6.0, text_width + 20.0, 26.0),
            background,
        );
        let mut color = config.message_text;
        color[3] *= alpha;
        font::push_text(out, viewport, x, y, config.message_pixel, color, text);
    }

    fn draw_toast(
        &self,
        out: &mut Vec<UiVertex>,
        width: f32,
        height: f32,
        hotbar_top: f32,
        icons: &ItemTextureMap,
    ) {
        let Some(toast) = self.toast else {
            return;
        };
        let config = &self.config;
        let alpha = (toast.remaining / config.toast_fade).clamp(0.0, 1.0);
        let (text, base_color) = match toast.kind {
            ToastKind::Pickup => (
                format!("+{} {}", toast.count, item_name(toast.item)),
                config.toast_text,
            ),
            ToastKind::ToolBroken => (
                format!("{} 已损坏", item_name(toast.item)),
                config.broken_text,
            ),
        };
        let has_icon = icons.icon(toast.item).is_some();
        let icon_span = if has_icon {
            config.toast_icon_size + 8.0
        } else {
            0.0
        };
        let text_width = font::text_width(&text, config.toast_pixel);
        let total_width = icon_span + text_width;
        let x = (width - total_width) * 0.5;
        let y = hotbar_top - config.toast_offset;

        if has_icon {
            push_item_icon(
                out,
                (width, height),
                UiRect::new(x, y - 2.0, config.toast_icon_size, config.toast_icon_size),
                toast.item,
                icons,
            );
        }
        let text_x = x + icon_span;
        font::push_text(
            out,
            (width, height),
            text_x + 1.0,
            y + 1.0,
            config.toast_pixel,
            [0.0, 0.0, 0.0, 0.85 * alpha],
            &text,
        );
        let mut color = base_color;
        color[3] *= alpha;
        font::push_text(
            out,
            (width, height),
            text_x,
            y,
            config.toast_pixel,
            color,
            &text,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toast_expires_and_pickups_merge() {
        let mut notifications = Notifications::default();
        notifications.show_pickup(ItemId::STONE, 3);
        notifications.show_pickup(ItemId::STONE, 5);
        assert_eq!(notifications.toast.map(|toast| toast.count), Some(8));
        notifications.show_pickup(ItemId::DIRT, 1);
        assert_eq!(
            notifications.toast.map(|toast| toast.item),
            Some(ItemId::DIRT)
        );
        notifications.update(notifications.config.toast_duration + 0.1);
        assert!(notifications.toast.is_none());
    }
}
