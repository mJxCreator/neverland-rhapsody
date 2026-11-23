//! 快捷栏模块：底板、9 个槽位、选中框、选中物品名称与物品图标。
//!
//! 模块只关心快捷栏自身的几何与参数；外部通过 [`HotbarUi::set`] 写入槽位数据，
//! 通过 [`HotbarUi::draw`] 生成顶点。槽位/选中框/底板的比例见 [`geometry`]。

pub mod config;
pub mod geometry;

use crate::item::{item_name, ItemStack, HOTBAR_SLOTS};
use crate::text;
use crate::texture::{ItemTextureMap, UiTextureMap};
use crate::ui::core::animation::UiClock;
use crate::ui::core::draw::{push_quad, push_ui_view, UiVertex};
use crate::ui::core::font;
use crate::ui::core::layout::UiRect;
use crate::ui::core::widget::{self, SlotSkin, SlotState};

use self::config::HotbarConfig;
use self::geometry::HotbarGeometry;

/// 快捷栏运行时状态。
#[derive(Clone, Debug, PartialEq)]
pub struct HotbarUi {
    /// 当前选中的槽位下标。
    pub selected_slot: usize,
    /// 9 个槽位的物品堆叠。
    pub slots: [Option<ItemStack>; HOTBAR_SLOTS],
    /// 可调参数。
    pub config: HotbarConfig,
    /// 选中格呼吸动画时钟（模块内部状态）。
    pub(crate) clock: UiClock,
}

impl Default for HotbarUi {
    fn default() -> Self {
        Self {
            selected_slot: 0,
            slots: [None; HOTBAR_SLOTS],
            config: HotbarConfig::default(),
            clock: UiClock::default(),
        }
    }
}

impl HotbarUi {
    /// 写入槽位与选中下标；下标自动限制到合法范围。
    pub fn set(&mut self, selected_slot: usize, slots: [Option<ItemStack>; HOTBAR_SLOTS]) {
        self.selected_slot = selected_slot.min(HOTBAR_SLOTS - 1);
        self.slots = slots;
    }

    /// 推进选中格呼吸动画。
    pub fn update(&mut self, delta_secs: f32) {
        self.clock.update(delta_secs);
    }

    /// 快捷栏顶边距屏幕底部的距离，供状态条等模块做垂直锚点。
    pub fn top_offset(&self) -> f32 {
        self.config.top_offset()
    }

    /// 绘制底板、槽位、选中框与选中物品名称。
    pub fn draw(
        &self,
        out: &mut Vec<UiVertex>,
        width: f32,
        height: f32,
        icons: &ItemTextureMap,
        ui: &UiTextureMap,
    ) {
        let viewport = (width, height);
        let geometry = HotbarGeometry::from_slot_size(self.config.slot_size);
        let total = geometry.slots_width(HOTBAR_SLOTS);
        let start_x = (width - total) * 0.5;
        let y = height - self.config.bottom_margin - geometry.slot;

        if let Some(view) = ui.get("hotbar_bg") {
            let first_slot = UiRect::new(start_x, y, geometry.slot, geometry.slot);
            let rect = geometry.background_rect(first_slot, HOTBAR_SLOTS);
            push_ui_view(out, viewport, rect, view, [1.0; 4]);
        }

        let pulse = self.clock.pulse(self.config.selected_pulse_speed);
        for index in 0..HOTBAR_SLOTS {
            let x = start_x + index as f32 * (geometry.slot + geometry.gap);
            let selected = index == self.selected_slot;
            let rect = UiRect::new(x, y, geometry.slot, geometry.slot);

            if selected {
                let expand = 1.0 + pulse * 2.0;
                let mut glow = self.config.selected_glow;
                glow[3] = 0.12 + pulse * 0.20;
                push_quad(
                    out,
                    viewport,
                    UiRect::new(
                        rect.x - expand,
                        rect.y - expand,
                        rect.width + expand * 2.0,
                        rect.height + expand * 2.0,
                    ),
                    glow,
                );
            }

            widget::push_slot(
                out,
                viewport,
                rect,
                selected.then(|| geometry.selected_rect(rect)),
                if selected {
                    SlotState::Selected
                } else {
                    SlotState::Normal
                },
                self.slots[index],
                icons,
                ui,
                SlotSkin::Hotbar,
                self.config.background,
            );
        }

        self.draw_selected_name(out, width, height);
    }

    /// 当前快捷栏物品的名称；工具损坏时改用警示色并注明状态。
    fn draw_selected_name(&self, out: &mut Vec<UiVertex>, width: f32, height: f32) {
        let Some(stack) = self.slots[self.selected_slot] else {
            return;
        };
        let text = if stack.is_broken() {
            text::item_broken(item_name(stack.item()))
        } else {
            item_name(stack.item()).to_string()
        };
        let color = if stack.is_broken() {
            self.config.broken_color
        } else {
            self.config.label_color
        };
        let text_width = font::text_width(&text, self.config.label_pixel);
        let x = (width - text_width) * 0.5;
        let y = height - self.top_offset() - self.config.label_offset;
        font::push_text(
            out,
            (width, height),
            x + 1.0,
            y + 1.0,
            self.config.label_pixel,
            self.config.shadow_color,
            &text,
        );
        font::push_text(
            out,
            (width, height),
            x,
            y,
            self.config.label_pixel,
            color,
            &text,
        );
    }
}
