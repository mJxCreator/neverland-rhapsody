//! 背包界面：27 格主背包 + 9 格快捷栏、底部物品详情、左右键移动/拆分与悬停提示。
//!
//! 本模块只负责界面状态、布局与点击语义，不直接修改世界或存档；
//! 所有槽位变更都通过 [`Inventory`] 接口完成，因此不会产生重复或丢失物品。
//! 视觉细节复用 [`super::widget`]，保证与快捷栏、合成界面的槽位表现一致。

use super::animation::UiAnimation;
use super::draw::{push_border, push_quad, UiVertex};
use super::font;
use super::input::ClickButton;
use super::layout::{centered_panel, UiRect};
use super::theme;
use super::widget::{self, SlotSkin, SlotState};
use crate::item::{Inventory, ItemStack, HOTBAR_OFFSET, TOTAL_SLOTS};
use crate::texture::{ItemTextureMap, UiTextureMap};

/// 基础布局尺寸（像素）。
const BASE_SLOT: f32 = 44.0;
const BASE_GAP: f32 = 6.0;
const BASE_PADDING: f32 = 12.0;
const BASE_TITLE: f32 = 24.0;
const BASE_HOTBAR_GAP: f32 = 14.0;
const BASE_INFO_GAP: f32 = 10.0;
const BASE_INFO_HEIGHT: f32 = 38.0;
const COLUMNS: usize = 9;
const MAIN_ROWS: usize = 3;
/// 标题字号。
const TITLE_PIXEL: f32 = 2.4;
/// 详情区字号。
const INFO_PIXEL: f32 = 1.6;
/// 拖动图标相对鼠标的偏移（像素）。
const HELD_OFFSET: f32 = 8.0;

/// 背包界面状态：开关、鼠标位置、正在拖动的物品与按压反馈。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InventoryUi {
    open: bool,
    cursor: (f32, f32),
    held: Option<ItemStack>,
    press_slot: Option<usize>,
    press: UiAnimation,
    ui_textures: UiTextureMap,
}

impl InventoryUi {
    pub fn new() -> Self {
        Self::default()
    }

    /// 替换 UI 控件图集；资源加载或热重载后由状态管理器调用。
    pub fn set_ui_textures(&mut self, textures: UiTextureMap) {
        self.ui_textures = textures;
    }

    #[cfg(test)]
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(&mut self) {
        self.open = true;
        self.press_slot = None;
        self.press = UiAnimation::default();
    }

    /// 推进按压反馈动画。
    pub fn update(&mut self, delta_secs: f32) {
        self.press.update(delta_secs);
        if !self.press.is_active() {
            self.press_slot = None;
        }
    }

    /// 尝试关闭界面：拖动中的物品先放回背包，放不下时保持界面开启，避免丢失。
    /// 回放必须通过 [`Inventory::add_stack`]，否则工具耐久会被重置。
    pub fn try_close(&mut self, inventory: &mut Inventory) -> bool {
        if let Some(held) = self.held {
            if let Some(remaining) = inventory.add_stack(held) {
                self.held = Some(remaining);
                return false;
            }
        }
        self.held = None;
        self.open = false;
        self.press_slot = None;
        true
    }

    /// 强制关闭界面（死亡等场景）：拖动中的物品优先放回背包，实在放不下则丢弃，
    /// 避免死亡界面被背包遮挡后玩家无法继续操作。
    pub fn force_close(&mut self, inventory: &mut Inventory) {
        if let Some(held) = self.held.take() {
            if let Some(remaining) = inventory.add_stack(held) {
                tracing::warn!(
                    item = ?remaining.item(),
                    count = remaining.count(),
                    "死亡时背包已满，拖动中的物品被丢弃"
                );
            }
        }
        self.open = false;
        self.press_slot = None;
    }

    /// 当前拖动中的物品。
    #[cfg(test)]
    pub fn held(&self) -> Option<ItemStack> {
        self.held
    }

    pub fn set_cursor(&mut self, x: f32, y: f32) {
        if x.is_finite() && y.is_finite() {
            self.cursor = (x, y);
        }
    }

    pub fn cursor(&self) -> (f32, f32) {
        self.cursor
    }

    /// 处理一次点击；返回背包内容是否发生变化。
    pub fn click_at(
        &mut self,
        inventory: &mut Inventory,
        button: ClickButton,
        cursor: (f32, f32),
        viewport: (f32, f32),
    ) -> bool {
        if !self.open {
            return false;
        }
        self.set_cursor(cursor.0, cursor.1);
        let Some(layout) = Layout::compute(viewport) else {
            return false;
        };
        let Some(index) = layout.slot_at(cursor) else {
            return false;
        };

        self.press_slot = Some(index);
        self.press.start(theme::FEEDBACK_DURATION);
        match button {
            ClickButton::Left => self.left_click(inventory, index),
            ClickButton::Right => self.right_click(inventory, index),
        }
    }

    fn left_click(&mut self, inventory: &mut Inventory, index: usize) -> bool {
        let existing = inventory.slot(index);
        match (self.held, existing) {
            (None, None) => false,
            (None, Some(_)) => {
                self.held = inventory.take_slot(index);
                true
            }
            (Some(held), None) => {
                inventory.set_slot(index, Some(held));
                self.held = None;
                true
            }
            (Some(held), Some(existing)) => {
                if held.can_merge_with(existing) {
                    self.held = inventory.insert_stack(index, held);
                } else {
                    self.held = inventory.set_slot(index, Some(held));
                }
                true
            }
        }
    }

    fn right_click(&mut self, inventory: &mut Inventory, index: usize) -> bool {
        let existing = inventory.slot(index);
        match (self.held, existing) {
            (None, None) => false,
            (None, Some(_)) => {
                self.held = inventory.split_half(index);
                true
            }
            (Some(held), None) => {
                let single = held.with_count(1).expect("单件合法");
                if inventory.insert_stack(index, single).is_some() {
                    return false;
                }
                self.held = held.with_count(held.count() - 1);
                true
            }
            (Some(held), Some(existing)) if held.can_merge_with(existing) => {
                if !inventory.add_one(index) {
                    return false;
                }
                self.held = held.with_count(held.count() - 1);
                true
            }
            (Some(_), Some(_)) => false,
        }
    }

    /// 生成背包面板几何；关闭时写入空列表。
    pub fn build(
        &self,
        out: &mut Vec<UiVertex>,
        inventory: &Inventory,
        selected_hotbar: usize,
        icons: &ItemTextureMap,
        viewport: (f32, f32),
    ) {
        out.clear();
        if !self.open {
            return;
        }
        let Some(layout) = Layout::compute(viewport) else {
            return;
        };
        let hovered = layout.slot_at(self.cursor);

        push_quad(
            out,
            viewport,
            UiRect::new(0.0, 0.0, viewport.0, viewport.1),
            theme::OVERLAY_LIGHT,
        );
        widget::push_panel(
            out,
            viewport,
            layout.panel,
            &self.ui_textures,
            "inventory_panel",
        );

        font::push_text(
            out,
            viewport,
            layout.panel.x + layout.padding,
            layout.panel.y + layout.padding,
            TITLE_PIXEL,
            theme::TEXT_TITLE,
            "INVENTORY",
        );
        let hint = "E / ESC TO CLOSE";
        let hint_width = font::text_width(hint, 1.4);
        font::push_text(
            out,
            viewport,
            layout.panel.x + layout.panel.width - layout.padding - hint_width,
            layout.panel.y + layout.padding + 2.0,
            1.4,
            theme::TEXT_MUTED,
            hint,
        );

        for index in 0..TOTAL_SLOTS {
            let rect = layout.slot_rect(index);
            let selected = index >= HOTBAR_OFFSET && index - HOTBAR_OFFSET == selected_hotbar;
            let stack = inventory.slot(index);
            let state = if selected {
                SlotState::Selected
            } else if Some(index) == hovered {
                SlotState::Hover
            } else if stack.is_some_and(ItemStack::is_broken) {
                SlotState::Disabled
            } else {
                SlotState::Normal
            };
            widget::push_slot(
                out,
                viewport,
                rect,
                state,
                stack,
                icons,
                &self.ui_textures,
                SlotSkin::Inventory,
                theme::SLOT_BACKGROUND,
            );
            if self.press_slot == Some(index) && self.press.is_active() {
                let alpha = 0.22 * self.press.decay();
                push_quad(
                    out,
                    viewport,
                    rect.inset(widget::SLOT_BORDER_THICKNESS),
                    [1.0, 1.0, 1.0, alpha],
                );
            }
        }

        let focus = hovered
            .and_then(|index| inventory.slot(index))
            .or(self.held)
            .or_else(|| inventory.hotbar_stack(selected_hotbar));
        push_info_area(out, viewport, layout, focus);

        if let Some(stack) = hovered.and_then(|index| inventory.slot(index)) {
            widget::push_item_tooltip(out, viewport, self.cursor, stack, icons, &self.ui_textures);
        }

        if let Some(held) = self.held {
            let size = layout.slot * 0.72;
            let rect = UiRect::new(
                self.cursor.0 + HELD_OFFSET,
                self.cursor.1 + HELD_OFFSET,
                size,
                size,
            );
            widget::push_icon(out, viewport, rect, held.item(), icons);
            widget::push_durability_strip(out, viewport, rect, held);
            if held.count() > 1 {
                let text = held.count().to_string();
                let text_width = font::text_width(&text, 2.0);
                let text_x = rect.right() - 2.0 - text_width;
                let text_y = rect.bottom() - 2.0 - font::GLYPH_HEIGHT as f32 * 2.0;
                font::push_text(
                    out,
                    viewport,
                    text_x + 1.0,
                    text_y + 1.0,
                    2.0,
                    theme::TEXT_SHADOW,
                    &text,
                );
                font::push_text(out, viewport, text_x, text_y, 2.0, theme::COUNT_TEXT, &text);
            }
        }
    }
}

/// 底部详情区：显示悬停物品、拖动物品或当前快捷栏选中物品。
fn push_info_area(
    out: &mut Vec<UiVertex>,
    viewport: (f32, f32),
    layout: Layout,
    stack: Option<ItemStack>,
) {
    let rect = layout.info;
    push_quad(out, viewport, rect, [0.03, 0.03, 0.04, 0.85]);
    push_border(out, viewport, rect, 1.0, theme::SLOT_BORDER);
    let Some(stack) = stack else {
        font::push_text(
            out,
            viewport,
            rect.x + 8.0,
            rect.y + 11.0,
            INFO_PIXEL,
            theme::TEXT_MUTED,
            "EMPTY HAND",
        );
        return;
    };
    let lines = widget::item_summary_lines(stack);
    let mut text_y = rect.y + 7.0;
    for (index, line) in lines.iter().enumerate() {
        let color = if index == 0 {
            theme::TEXT
        } else if stack.is_broken() {
            theme::TOOL_BROKEN
        } else {
            theme::TEXT_MUTED
        };
        font::push_text(
            out,
            viewport,
            rect.x + 9.0,
            text_y + 1.0,
            INFO_PIXEL,
            theme::TEXT_SHADOW,
            line,
        );
        font::push_text(out, viewport, rect.x + 8.0, text_y, INFO_PIXEL, color, line);
        text_y += font::GLYPH_HEIGHT as f32 * INFO_PIXEL + 4.0;
    }
}

/// 当前视口下的面板布局。
#[derive(Clone, Copy, Debug)]
struct Layout {
    panel: UiRect,
    padding: f32,
    title: f32,
    slot: f32,
    gap: f32,
    hotbar_gap: f32,
    info: UiRect,
}

impl Layout {
    fn compute(viewport: (f32, f32)) -> Option<Self> {
        if !super::layout::valid_viewport(viewport.0, viewport.1) {
            return None;
        }

        let base_width =
            BASE_PADDING * 2.0 + COLUMNS as f32 * BASE_SLOT + (COLUMNS - 1) as f32 * BASE_GAP;
        let base_height = BASE_PADDING * 2.0
            + BASE_TITLE
            + MAIN_ROWS as f32 * BASE_SLOT
            + (MAIN_ROWS - 1) as f32 * BASE_GAP
            + BASE_HOTBAR_GAP
            + BASE_SLOT
            + BASE_INFO_GAP
            + BASE_INFO_HEIGHT;
        let scale = ((viewport.0 - 32.0) / base_width)
            .min((viewport.1 - 32.0) / base_height)
            .clamp(0.4, 1.0);

        let panel = centered_panel(viewport, base_width * scale, base_height * scale);
        let padding = BASE_PADDING * scale;
        let title = BASE_TITLE * scale;
        let slot = BASE_SLOT * scale;
        let gap = BASE_GAP * scale;
        let hotbar_gap = BASE_HOTBAR_GAP * scale;
        let info = UiRect::new(
            panel.x + padding,
            panel.y
                + padding
                + title
                + MAIN_ROWS as f32 * slot
                + (MAIN_ROWS - 1) as f32 * gap
                + hotbar_gap
                + slot
                + BASE_INFO_GAP * scale,
            panel.width - padding * 2.0,
            BASE_INFO_HEIGHT * scale,
        );

        Some(Self {
            panel,
            padding,
            title,
            slot,
            gap,
            hotbar_gap,
            info,
        })
    }

    fn slot_rect(self, index: usize) -> UiRect {
        let (column, row_y) = if index < HOTBAR_OFFSET {
            let row = index / COLUMNS;
            (
                index % COLUMNS,
                self.panel.y + self.padding + self.title + row as f32 * (self.slot + self.gap),
            )
        } else {
            (
                index - HOTBAR_OFFSET,
                self.panel.y
                    + self.padding
                    + self.title
                    + MAIN_ROWS as f32 * self.slot
                    + (MAIN_ROWS - 1) as f32 * self.gap
                    + self.hotbar_gap,
            )
        };

        UiRect::new(
            self.panel.x + self.padding + column as f32 * (self.slot + self.gap),
            row_y,
            self.slot,
            self.slot,
        )
    }

    fn slot_at(self, cursor: (f32, f32)) -> Option<usize> {
        (0..TOTAL_SLOTS).find(|index| self.slot_rect(*index).contains(cursor.0, cursor.1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::ItemId;

    fn stone(count: u32) -> ItemStack {
        ItemStack::new(ItemId::STONE, count).expect("合法堆叠")
    }

    fn layout() -> Layout {
        Layout::compute((1280.0, 720.0)).expect("视口有效")
    }

    fn cursor_on(index: usize) -> (f32, f32) {
        let rect = layout().slot_rect(index);
        (rect.x + rect.width * 0.5, rect.y + rect.height * 0.5)
    }

    #[test]
    fn panels_fit_inside_small_viewports() {
        let layout = Layout::compute((480.0, 360.0)).expect("小视口也应有布局");
        assert!(layout.panel.x >= 0.0);
        assert!(layout.panel.y >= 0.0);
        assert!(layout.panel.right() <= 480.0);
        assert!(layout.panel.bottom() <= 360.0);
        assert!(layout.info.bottom() <= layout.panel.bottom());
    }

    #[test]
    fn layout_is_centered_at_supported_resolutions() {
        for viewport in [(1280.0, 720.0), (1600.0, 900.0), (1920.0, 1080.0)] {
            let layout = Layout::compute(viewport).expect("支持的分辨率");
            assert!((layout.panel.x + layout.panel.width * 0.5 - viewport.0 * 0.5).abs() < 0.01);
            assert!((layout.panel.y + layout.panel.height * 0.5 - viewport.1 * 0.5).abs() < 0.01);
        }
    }

    #[test]
    fn left_click_moves_stacks_between_slot_and_cursor() {
        let mut inventory = Inventory::new();
        inventory.set_slot(0, Some(stone(5)));
        let mut ui = InventoryUi::new();
        ui.open();

        assert!(ui.click_at(
            &mut inventory,
            ClickButton::Left,
            cursor_on(0),
            (1280.0, 720.0)
        ));
        assert_eq!(ui.held().map(ItemStack::count), Some(5));
        assert_eq!(inventory.slot(0), None);

        assert!(ui.click_at(
            &mut inventory,
            ClickButton::Left,
            cursor_on(27),
            (1280.0, 720.0)
        ));
        assert_eq!(ui.held(), None);
        assert_eq!(inventory.hotbar_stack(0).map(ItemStack::count), Some(5));
    }

    #[test]
    fn left_click_merges_same_items_and_swaps_different_items() {
        let mut inventory = Inventory::new();
        inventory.set_slot(0, Some(stone(60)));
        inventory.set_slot(1, Some(stone(60)));
        inventory.set_slot(2, Some(ItemStack::new(ItemId::DIRT, 2).expect("合法堆叠")));

        let mut ui = InventoryUi::new();
        ui.open();

        ui.click_at(
            &mut inventory,
            ClickButton::Left,
            cursor_on(0),
            (1280.0, 720.0),
        );
        ui.click_at(
            &mut inventory,
            ClickButton::Left,
            cursor_on(1),
            (1280.0, 720.0),
        );
        assert_eq!(inventory.slot(1).map(ItemStack::count), Some(64));
        assert_eq!(ui.held().map(ItemStack::count), Some(56));

        ui.click_at(
            &mut inventory,
            ClickButton::Left,
            cursor_on(2),
            (1280.0, 720.0),
        );
        assert_eq!(inventory.slot(2).map(ItemStack::item), Some(ItemId::STONE));
        assert_eq!(ui.held().map(ItemStack::item), Some(ItemId::DIRT));
    }

    #[test]
    fn right_click_splits_and_places_single_items() {
        let mut inventory = Inventory::new();
        inventory.set_slot(0, Some(stone(9)));
        let mut ui = InventoryUi::new();
        ui.open();

        ui.click_at(
            &mut inventory,
            ClickButton::Right,
            cursor_on(0),
            (1280.0, 720.0),
        );
        assert_eq!(ui.held().map(ItemStack::count), Some(5));
        assert_eq!(inventory.slot(0).map(ItemStack::count), Some(4));

        ui.click_at(
            &mut inventory,
            ClickButton::Right,
            cursor_on(1),
            (1280.0, 720.0),
        );
        assert_eq!(inventory.slot(1).map(ItemStack::count), Some(1));
        assert_eq!(ui.held().map(ItemStack::count), Some(4));
    }

    #[test]
    fn closing_stows_the_held_stack_back_into_the_inventory() {
        let mut inventory = Inventory::new();
        inventory.set_slot(0, Some(stone(7)));
        let mut ui = InventoryUi::new();
        ui.open();
        ui.click_at(
            &mut inventory,
            ClickButton::Left,
            cursor_on(0),
            (1280.0, 720.0),
        );
        assert_eq!(ui.held().map(ItemStack::count), Some(7));

        assert!(ui.try_close(&mut inventory));
        assert!(!ui.is_open());
        assert_eq!(ui.held(), None);
        assert_eq!(inventory.count_of(ItemId::STONE), 7);
    }

    #[test]
    fn clicks_outside_the_panel_do_nothing() {
        let mut inventory = Inventory::new();
        inventory.set_slot(0, Some(stone(1)));
        let mut ui = InventoryUi::new();
        ui.open();
        ui.set_cursor(0.0, 0.0);
        assert!(!ui.click_at(
            &mut inventory,
            ClickButton::Left,
            (0.0, 0.0),
            (1280.0, 720.0)
        ));
        assert_eq!(inventory.slot(0).map(ItemStack::count), Some(1));
        assert!(ui.press_slot.is_none());
    }

    #[test]
    fn geometry_is_only_generated_while_open() {
        let inventory = Inventory::new();
        let mut ui = InventoryUi::new();
        let mut vertices = Vec::new();
        ui.build(
            &mut vertices,
            &inventory,
            0,
            &ItemTextureMap::empty(),
            (1280.0, 720.0),
        );
        assert!(vertices.is_empty());

        ui.open();
        ui.build(
            &mut vertices,
            &inventory,
            0,
            &ItemTextureMap::empty(),
            (1280.0, 720.0),
        );
        assert_eq!(vertices.len() % 6, 0);
        for vertex in &vertices {
            assert!((-1.0..=1.0).contains(&vertex.position[0]));
            assert!((-1.0..=1.0).contains(&vertex.position[1]));
        }
    }

    #[test]
    fn every_slot_has_a_distinct_hit_area() {
        let layout = layout();
        for index in 0..TOTAL_SLOTS {
            assert_eq!(layout.slot_at(cursor_on(index)), Some(index));
        }
        assert_eq!(layout.slot_at((0.0, 0.0)), None);
    }

    #[test]
    fn closing_preserves_the_held_tool_durability() {
        let mut inventory = Inventory::new();
        let pickaxe = ItemStack::new(ItemId::WOODEN_PICKAXE, 1)
            .expect("工具堆叠合法")
            .with_durability(17);
        inventory.set_slot(0, Some(pickaxe));

        let mut ui = InventoryUi::new();
        ui.open();
        ui.click_at(
            &mut inventory,
            ClickButton::Left,
            cursor_on(0),
            (1280.0, 720.0),
        );
        assert_eq!(ui.held().and_then(ItemStack::durability), Some(17));

        assert!(ui.try_close(&mut inventory));
        assert_eq!(
            inventory
                .slot(HOTBAR_OFFSET)
                .and_then(ItemStack::durability),
            Some(17),
            "回放背包后耐久不能被重置"
        );
    }

    #[test]
    fn tool_slots_draw_extra_durability_geometry() {
        let viewport = (1280.0, 720.0);
        let mut ui = InventoryUi::new();
        ui.open();

        let mut inventory = Inventory::new();
        inventory.set_slot(0, Some(stone(1)));
        let mut block_vertices = Vec::new();
        ui.build(
            &mut block_vertices,
            &inventory,
            0,
            &ItemTextureMap::empty(),
            viewport,
        );

        inventory.set_slot(
            0,
            Some(
                ItemStack::new(ItemId::WOODEN_PICKAXE, 1)
                    .expect("工具堆叠合法")
                    .with_durability(30),
            ),
        );
        let mut tool_vertices = Vec::new();
        ui.build(
            &mut tool_vertices,
            &inventory,
            0,
            &ItemTextureMap::empty(),
            viewport,
        );

        assert_eq!(
            tool_vertices.len(),
            block_vertices.len() + 12,
            "工具槽位额外绘制耐久条背景与填充"
        );
    }

    #[test]
    fn the_info_area_reflects_the_selected_hotbar_item() {
        let viewport = (1280.0, 720.0);
        let mut ui = InventoryUi::new();
        ui.open();
        let mut inventory = Inventory::new();
        inventory.set_slot(HOTBAR_OFFSET, Some(stone(12)));

        let mut empty = Vec::new();
        ui.build(
            &mut empty,
            &Inventory::new(),
            0,
            &ItemTextureMap::empty(),
            viewport,
        );
        let mut selected = Vec::new();
        ui.build(
            &mut selected,
            &inventory,
            0,
            &ItemTextureMap::empty(),
            viewport,
        );
        assert!(
            selected.len() > empty.len(),
            "选中快捷栏物品时底部详情区应显示名称与类型"
        );
    }

    #[test]
    fn press_feedback_clears_after_the_animation() {
        let mut inventory = Inventory::new();
        inventory.set_slot(0, Some(stone(2)));
        let mut ui = InventoryUi::new();
        ui.open();
        ui.click_at(
            &mut inventory,
            ClickButton::Left,
            cursor_on(0),
            (1280.0, 720.0),
        );
        assert!(ui.press_slot.is_some());
        ui.update(theme::FEEDBACK_DURATION + 0.05);
        assert!(ui.press_slot.is_none());
    }
}
