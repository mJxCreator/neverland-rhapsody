//! 营火烹饪界面：输入槽、输出槽、进度条与关闭提示。
//!
//! 界面本身不推进烹饪；它只负责把玩家点击转换为对背包与营火状态的原子操作，
//! 烹饪进度由游戏层的营火注册表持续更新，因此关闭界面不会中断烹饪。

use glam::IVec3;

use super::draw::{push_border, push_quad, UiVertex};
use super::font;
use super::layout::{centered_panel, UiRect};
use super::theme;
use super::widget::{self, SlotSkin, SlotState};
use crate::item::{Inventory, ItemStack};
use crate::survival::cooking::{recipe_for, CampfireState};
use crate::texture::{ItemTextureMap, UiTextureMap};

/// 每点击一次输入槽最多转移的物品数量。
const TRANSFER_LIMIT: u32 = 16;

/// 烹饪界面的布局矩形。
#[derive(Clone, Copy, Debug)]
struct Layout {
    panel: UiRect,
    input: UiRect,
    output: UiRect,
    progress: UiRect,
}

fn layout(viewport: (f32, f32)) -> Layout {
    let panel = centered_panel(viewport, 560.0, 320.0);
    let slot_size = 56.0;
    let input = UiRect::new(panel.x + 84.0, panel.y + 150.0, slot_size, slot_size);
    let output = UiRect::new(
        panel.x + panel.width - 84.0 - slot_size,
        panel.y + 150.0,
        slot_size,
        slot_size,
    );
    let progress = UiRect::new(panel.x + 180.0, panel.y + 172.0, panel.width - 360.0, 16.0);
    Layout {
        panel,
        input,
        output,
        progress,
    }
}

/// 烹饪界面的一次点击结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CookingAction {
    None,
    /// 营火或背包内容已改变，需要落盘。
    Changed,
}

/// 烹饪界面状态。
#[derive(Clone, Debug, PartialEq)]
pub struct CookingUi {
    open: bool,
    cursor: (f32, f32),
    campfire: Option<IVec3>,
    textures: UiTextureMap,
}

impl Default for CookingUi {
    fn default() -> Self {
        Self::new()
    }
}

impl CookingUi {
    pub fn new() -> Self {
        Self {
            open: false,
            cursor: (0.0, 0.0),
            campfire: None,
            textures: UiTextureMap::empty(),
        }
    }

    pub fn set_ui_textures(&mut self, textures: &UiTextureMap) {
        self.textures = textures.clone();
    }

    pub fn open(&mut self, campfire: IVec3) {
        self.open = true;
        self.campfire = Some(campfire);
    }

    pub fn close(&mut self) {
        self.open = false;
        self.campfire = None;
    }

    #[allow(dead_code)]
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn campfire(&self) -> Option<IVec3> {
        self.campfire
    }

    #[allow(dead_code)]
    pub fn cursor(&self) -> (f32, f32) {
        self.cursor
    }

    pub fn set_cursor(&mut self, x: f32, y: f32) {
        self.cursor = (x, y);
    }

    /// 处理一次点击；只命中输入槽或输出槽。
    pub fn click_at(
        &mut self,
        campfire: Option<&mut CampfireState>,
        inventory: &mut Inventory,
        viewport: (f32, f32),
    ) -> CookingAction {
        let Some(campfire) = campfire else {
            return CookingAction::None;
        };
        let layout = layout(viewport);
        let (x, y) = self.cursor;
        if layout.input.contains(x, y) {
            return if move_into_campfire(campfire, inventory) {
                CookingAction::Changed
            } else {
                CookingAction::None
            };
        }
        if layout.output.contains(x, y) {
            return if take_from_campfire(campfire, inventory) {
                CookingAction::Changed
            } else {
                CookingAction::None
            };
        }
        CookingAction::None
    }

    /// 生成烹饪界面几何。
    pub fn build(
        &self,
        out: &mut Vec<UiVertex>,
        campfire: Option<&CampfireState>,
        inventory: &Inventory,
        icons: &ItemTextureMap,
        viewport: (f32, f32),
    ) {
        if !self.open {
            return;
        }
        let layout = layout(viewport);
        push_quad(
            out,
            viewport,
            UiRect::new(0.0, 0.0, viewport.0, viewport.1),
            theme::OVERLAY_DARK,
        );
        widget::push_panel(
            out,
            viewport,
            layout.panel,
            &self.textures,
            "panel_background",
        );
        push_border(out, viewport, layout.panel, 2.0, theme::PANEL_BORDER);

        let title = "CAMPFIRE";
        let title_width = font::text_width(title, 3.0);
        font::push_text(
            out,
            viewport,
            layout.panel.x + (layout.panel.width - title_width) * 0.5,
            layout.panel.y + 26.0,
            3.0,
            theme::TEXT_TITLE,
            title,
        );

        let (input_stack, output_stack, ratio, cooking) = match campfire {
            Some(state) => (
                state.input(),
                state.output(),
                state.progress_ratio(),
                state.is_cooking(),
            ),
            None => (None, None, 0.0, false),
        };
        let input_state = if campfire.is_some() {
            SlotState::Normal
        } else {
            SlotState::Disabled
        };
        widget::push_slot(
            out,
            viewport,
            layout.input,
            input_state,
            input_stack,
            icons,
            &self.textures,
            SlotSkin::Inventory,
            theme::SLOT_BACKGROUND,
        );
        widget::push_slot(
            out,
            viewport,
            layout.output,
            SlotState::Normal,
            output_stack,
            icons,
            &self.textures,
            SlotSkin::Inventory,
            theme::SLOT_BACKGROUND,
        );

        // 进度条背景、填充与百分比。
        push_quad(out, viewport, layout.progress, theme::PROGRESS_BACKGROUND);
        if ratio > 0.0 {
            let fill = UiRect {
                width: layout.progress.width * ratio,
                ..layout.progress
            };
            push_quad(out, viewport, fill, theme::PROGRESS_FILL);
        }
        push_border(out, viewport, layout.progress, 1.0, theme::PANEL_BORDER);
        let percent = format!("{}%", (ratio * 100.0).round() as u32);
        let percent_width = font::text_width(&percent, 2.0);
        font::push_text(
            out,
            viewport,
            layout.progress.x + (layout.progress.width - percent_width) * 0.5,
            layout.progress.y + 24.0,
            2.0,
            theme::TEXT_MUTED,
            &percent,
        );

        let (hint, color) = if cooking {
            ("COOKING", theme::CRAFT_SUCCESS)
        } else {
            ("CLICK INPUT TO ADD RAW MEAT", theme::TEXT_MUTED)
        };
        let hint_width = font::text_width(hint, 2.0);
        font::push_text(
            out,
            viewport,
            layout.panel.x + (layout.panel.width - hint_width) * 0.5,
            layout.panel.y + 240.0,
            2.0,
            color,
            hint,
        );
        let close = "PRESS E OR ESC TO CLOSE";
        let close_width = font::text_width(close, 1.5);
        font::push_text(
            out,
            viewport,
            layout.panel.x + (layout.panel.width - close_width) * 0.5,
            layout.panel.y + layout.panel.height - 34.0,
            1.5,
            theme::TEXT_MUTED,
            close,
        );

        // 背包中的可烹饪物品数量，帮助玩家确认输入来源。
        let available = cookable_count(inventory);
        let info = format!("RAW MEAT IN PACK: {available}");
        let info_width = font::text_width(&info, 1.5);
        font::push_text(
            out,
            viewport,
            layout.panel.x + (layout.panel.width - info_width) * 0.5,
            layout.panel.y + 270.0,
            1.5,
            theme::TEXT_MUTED,
            &info,
        );
    }
}

/// 把背包中的生肉移入营火输入槽。
fn move_into_campfire(campfire: &mut CampfireState, inventory: &mut Inventory) -> bool {
    let capacity = match campfire.input() {
        None => 64,
        Some(stack) => stack.room(),
    };
    if capacity == 0 {
        return false;
    }
    let Some((slot, stack)) = find_cookable_stack(inventory) else {
        return false;
    };
    let moved = capacity.min(stack.count()).min(TRANSFER_LIMIT);
    if moved == 0 {
        return false;
    }
    let Some(portion) = stack.with_count(moved) else {
        return false;
    };
    let remaining = stack.count() - moved;
    let _ = inventory.set_slot(slot, stack.with_count(remaining));
    let leftover = campfire.add_input(portion);
    if let Some(leftover) = leftover {
        // 理论上容量已校验，不应发生；兜底退回背包，避免物品丢失。
        let _ = inventory.add_stack(leftover);
    }
    true
}

/// 取出营火产出放入背包；放不下时原样退回。
fn take_from_campfire(campfire: &mut CampfireState, inventory: &mut Inventory) -> bool {
    let Some(output) = campfire.take_output() else {
        return false;
    };
    if let Some(leftover) = inventory.add_stack(output) {
        let _ = campfire.deposit_output(leftover);
    }
    true
}

fn find_cookable_stack(inventory: &Inventory) -> Option<(usize, ItemStack)> {
    (0..crate::item::inventory::TOTAL_SLOTS).find_map(|slot| {
        let stack = inventory.slot(slot)?;
        recipe_for(stack.item()).map(|_| (slot, stack))
    })
}

fn cookable_count(inventory: &Inventory) -> u32 {
    (0..crate::item::inventory::TOTAL_SLOTS)
        .filter_map(|slot| inventory.slot(slot))
        .filter(|stack| recipe_for(stack.item()).is_some())
        .map(ItemStack::count)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::ItemId;

    fn inventory_with(slot: usize, item: ItemId, count: u32) -> Inventory {
        let mut inventory = Inventory::new();
        let _ = inventory.set_slot(slot, ItemStack::new(item, count));
        inventory
    }

    #[test]
    fn click_routes_only_to_slots() {
        let mut ui = CookingUi::new();
        ui.open(IVec3::new(0, 1, 2));
        let mut fire = CampfireState::empty(IVec3::new(0, 1, 2));
        let mut inventory = inventory_with(0, ItemId::RAW_PORKCHOP, 3);
        let viewport = (1280.0, 720.0);
        let layout = layout(viewport);
        ui.set_cursor(layout.panel.x + 4.0, layout.panel.y + 4.0);
        assert_eq!(
            ui.click_at(Some(&mut fire), &mut inventory, viewport),
            CookingAction::None
        );
        ui.set_cursor(layout.input.x + 4.0, layout.input.y + 4.0);
        assert_eq!(
            ui.click_at(Some(&mut fire), &mut inventory, viewport),
            CookingAction::Changed
        );
        assert!(fire.input().is_some());
        assert_eq!(inventory.count_of(ItemId::RAW_PORKCHOP), 0);
    }

    #[test]
    fn taking_output_returns_items_to_inventory() {
        let mut fire = CampfireState::empty(IVec3::new(0, 1, 2));
        fire.add_input(ItemStack::new(ItemId::RAW_BEEF, 1).unwrap());
        fire.update(crate::survival::cooking::COOK_SECONDS);
        let mut inventory = Inventory::new();
        assert!(take_from_campfire(&mut fire, &mut inventory));
        assert_eq!(inventory.count_of(ItemId::COOKED_BEEF), 1);
        assert!(fire.output().is_none());
    }

    #[test]
    fn closing_keeps_cooking_state() {
        let mut ui = CookingUi::new();
        ui.open(IVec3::new(1, 2, 3));
        assert!(ui.is_open());
        ui.close();
        assert!(!ui.is_open());
        assert!(ui.campfire().is_none());
    }
}
