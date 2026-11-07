//! 背包界面：27 格主背包 + 9 格快捷栏面板、左右键移动/拆分堆叠与悬停详情。
//!
//! 本模块只负责界面状态、布局与点击语义，不直接修改世界或存档；
//! 所有槽位变更都通过 [`Inventory`] 接口完成，因此不会产生重复或丢失物品。
//! 工具槽位额外显示耐久条，悬停信息包含类别、等级与当前/最大耐久。

use super::font;
use super::hud::{durability_color, push_quad, HudRect, HudVertex};
use crate::item::{
    icon_color, item_name, tool_registry, Inventory, ItemStack, HOTBAR_OFFSET, TOTAL_SLOTS,
};

/// 鼠标按键语义，避免界面层依赖具体输入框架。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickButton {
    Left,
    Right,
}

/// 基础布局尺寸（像素）。
const BASE_SLOT: f32 = 44.0;
const BASE_GAP: f32 = 6.0;
const BASE_PADDING: f32 = 12.0;
const BASE_TITLE: f32 = 24.0;
const BASE_HOTBAR_GAP: f32 = 14.0;
const COLUMNS: usize = 9;
const MAIN_ROWS: usize = 3;

const OVERLAY: [f32; 4] = [0.0, 0.0, 0.0, 0.4];
const PANEL_BORDER: [f32; 4] = [0.72, 0.74, 0.78, 0.95];
const PANEL_BACKGROUND: [f32; 4] = [0.10, 0.11, 0.12, 0.94];
const SLOT_BACKGROUND: [f32; 4] = [0.04, 0.05, 0.06, 0.85];
const SLOT_BORDER: [f32; 4] = [0.42, 0.44, 0.48, 0.8];
const SLOT_SELECTED: [f32; 4] = [1.0, 1.0, 1.0, 0.95];
const SLOT_HOVER: [f32; 4] = [0.85, 0.9, 1.0, 0.45];
const COUNT_TEXT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const TEXT_COLOR: [f32; 4] = [0.92, 0.94, 0.98, 1.0];
const TEXT_SHADOW: [f32; 4] = [0.0, 0.0, 0.0, 0.9];
const TOOLTIP_BACKGROUND: [f32; 4] = [0.05, 0.05, 0.06, 0.95];

/// 背包界面状态：开关、鼠标位置与正在拖动的物品。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InventoryUi {
    open: bool,
    cursor: (f32, f32),
    held: Option<ItemStack>,
}

impl InventoryUi {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(&mut self) {
        self.open = true;
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
        true
    }

    /// 当前拖动中的物品。
    #[allow(dead_code)]
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
        let Some(layout) = Layout::compute(viewport) else {
            return false;
        };
        let Some(index) = slot_at(layout, cursor) else {
            return false;
        };

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
        out: &mut Vec<HudVertex>,
        inventory: &Inventory,
        selected_hotbar: usize,
        viewport: (f32, f32),
    ) {
        out.clear();
        if !self.open {
            return;
        }
        let (width, height) = viewport;
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return;
        }
        let Some(layout) = Layout::compute(viewport) else {
            return;
        };
        let hovered = slot_at(layout, self.cursor);

        push_quad(
            out,
            viewport,
            HudRect {
                x: 0.0,
                y: 0.0,
                width,
                height,
            },
            OVERLAY,
        );
        push_quad(
            out,
            viewport,
            HudRect {
                x: layout.x - 3.0,
                y: layout.y - 3.0,
                width: layout.width + 6.0,
                height: layout.height + 6.0,
            },
            PANEL_BORDER,
        );
        push_quad(
            out,
            viewport,
            HudRect {
                x: layout.x,
                y: layout.y,
                width: layout.width,
                height: layout.height,
            },
            PANEL_BACKGROUND,
        );

        font::push_text(
            out,
            viewport,
            layout.x + layout.padding,
            layout.y + layout.padding,
            2.0,
            TEXT_COLOR,
            "INVENTORY",
        );

        for index in 0..TOTAL_SLOTS {
            let rect = layout.slot_rect(index);
            let selected = index >= HOTBAR_OFFSET && index - HOTBAR_OFFSET == selected_hotbar;
            let border = if selected { SLOT_SELECTED } else { SLOT_BORDER };
            push_quad(out, viewport, rect, border);
            push_quad(
                out,
                viewport,
                HudRect {
                    x: rect.x + 2.0,
                    y: rect.y + 2.0,
                    width: rect.width - 4.0,
                    height: rect.height - 4.0,
                },
                SLOT_BACKGROUND,
            );

            if Some(index) == hovered {
                push_quad(
                    out,
                    viewport,
                    HudRect {
                        x: rect.x + 2.0,
                        y: rect.y + 2.0,
                        width: rect.width - 4.0,
                        height: rect.height - 4.0,
                    },
                    SLOT_HOVER,
                );
            }

            if let Some(stack) = inventory.slot(index) {
                push_item_icon(out, viewport, rect, stack);
                push_slot_durability(out, viewport, rect, stack);
                push_count(out, viewport, rect, stack);
            }
        }

        if let Some(stack) = hovered.and_then(|index| inventory.slot(index)) {
            push_tooltip(out, viewport, layout, stack);
        }

        if let Some(held) = self.held {
            let size = layout.slot * 0.7;
            let rect = HudRect {
                x: self.cursor.0 + 8.0,
                y: self.cursor.1 + 8.0,
                width: size,
                height: size,
            };
            push_item_icon(out, viewport, rect, held);
            push_slot_durability(out, viewport, rect, held);
            if held.count() > 1 {
                let text = held.count().to_string();
                let text_width = font::text_width(&text, 2.0);
                let text_x = rect.x + rect.width - 2.0 - text_width;
                let text_y = rect.y + rect.height - 2.0 - font::GLYPH_HEIGHT as f32 * 2.0;
                font::push_text(
                    out,
                    viewport,
                    text_x + 1.0,
                    text_y + 1.0,
                    2.0,
                    TEXT_SHADOW,
                    &text,
                );
                font::push_text(out, viewport, text_x, text_y, 2.0, COUNT_TEXT, &text);
            }
        }
    }
}

fn push_item_icon(out: &mut Vec<HudVertex>, viewport: (f32, f32), rect: HudRect, stack: ItemStack) {
    let color = icon_color(stack.item());
    let inset = rect.width * 0.14;
    push_quad(
        out,
        viewport,
        HudRect {
            x: rect.x + inset,
            y: rect.y + inset,
            width: rect.width - inset * 2.0,
            height: rect.height - inset * 2.0,
        },
        [color.x, color.y, color.z, 0.95],
    );
}

fn push_count(out: &mut Vec<HudVertex>, viewport: (f32, f32), rect: HudRect, stack: ItemStack) {
    if stack.count() <= 1 {
        return;
    }
    let text = stack.count().to_string();
    let text_width = font::text_width(&text, 2.0);
    let text_x = rect.x + rect.width - 3.0 - text_width;
    let text_y = rect.y + rect.height - 3.0 - font::GLYPH_HEIGHT as f32 * 2.0;
    font::push_text(
        out,
        viewport,
        text_x + 1.0,
        text_y + 1.0,
        2.0,
        TEXT_SHADOW,
        &text,
    );
    font::push_text(out, viewport, text_x, text_y, 2.0, COUNT_TEXT, &text);
}

/// 槽位底部的耐久条；只有带独立耐久的工具绘制。
fn push_slot_durability(
    out: &mut Vec<HudVertex>,
    viewport: (f32, f32),
    rect: HudRect,
    stack: ItemStack,
) {
    let Some(max) = stack.max_durability() else {
        return;
    };
    if max == 0 {
        return;
    }

    let current = stack.durability().unwrap_or(max);
    let ratio = (current as f32 / max as f32).clamp(0.0, 1.0);
    let bar_height = (rect.height * 0.08).max(3.0);
    let inset = rect.width * 0.12;
    let bar = HudRect {
        x: rect.x + inset,
        y: rect.y + rect.height - inset - bar_height,
        width: rect.width - inset * 2.0,
        height: bar_height,
    };
    let background = if current == 0 {
        [0.55, 0.12, 0.12, 0.9]
    } else {
        [0.0, 0.0, 0.0, 0.75]
    };
    push_quad(out, viewport, bar, background);
    if ratio > 0.0 {
        push_quad(
            out,
            viewport,
            HudRect {
                width: bar.width * ratio,
                ..bar
            },
            durability_color(ratio),
        );
    }
}

/// 悬停详情：物品名；工具额外显示类别/等级与当前、最大耐久。
fn tooltip_lines(stack: ItemStack) -> Vec<String> {
    let mut lines = vec![item_name(stack.item()).to_string()];
    if let Some(definition) = tool_registry::definition(stack.item()) {
        lines.push(format!(
            "{} / {}",
            definition.tool_type.name(),
            definition.tier.name()
        ));
        let max = definition.max_durability;
        let current = stack.durability().unwrap_or(max);
        if current == 0 {
            lines.push("BROKEN".to_string());
        } else {
            lines.push(format!("DURABILITY {}/{}", current, max));
        }
    }
    lines
}

fn push_tooltip(out: &mut Vec<HudVertex>, viewport: (f32, f32), layout: Layout, stack: ItemStack) {
    let lines = tooltip_lines(stack);
    let line_height = font::GLYPH_HEIGHT as f32 * 2.0 + 4.0;
    let text_width = lines
        .iter()
        .map(|line| font::text_width(line, 2.0))
        .fold(0.0_f32, f32::max);
    let width = text_width + 16.0;
    let height = line_height * lines.len() as f32 + 8.0;
    let x = layout.x + (layout.width - width) * 0.5;
    let y = (layout.y - height - 8.0).max(4.0);
    push_quad(
        out,
        viewport,
        HudRect {
            x,
            y,
            width,
            height,
        },
        TOOLTIP_BACKGROUND,
    );
    for (index, line) in lines.iter().enumerate() {
        font::push_text(
            out,
            viewport,
            x + 8.0,
            y + 4.0 + index as f32 * line_height,
            2.0,
            TEXT_COLOR,
            line,
        );
    }
}

/// 当前视口下的面板布局。
#[derive(Clone, Copy, Debug)]
struct Layout {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    slot: f32,
    gap: f32,
    padding: f32,
    title: f32,
    hotbar_gap: f32,
}

impl Layout {
    fn compute(viewport: (f32, f32)) -> Option<Self> {
        let (width, height) = viewport;
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return None;
        }

        let base_width =
            BASE_PADDING * 2.0 + COLUMNS as f32 * BASE_SLOT + (COLUMNS - 1) as f32 * BASE_GAP;
        let base_height = BASE_PADDING * 2.0
            + BASE_TITLE
            + MAIN_ROWS as f32 * BASE_SLOT
            + (MAIN_ROWS - 1) as f32 * BASE_GAP
            + BASE_HOTBAR_GAP
            + BASE_SLOT;
        let scale = ((width - 32.0) / base_width)
            .min((height - 32.0) / base_height)
            .clamp(0.4, 1.0);

        let panel_width = base_width * scale;
        let panel_height = base_height * scale;
        let x = (width - panel_width) * 0.5;
        let y = (height - panel_height) * 0.5;

        Some(Self {
            x,
            y,
            width: panel_width,
            height: panel_height,
            slot: BASE_SLOT * scale,
            gap: BASE_GAP * scale,
            padding: BASE_PADDING * scale,
            title: BASE_TITLE * scale,
            hotbar_gap: BASE_HOTBAR_GAP * scale,
        })
    }

    fn slot_rect(self, index: usize) -> HudRect {
        let (column, row_y) = if index < HOTBAR_OFFSET {
            let row = index / COLUMNS;
            (
                index % COLUMNS,
                self.y + self.padding + self.title + row as f32 * (self.slot + self.gap),
            )
        } else {
            (
                index - HOTBAR_OFFSET,
                self.y
                    + self.padding
                    + self.title
                    + MAIN_ROWS as f32 * self.slot
                    + (MAIN_ROWS - 1) as f32 * self.gap
                    + self.hotbar_gap,
            )
        };

        HudRect {
            x: self.x + self.padding + column as f32 * (self.slot + self.gap),
            y: row_y,
            width: self.slot,
            height: self.slot,
        }
    }
}

fn slot_at(layout: Layout, cursor: (f32, f32)) -> Option<usize> {
    (0..TOTAL_SLOTS).find(|index| {
        let rect = layout.slot_rect(*index);
        cursor.0 >= rect.x
            && cursor.0 <= rect.x + rect.width
            && cursor.1 >= rect.y
            && cursor.1 <= rect.y + rect.height
    })
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
        assert!(layout.x >= 0.0);
        assert!(layout.y >= 0.0);
        assert!(layout.x + layout.width <= 480.0);
        assert!(layout.y + layout.height <= 360.0);
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

        // 60 与 60 合并到 64，剩余 56 回到光标。
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

        // 不同物品交换。
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
    }

    #[test]
    fn geometry_is_only_generated_while_open() {
        let inventory = Inventory::new();
        let mut ui = InventoryUi::new();
        let mut vertices = Vec::new();
        ui.build(&mut vertices, &inventory, 0, (1280.0, 720.0));
        assert!(vertices.is_empty());

        ui.open();
        ui.build(&mut vertices, &inventory, 0, (1280.0, 720.0));
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
            assert_eq!(slot_at(layout, cursor_on(index)), Some(index));
        }
        assert_eq!(slot_at(layout, (0.0, 0.0)), None);
    }

    #[test]
    fn tooltip_details_appear_only_for_tools() {
        let stone_lines = tooltip_lines(stone(1));
        assert_eq!(stone_lines, vec![item_name(ItemId::STONE).to_string()]);

        let pickaxe = ItemStack::new(ItemId::STONE_PICKAXE, 1)
            .expect("工具堆叠合法")
            .with_durability(100);
        let lines = tooltip_lines(pickaxe);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[1], "PICKAXE / STONE");
        assert_eq!(lines[2], "DURABILITY 100/132");

        let broken = pickaxe.with_durability(0);
        assert_eq!(tooltip_lines(broken)[2], "BROKEN");
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
        ui.build(&mut block_vertices, &inventory, 0, viewport);

        inventory.set_slot(
            0,
            Some(
                ItemStack::new(ItemId::WOODEN_PICKAXE, 1)
                    .expect("工具堆叠合法")
                    .with_durability(30),
            ),
        );
        let mut tool_vertices = Vec::new();
        ui.build(&mut tool_vertices, &inventory, 0, viewport);

        assert_eq!(
            tool_vertices.len(),
            block_vertices.len() + 12,
            "工具槽位额外绘制耐久条背景与填充"
        );
    }
}
