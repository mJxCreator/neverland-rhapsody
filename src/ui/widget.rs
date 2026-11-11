//! 可复用 UI 控件：槽位、面板、按钮、物品提示框与物品描述。
//!
//! 各界面只负责布局与数据，具体视觉细节集中在这里，保证背包、快捷栏与合成界面
//! 使用同一套边框、留白、数量与耐久表现。

use crate::item::item_registry::definition;
use crate::item::tool_registry;
use crate::item::{item_name, ItemId, ItemStack};
use crate::texture::ItemTextureMap;

use super::draw::{push_border, push_item_icon, push_quad, push_ui_view, UiVertex};
use super::font;
use super::layout::{clamp_to_viewport, UiRect};
use super::theme;
use crate::texture::UiTextureMap;

/// 槽位所属的界面类型；决定使用快捷栏还是背包的 UI 纹理。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotSkin {
    Hotbar,
    Inventory,
}

/// 槽位外框厚度（像素）。
pub const SLOT_BORDER_THICKNESS: f32 = 3.0;
/// 槽位内部留白（像素）。
pub const SLOT_PADDING: f32 = 7.0;
/// 槽位数量文字的字号（像素格）。
pub const SLOT_COUNT_PIXEL: f32 = 2.0;
/// 物品提示框字号（像素格）。
pub const TOOLTIP_PIXEL: f32 = 2.0;
/// 提示框内边距（像素）。
pub const TOOLTIP_PADDING: f32 = 8.0;
/// 提示框行距（像素）。
pub const TOOLTIP_LINE_GAP: f32 = 4.0;
/// 提示框物品图标边长（像素）。
pub const TOOLTIP_ICON_SIZE: f32 = 24.0;

/// 槽位的视觉状态；决定边框、悬停与禁用表现。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotState {
    Normal,
    Hover,
    Selected,
    Disabled,
}

impl SlotState {
    /// 当前状态对应的边框颜色。
    pub fn border_color(self) -> [f32; 4] {
        match self {
            Self::Normal => theme::SLOT_BORDER,
            Self::Hover => theme::SLOT_HOVER,
            Self::Selected => theme::SLOT_SELECTED,
            Self::Disabled => theme::SLOT_DISABLED,
        }
    }
}

/// 绘制一个完整槽位：边框、底板、空槽、图标、数量与耐久条。
#[allow(clippy::too_many_arguments)]
pub fn push_slot(
    out: &mut Vec<UiVertex>,
    viewport: (f32, f32),
    rect: UiRect,
    state: SlotState,
    stack: Option<ItemStack>,
    icons: &ItemTextureMap,
    ui: &UiTextureMap,
    skin: SlotSkin,
    background: [f32; 4],
) {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let textured = ui
        .get(slot_texture(skin, state))
        .map(|view| push_ui_view(out, viewport, rect, view, [1.0; 4]))
        .is_some();
    if !textured {
        push_quad(out, viewport, rect, state.border_color());
        let inner = rect.inset(SLOT_BORDER_THICKNESS);
        push_quad(out, viewport, inner, background);
    }

    let content = rect.inset(SLOT_PADDING);
    match stack {
        None => {
            if !textured {
                push_quad(out, viewport, content, theme::SLOT_EMPTY);
            }
        }
        Some(stack) => {
            push_item_icon(out, viewport, content, stack.item(), icons);
            push_durability_strip(out, viewport, rect, stack);
            if stack.count() > 1 {
                let text = stack.count().to_string();
                let text_width = font::text_width(&text, SLOT_COUNT_PIXEL);
                let text_y =
                    rect.y + rect.height - 4.0 - font::GLYPH_HEIGHT as f32 * SLOT_COUNT_PIXEL;
                let text_x = rect.x + rect.width - 4.0 - text_width;
                font::push_text(
                    out,
                    viewport,
                    text_x + 1.0,
                    text_y + 1.0,
                    SLOT_COUNT_PIXEL,
                    theme::TEXT_SHADOW,
                    &text,
                );
                font::push_text(
                    out,
                    viewport,
                    text_x,
                    text_y,
                    SLOT_COUNT_PIXEL,
                    theme::COUNT_TEXT,
                    &text,
                );
            }
            if state == SlotState::Disabled {
                let inner = rect.inset(SLOT_BORDER_THICKNESS);
                push_quad(out, viewport, inner, theme::SLOT_DISABLED);
            }
        }
    }

    if state == SlotState::Hover && !textured {
        push_border(out, viewport, rect, 1.0, theme::SLOT_HOVER);
    }
}

fn slot_texture(skin: SlotSkin, state: SlotState) -> &'static str {
    match (skin, state) {
        (SlotSkin::Hotbar, SlotState::Selected) => "hotbar_slot_selected",
        (SlotSkin::Hotbar, _) => "hotbar_slot",
        (SlotSkin::Inventory, SlotState::Hover) => "inventory_slot_hover",
        (SlotSkin::Inventory, SlotState::Selected) => "inventory_slot_selected",
        (SlotSkin::Inventory, SlotState::Normal | SlotState::Disabled) => "inventory_slot",
    }
}

/// 槽位底部的耐久条；普通物品不绘制。
pub fn push_durability_strip(
    out: &mut Vec<UiVertex>,
    viewport: (f32, f32),
    rect: UiRect,
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
    let bar = UiRect::new(
        rect.x + SLOT_PADDING,
        rect.y + rect.height - SLOT_PADDING - 4.0,
        rect.width - SLOT_PADDING * 2.0,
        4.0,
    );
    let background = if current == 0 {
        theme::DURABILITY_BAR_BROKEN
    } else {
        theme::DURABILITY_BAR_BACKGROUND
    };
    push_quad(out, viewport, bar, background);
    if ratio > 0.0 {
        push_quad(
            out,
            viewport,
            UiRect {
                width: bar.width * ratio,
                ..bar
            },
            durability_color(ratio),
        );
    }
}

/// 按剩余耐久比例选择颜色：低于 25% 警示，低于 50% 过渡。
pub fn durability_color(ratio: f32) -> [f32; 4] {
    if ratio <= 0.25 {
        theme::DURABILITY_LOW
    } else if ratio <= 0.5 {
        theme::DURABILITY_MID
    } else {
        theme::DURABILITY_HIGH
    }
}

/// 绘制面板：优先使用 UI 图集中的控件纹理，缺失时回退投影、深色底板与细边框。
pub fn push_panel(
    out: &mut Vec<UiVertex>,
    viewport: (f32, f32),
    rect: UiRect,
    ui: &UiTextureMap,
    texture: &str,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    if let Some(view) = ui.get(texture) {
        push_ui_view(out, viewport, rect, view, [1.0; 4]);
        return;
    }
    push_quad(
        out,
        viewport,
        UiRect {
            x: rect.x + 4.0,
            y: rect.y + 5.0,
            ..rect
        },
        theme::PANEL_SHADOW,
    );
    push_quad(out, viewport, rect, theme::PANEL_BACKGROUND);
    push_border(out, viewport, rect, 2.0, theme::PANEL_BORDER);
}

/// 绘制通用按钮；优先使用 UI 图集，`enabled` 为 false 时降低文字亮度。
#[allow(clippy::too_many_arguments)]
pub fn push_button(
    out: &mut Vec<UiVertex>,
    viewport: (f32, f32),
    rect: UiRect,
    label: &str,
    hovered: bool,
    enabled: bool,
    pixel: f32,
    ui: &UiTextureMap,
) {
    let texture = if hovered && enabled {
        "button_hover"
    } else {
        "button"
    };
    let textured = ui
        .get(texture)
        .map(|view| push_ui_view(out, viewport, rect, view, [1.0; 4]))
        .is_some();
    if !textured {
        let background = if !enabled {
            theme::BUTTON_DISABLED
        } else if hovered {
            theme::BUTTON_HOVER
        } else {
            theme::BUTTON_BACKGROUND
        };
        push_quad(out, viewport, rect, background);
        push_border(out, viewport, rect, 1.5, theme::BUTTON_BORDER);
    }
    let text_width = font::text_width(label, pixel);
    let x = rect.x + (rect.width - text_width) * 0.5;
    let y = rect.y + (rect.height - font::GLYPH_HEIGHT as f32 * pixel) * 0.5;
    let color = if enabled {
        theme::TEXT
    } else {
        theme::TEXT_MUTED
    };
    font::push_text(
        out,
        viewport,
        x + 1.0,
        y + 1.0,
        pixel,
        theme::TEXT_SHADOW,
        label,
    );
    font::push_text(out, viewport, x, y, pixel, color, label);
}

/// 物品详情区使用的两行摘要：名称与类型/耐久。
pub fn item_summary_lines(stack: ItemStack) -> Vec<String> {
    let name = item_name(stack.item()).to_uppercase();
    let first = if stack.count() > 1 {
        format!("{} X{}", name, stack.count())
    } else {
        name.to_string()
    };
    let second = if crate::combat::weapon_registry::definition(stack.item()).is_some() {
        weapon_summary(stack)
    } else if crate::equipment::equipment_registry::definition(stack.item()).is_some() {
        armor_summary(stack)
    } else if stack.is_tool() {
        tool_summary(stack)
    } else {
        type_label(stack.item()).to_string()
    };
    vec![first, second]
}

/// 完整物品提示内容：名称、类型与工具耐久。
pub fn item_detail_lines(stack: ItemStack) -> Vec<(String, [f32; 4])> {
    let mut lines = Vec::new();
    let name = item_name(stack.item()).to_uppercase();
    let first = if stack.count() > 1 {
        format!("{} X{}", name, stack.count())
    } else {
        name.to_string()
    };
    lines.push((first, theme::TEXT));
    lines.push((type_label(stack.item()).to_string(), theme::TEXT_MUTED));
    if stack.is_broken() {
        lines.push(("BROKEN".to_string(), theme::TOOL_BROKEN));
    } else if crate::combat::weapon_registry::definition(stack.item()).is_some() {
        lines.push((weapon_summary(stack), tool_summary_color(stack)));
    } else if crate::equipment::equipment_registry::definition(stack.item()).is_some() {
        lines.push((armor_summary(stack), tool_summary_color(stack)));
    } else if stack.is_tool() {
        lines.push((tool_summary(stack), tool_summary_color(stack)));
    } else if stack.max_stack_size() > 1 {
        lines.push((
            format!("STACK {}/{}", stack.count(), stack.max_stack_size()),
            theme::TEXT_MUTED,
        ));
    }
    lines
}

fn type_label(item: ItemId) -> &'static str {
    match definition(item).map(|definition| definition.item_type) {
        Some(crate::item::item::ItemType::Block) => "BLOCK",
        Some(crate::item::item::ItemType::Tool) => "TOOL",
        Some(crate::item::item::ItemType::Weapon) => "WEAPON",
        Some(crate::item::item::ItemType::Armor) => "ARMOR",
        Some(crate::item::item::ItemType::Consumable) => "CONSUMABLE",
        _ => "MATERIAL",
    }
}

fn tool_summary(stack: ItemStack) -> String {
    let Some(max) = stack.max_durability() else {
        return "TOOL".to_string();
    };
    let current = stack.durability().unwrap_or(max);
    let kind = tool_registry::definition(stack.item())
        .map(|definition| format!("{} {}", definition.tool_type.name(), definition.tier.name()))
        .unwrap_or_else(|| "TOOL".to_string());
    if current == 0 {
        format!("{} BROKEN", kind)
    } else {
        format!("{} {}/{}", kind, current, max)
    }
}

fn weapon_summary(stack: ItemStack) -> String {
    let Some(definition) = crate::combat::weapon_registry::definition(stack.item()) else {
        return "WEAPON".to_string();
    };
    let current = stack.durability().unwrap_or(definition.max_durability);
    let kind = definition.weapon_type.name();
    if current == 0 {
        format!("{} {} DMG BROKEN", kind, definition.damage)
    } else {
        format!(
            "{} {} DMG {}/{}",
            kind, definition.damage, current, definition.max_durability
        )
    }
}

fn armor_summary(stack: ItemStack) -> String {
    let Some(definition) = crate::equipment::equipment_registry::definition(stack.item()) else {
        return "ARMOR".to_string();
    };
    let current = stack.durability().unwrap_or(definition.max_durability);
    definition.summary(current)
}

fn tool_summary_color(stack: ItemStack) -> [f32; 4] {
    let Some(max) = stack.max_durability() else {
        return theme::TEXT_MUTED;
    };
    if max == 0 {
        return theme::TEXT_MUTED;
    }
    let current = stack.durability().unwrap_or(max);
    if current == 0 {
        theme::TOOL_BROKEN
    } else {
        durability_color(current as f32 / max as f32)
    }
}

/// 绘制跟随鼠标的物品提示框，自动选择显示方向并限制在屏幕内。
///
/// 返回提示框矩形，便于测试位置与越界行为。
pub fn push_item_tooltip(
    out: &mut Vec<UiVertex>,
    viewport: (f32, f32),
    cursor: (f32, f32),
    stack: ItemStack,
    icons: &ItemTextureMap,
    ui: &UiTextureMap,
) -> UiRect {
    let lines = item_detail_lines(stack);
    let text_width = lines
        .iter()
        .map(|(line, _)| font::text_width(line, TOOLTIP_PIXEL))
        .fold(0.0_f32, f32::max);
    let line_height = font::GLYPH_HEIGHT as f32 * TOOLTIP_PIXEL;
    let has_icon = icons.icon(stack.item()).is_some();
    let icon_span = if has_icon {
        TOOLTIP_ICON_SIZE + 8.0
    } else {
        0.0
    };
    let width = text_width + icon_span + TOOLTIP_PADDING * 2.0;
    let height = lines.len() as f32 * line_height
        + (lines.len().saturating_sub(1)) as f32 * TOOLTIP_LINE_GAP
        + TOOLTIP_PADDING * 2.0;

    let mut rect = UiRect::new(cursor.0 + 14.0, cursor.1 + 14.0, width, height);
    if rect.right() > viewport.0 - 4.0 {
        rect.x = cursor.0 - width - 14.0;
    }
    if rect.bottom() > viewport.1 - 4.0 {
        rect.y = cursor.1 - height - 14.0;
    }
    let rect = clamp_to_viewport(rect, viewport, 4.0);

    if let Some(view) = ui.get("tooltip_bg") {
        push_ui_view(out, viewport, rect, view, [1.0; 4]);
    } else {
        push_quad(out, viewport, rect, theme::TOOLTIP_BACKGROUND);
        push_border(out, viewport, rect, 1.0, theme::TOOLTIP_BORDER);
    }

    if has_icon {
        push_item_icon(
            out,
            viewport,
            UiRect::new(
                rect.x + TOOLTIP_PADDING,
                rect.y + TOOLTIP_PADDING,
                TOOLTIP_ICON_SIZE,
                TOOLTIP_ICON_SIZE,
            ),
            stack.item(),
            icons,
        );
    }

    let text_x = rect.x + TOOLTIP_PADDING + icon_span;
    let mut text_y = rect.y + TOOLTIP_PADDING;
    for (line, color) in &lines {
        font::push_text(
            out,
            viewport,
            text_x + 1.0,
            text_y + 1.0,
            TOOLTIP_PIXEL,
            theme::TEXT_SHADOW,
            line,
        );
        font::push_text(out, viewport, text_x, text_y, TOOLTIP_PIXEL, *color, line);
        text_y += line_height + TOOLTIP_LINE_GAP;
    }
    rect
}

/// 绘制物品图集图标；图集缺少该物品时不产生几何。
pub fn push_icon(
    out: &mut Vec<UiVertex>,
    viewport: (f32, f32),
    rect: UiRect,
    item: ItemId,
    icons: &ItemTextureMap,
) {
    push_item_icon(out, viewport, rect, item, icons);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::ItemId;

    fn stone(count: u32) -> ItemStack {
        ItemStack::new(ItemId::STONE, count).expect("合法堆叠")
    }

    fn pickaxe(durability: u32) -> ItemStack {
        ItemStack::new(ItemId::WOODEN_PICKAXE, 1)
            .expect("工具堆叠合法")
            .with_durability(durability)
    }

    #[test]
    fn slot_draws_icon_count_and_durability() {
        let mut vertices = Vec::new();
        push_slot(
            &mut vertices,
            (1280.0, 720.0),
            UiRect::new(10.0, 10.0, 48.0, 48.0),
            SlotState::Normal,
            Some(pickaxe(30)),
            &ItemTextureMap::empty(),
            &UiTextureMap::empty(),
            SlotSkin::Hotbar,
            theme::HOTBAR_BACKGROUND,
        );
        // 边框 + 底板 + 图标缺失 + 耐久背景/填充。
        assert_eq!(vertices.len(), 6 + 6 + 12);
    }

    #[test]
    fn disabled_slot_adds_a_dimming_overlay() {
        let mut normal = Vec::new();
        let mut disabled = Vec::new();
        push_slot(
            &mut normal,
            (1280.0, 720.0),
            UiRect::new(0.0, 0.0, 48.0, 48.0),
            SlotState::Normal,
            Some(pickaxe(0)),
            &ItemTextureMap::empty(),
            &UiTextureMap::empty(),
            SlotSkin::Hotbar,
            theme::HOTBAR_BACKGROUND,
        );
        push_slot(
            &mut disabled,
            (1280.0, 720.0),
            UiRect::new(0.0, 0.0, 48.0, 48.0),
            SlotState::Disabled,
            Some(pickaxe(0)),
            &ItemTextureMap::empty(),
            &UiTextureMap::empty(),
            SlotSkin::Hotbar,
            theme::HOTBAR_BACKGROUND,
        );
        assert_eq!(disabled.len(), normal.len() + 6);
    }

    #[test]
    fn tooltip_lines_describe_tools_and_materials() {
        let tool = item_detail_lines(pickaxe(30));
        assert!(tool.iter().any(|(line, _)| line == "WOODEN PICKAXE"));
        assert!(tool.iter().any(|(line, _)| line == "TOOL"));
        assert!(tool.iter().any(|(line, _)| line.contains("30/60")));

        let material = item_summary_lines(stone(5));
        assert_eq!(material[0], "STONE X5");
        assert_eq!(material[1], "BLOCK");
    }

    #[test]
    fn tooltip_is_flipped_away_from_edges() {
        let mut vertices = Vec::new();
        let rect = push_item_tooltip(
            &mut vertices,
            (1280.0, 720.0),
            (1270.0, 710.0),
            pickaxe(30),
            &ItemTextureMap::empty(),
            &UiTextureMap::empty(),
        );
        assert!(rect.right() <= 1276.0);
        assert!(rect.bottom() <= 716.0);
        assert!(!vertices.is_empty());
    }
}
