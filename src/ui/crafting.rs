//! 合成界面：左侧分类与配方列表、右侧配方详情与合成按钮。
//!
//! 本模块只负责界面状态、布局与点击语义；实际扣材料与产出由
//! [`crate::item::crafting`] 原子执行。界面每帧从背包读取材料数量，
//! 因此合成成功或外部拾取后，需求列表与按钮状态会立即更新。

use super::draw::{push_border, push_quad, push_ui_view, UiVertex};
use super::font;
use super::layout::{centered_panel, valid_viewport, UiRect};
use super::theme;
use super::widget;
use crate::item::crafting::{self, CraftError};
use crate::item::{item_name, Inventory, ItemStack, Recipe, RecipeCategory, RecipeId};
use crate::texture::{ItemTextureMap, UiTextureMap};

/// 面板基础尺寸（像素）。
const BASE_PANEL_WIDTH: f32 = 900.0;
const BASE_PANEL_HEIGHT: f32 = 620.0;
const PADDING: f32 = 18.0;
const HEADER_HEIGHT: f32 = 54.0;
const GUTTER: f32 = 20.0;
const LEFT_RATIO: f32 = 0.46;
const TAB_HEIGHT: f32 = 30.0;
const TAB_GAP: f32 = 6.0;
const LIST_GAP: f32 = 6.0;
const LIST_TOP_GAP: f32 = 10.0;
const ROW_HEIGHT: f32 = 44.0;
const MIN_ROW_HEIGHT: f32 = 10.0;
const CRAFT_WIDTH: f32 = 180.0;
const CRAFT_HEIGHT: f32 = 44.0;
const DETAIL_ICON_SIZE: f32 = 52.0;
const INGREDIENT_ROW: f32 = 24.0;
const INGREDIENT_GAP: f32 = 4.0;
const TITLE_PIXEL: f32 = 2.6;
const TEXT_PIXEL: f32 = 1.6;
const ROW_TEXT_PIXEL: f32 = 1.5;
const TAB_PIXEL: f32 = 1.5;
/// 成功/失败提示的显示时长（秒）。
const MESSAGE_DURATION: f32 = 2.5;
/// 提示最后阶段的淡出时长（秒）。
const MESSAGE_FADE: f32 = 0.4;

/// 分类筛选；`All` 展示全部已启用配方。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Filter {
    All,
    Category(RecipeCategory),
}

impl Filter {
    fn matches(self, recipe: &Recipe) -> bool {
        match self {
            Self::All => true,
            Self::Category(category) => recipe.category == category,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::All => "ALL",
            Self::Category(category) => category.label(),
        }
    }
}

/// 合成结果提示。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CraftMessage {
    Success {
        item: crate::item::ItemId,
        count: u32,
    },
    Failure(CraftError),
}

/// 合成界面状态。
#[derive(Clone, Debug, PartialEq)]
pub struct CraftingUi {
    open: bool,
    cursor: (f32, f32),
    filter: Filter,
    selected: Option<RecipeId>,
    message: Option<CraftMessage>,
    message_remaining: f32,
    flash: super::animation::UiAnimation,
    ui_textures: UiTextureMap,
}

impl Default for CraftingUi {
    fn default() -> Self {
        Self::new()
    }
}

impl CraftingUi {
    pub fn new() -> Self {
        Self {
            open: false,
            cursor: (0.0, 0.0),
            filter: Filter::All,
            selected: None,
            message: None,
            message_remaining: 0.0,
            flash: super::animation::UiAnimation::default(),
            ui_textures: UiTextureMap::empty(),
        }
    }

    /// 替换 UI 控件图集；资源加载或热重载后由状态管理器调用。
    pub fn set_ui_textures(&mut self, textures: UiTextureMap) {
        self.ui_textures = textures;
    }

    pub fn open(&mut self) {
        self.open = true;
        self.filter = Filter::All;
        self.selected = None;
        self.message = None;
        self.message_remaining = 0.0;
        self.flash = super::animation::UiAnimation::default();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.selected = None;
        self.message = None;
        self.message_remaining = 0.0;
    }

    pub fn set_cursor(&mut self, x: f32, y: f32) {
        if x.is_finite() && y.is_finite() {
            self.cursor = (x, y);
        }
    }

    pub fn cursor(&self) -> (f32, f32) {
        self.cursor
    }

    /// 记录一次成功合成，并启动短暂的成功闪光。
    pub fn show_success(&mut self, output: ItemStack) {
        self.message = Some(CraftMessage::Success {
            item: output.item(),
            count: output.count(),
        });
        self.message_remaining = MESSAGE_DURATION;
        self.flash.start(theme::FEEDBACK_DURATION);
    }

    /// 记录一次失败合成。
    pub fn show_failure(&mut self, error: CraftError) {
        self.message = Some(CraftMessage::Failure(error));
        self.message_remaining = MESSAGE_DURATION;
    }

    /// 推进提示与闪光计时。
    pub fn update(&mut self, delta_secs: f32) {
        if !delta_secs.is_finite() || delta_secs <= 0.0 {
            return;
        }
        self.flash.update(delta_secs);
        if self.message.is_some() {
            self.message_remaining = (self.message_remaining - delta_secs).max(0.0);
            if self.message_remaining <= 0.0 {
                self.message = None;
            }
        }
    }

    /// 处理一次左键点击；只有点击合成按钮时才返回需要外部执行的配方。
    pub fn click_at(
        &mut self,
        recipes: &[Recipe],
        viewport: (f32, f32),
        cursor: (f32, f32),
    ) -> Option<RecipeId> {
        if !self.open {
            return None;
        }
        self.set_cursor(cursor.0, cursor.1);
        let filters = filters(recipes);
        let visible = visible_recipes(recipes, self.filter);
        let layout = Layout::compute(viewport, filters.len(), visible.len())?;

        if let Some(index) = layout
            .tabs
            .iter()
            .position(|rect| rect.contains(cursor.0, cursor.1))
        {
            self.filter = filters[index];
            self.selected = None;
            return None;
        }

        if let Some(index) = layout
            .rows
            .iter()
            .position(|rect| rect.contains(cursor.0, cursor.1))
        {
            if let Some(recipe) = visible.get(index) {
                self.selected = Some(recipe.id);
            }
            return None;
        }

        if layout.craft.contains(cursor.0, cursor.1) {
            if let Some(recipe) = effective_selection(self.selected, &visible) {
                return Some(recipe.id);
            }
        }
        None
    }

    /// 生成合成界面几何；关闭时写入空列表。
    pub fn build(
        &self,
        out: &mut Vec<UiVertex>,
        recipes: &[Recipe],
        inventory: &Inventory,
        icons: &ItemTextureMap,
        viewport: (f32, f32),
    ) {
        out.clear();
        if !self.open {
            return;
        }
        let filters = filters(recipes);
        let visible = visible_recipes(recipes, self.filter);
        let Some(layout) = Layout::compute(viewport, filters.len(), visible.len()) else {
            return;
        };

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
            &self.ui_textures,
            "inventory_panel",
        );

        font::push_text(
            out,
            viewport,
            layout.panel.x + PADDING,
            layout.panel.y + PADDING,
            TITLE_PIXEL,
            theme::TEXT_TITLE,
            "CRAFTING",
        );
        let hint = "C / ESC TO CLOSE";
        let hint_width = font::text_width(hint, 1.4);
        font::push_text(
            out,
            viewport,
            layout.panel.x + layout.panel.width - PADDING - hint_width,
            layout.panel.y + PADDING + 4.0,
            1.4,
            theme::TEXT_MUTED,
            hint,
        );

        self.push_tabs(out, viewport, &layout, &filters);
        self.push_recipes(out, viewport, &layout, &visible, inventory, icons);

        let selection = effective_selection(self.selected, &visible);
        self.push_detail(out, viewport, &layout, selection, inventory, icons);
    }

    fn push_tabs(
        &self,
        out: &mut Vec<UiVertex>,
        viewport: (f32, f32),
        layout: &Layout,
        filters: &[Filter],
    ) {
        for (index, rect) in layout.tabs.iter().enumerate() {
            let filter = filters[index];
            let active = filter == self.filter;
            let hovered = rect.contains(self.cursor.0, self.cursor.1);
            push_quad(
                out,
                viewport,
                *rect,
                if active {
                    theme::CRAFT_CATEGORY_ACTIVE
                } else if hovered {
                    theme::CRAFT_ROW_HOVER
                } else {
                    theme::CRAFT_ROW_BACKGROUND
                },
            );
            push_border(
                out,
                viewport,
                *rect,
                1.0,
                if active {
                    theme::CRAFT_ROW_SELECTED
                } else {
                    theme::SLOT_BORDER
                },
            );
            let label = filter.label();
            let text_width = font::text_width(label, TAB_PIXEL);
            font::push_text(
                out,
                viewport,
                rect.x + (rect.width - text_width) * 0.5,
                rect.y + (rect.height - font::GLYPH_HEIGHT as f32 * TAB_PIXEL) * 0.5,
                TAB_PIXEL,
                if active {
                    theme::TEXT
                } else {
                    theme::TEXT_MUTED
                },
                label,
            );
        }
    }

    fn push_recipes(
        &self,
        out: &mut Vec<UiVertex>,
        viewport: (f32, f32),
        layout: &Layout,
        visible: &[&Recipe],
        inventory: &Inventory,
        icons: &ItemTextureMap,
    ) {
        for (index, rect) in layout.rows.iter().enumerate() {
            let Some(recipe) = visible.get(index) else {
                continue;
            };
            let selected =
                self.selected == Some(recipe.id) || (self.selected.is_none() && index == 0);
            let hovered = rect.contains(self.cursor.0, self.cursor.1);
            let texture = if selected {
                "crafting_row_selected"
            } else if hovered {
                "crafting_row_hover"
            } else {
                "crafting_row"
            };
            if let Some(view) = self.ui_textures.get(texture) {
                push_ui_view(out, viewport, *rect, view, [1.0; 4]);
            } else {
                push_quad(
                    out,
                    viewport,
                    *rect,
                    if selected {
                        theme::CRAFT_ROW_ACTIVE
                    } else if hovered {
                        theme::CRAFT_ROW_HOVER
                    } else {
                        theme::CRAFT_ROW_BACKGROUND
                    },
                );
            }
            push_border(
                out,
                viewport,
                *rect,
                if selected { 2.0 } else { 1.0 },
                if selected {
                    theme::CRAFT_ROW_SELECTED
                } else {
                    theme::SLOT_BORDER
                },
            );

            let icon_rect = UiRect::new(
                rect.x + 8.0,
                rect.y + (rect.height - 26.0) * 0.5,
                26.0,
                26.0,
            );
            push_quad(out, viewport, icon_rect, theme::SLOT_BACKGROUND);
            push_border(out, viewport, icon_rect, 1.0, theme::SLOT_BORDER);
            widget::push_icon(
                out,
                viewport,
                icon_rect.inset(2.0),
                recipe.output.item(),
                icons,
            );

            let craftable = crafting::check(recipe, inventory).is_ok();
            let name = item_name(recipe.output.item());
            font::push_text(
                out,
                viewport,
                icon_rect.right() + 10.0,
                rect.y + (rect.height - font::GLYPH_HEIGHT as f32 * ROW_TEXT_PIXEL) * 0.5,
                ROW_TEXT_PIXEL,
                if craftable {
                    theme::TEXT
                } else {
                    theme::TEXT_MUTED
                },
                name,
            );
            let count = format!("X{}", recipe.output_count());
            let count_width = font::text_width(&count, ROW_TEXT_PIXEL);
            font::push_text(
                out,
                viewport,
                rect.right() - 10.0 - count_width,
                rect.y + (rect.height - font::GLYPH_HEIGHT as f32 * ROW_TEXT_PIXEL) * 0.5,
                ROW_TEXT_PIXEL,
                if craftable {
                    theme::CRAFT_MATERIAL_OK
                } else {
                    theme::CRAFT_MATERIAL_MISSING
                },
                &count,
            );
        }
    }

    fn push_detail(
        &self,
        out: &mut Vec<UiVertex>,
        viewport: (f32, f32),
        layout: &Layout,
        selection: Option<&Recipe>,
        inventory: &Inventory,
        icons: &ItemTextureMap,
    ) {
        if self.flash.is_active() {
            let mut color = theme::CRAFT_SUCCESS;
            color[3] *= self.flash.decay();
            push_border(out, viewport, layout.right, 2.0, color);
        }

        let Some(recipe) = selection else {
            font::push_text(
                out,
                viewport,
                layout.right.x + 12.0,
                layout.right.y + 24.0,
                TEXT_PIXEL,
                theme::TEXT_MUTED,
                "SELECT A RECIPE",
            );
            return;
        };

        let status = crafting::check(recipe, inventory);
        let (status_text, status_color) = match status {
            Ok(()) => ("CRAFTABLE", theme::CRAFT_MATERIAL_OK),
            Err(error) => (error.message(), theme::CRAFT_MATERIAL_MISSING),
        };

        push_quad(out, viewport, layout.detail_icon, theme::SLOT_BACKGROUND);
        push_border(out, viewport, layout.detail_icon, 1.0, theme::SLOT_BORDER);
        widget::push_icon(
            out,
            viewport,
            layout.detail_icon.inset(6.0),
            recipe.output.item(),
            icons,
        );

        let name = item_name(recipe.output.item());
        font::push_text(
            out,
            viewport,
            layout.detail_icon.right() + 14.0,
            layout.detail_icon.y + 5.0,
            2.0,
            theme::TEXT,
            name,
        );
        let count = format!("X{}", recipe.output_count());
        font::push_text(
            out,
            viewport,
            layout.detail_icon.right() + 14.0,
            layout.detail_icon.y + 26.0,
            TEXT_PIXEL,
            theme::TEXT_MUTED,
            &count,
        );
        font::push_text(
            out,
            viewport,
            layout.right.x + 12.0,
            layout.status_y,
            TEXT_PIXEL,
            status_color,
            status_text,
        );

        font::push_text(
            out,
            viewport,
            layout.right.x + 12.0,
            layout.requirements_y,
            1.4,
            theme::TEXT_MUTED,
            "REQUIREMENTS",
        );
        let mut y = layout.ingredients_y;
        for ingredient in recipe.ingredients {
            let owned = inventory.count_of(ingredient.item);
            let enough = owned >= ingredient.count;
            let icon_rect = UiRect::new(layout.right.x + 12.0, y, 18.0, 18.0);
            push_quad(out, viewport, icon_rect, theme::SLOT_BACKGROUND);
            push_border(out, viewport, icon_rect, 1.0, theme::SLOT_BORDER);
            widget::push_icon(out, viewport, icon_rect.inset(2.0), ingredient.item, icons);
            let text = format!(
                "{} {}/{}",
                item_name(ingredient.item),
                owned.min(ingredient.count),
                ingredient.count
            );
            font::push_text(
                out,
                viewport,
                icon_rect.right() + 8.0,
                y + (18.0 - font::GLYPH_HEIGHT as f32 * ROW_TEXT_PIXEL) * 0.5,
                ROW_TEXT_PIXEL,
                if enough {
                    theme::CRAFT_MATERIAL_OK
                } else {
                    theme::CRAFT_MATERIAL_MISSING
                },
                &text,
            );
            y += INGREDIENT_ROW + INGREDIENT_GAP;
        }

        let hovered = layout.craft.contains(self.cursor.0, self.cursor.1);
        let enabled = status.is_ok();
        widget::push_button(
            out,
            viewport,
            layout.craft,
            "CRAFT",
            hovered,
            enabled,
            2.0,
            &self.ui_textures,
        );

        if let Some(message) = self.message {
            let alpha = message_alpha(self.message_remaining);
            let (text, mut color) = match message {
                CraftMessage::Success { item, count } => (
                    format!("+{} {} CRAFTED", count, item_name(item)),
                    theme::CRAFT_SUCCESS,
                ),
                CraftMessage::Failure(error) => (error.message().to_string(), theme::CRAFT_FAILURE),
            };
            color[3] *= alpha;
            let text_width = font::text_width(&text, TEXT_PIXEL);
            font::push_text(
                out,
                viewport,
                layout.right.x + (layout.right.width - text_width) * 0.5,
                layout.message_y,
                TEXT_PIXEL,
                color,
                &text,
            );
        }
    }
}

fn message_alpha(remaining: f32) -> f32 {
    if remaining <= 0.0 {
        0.0
    } else if remaining < MESSAGE_FADE {
        remaining / MESSAGE_FADE
    } else {
        1.0
    }
}

fn filters(recipes: &[Recipe]) -> Vec<Filter> {
    let mut filters = vec![Filter::All];
    for recipe in recipes.iter().filter(|recipe| recipe.enabled) {
        let filter = Filter::Category(recipe.category);
        if !filters.contains(&filter) {
            filters.push(filter);
        }
    }
    filters
}

fn visible_recipes(recipes: &[Recipe], filter: Filter) -> Vec<&Recipe> {
    recipes
        .iter()
        .filter(|recipe| recipe.enabled && filter.matches(recipe))
        .collect()
}

fn effective_selection<'a>(
    selected: Option<RecipeId>,
    visible: &'a [&'a Recipe],
) -> Option<&'a Recipe> {
    selected
        .and_then(|id| visible.iter().find(|recipe| recipe.id == id).copied())
        .or_else(|| visible.first().copied())
}

/// 当前视口下的两栏布局；只生成可完整显示的配方行。
#[derive(Clone, Debug)]
struct Layout {
    panel: UiRect,
    tabs: Vec<UiRect>,
    rows: Vec<UiRect>,
    right: UiRect,
    detail_icon: UiRect,
    status_y: f32,
    requirements_y: f32,
    ingredients_y: f32,
    craft: UiRect,
    message_y: f32,
}

impl Layout {
    fn compute(viewport: (f32, f32), tab_count: usize, recipe_count: usize) -> Option<Self> {
        if !valid_viewport(viewport.0, viewport.1) {
            return None;
        }
        let scale = ((viewport.0 - 40.0) / BASE_PANEL_WIDTH)
            .min((viewport.1 - 40.0) / BASE_PANEL_HEIGHT)
            .clamp(0.4, 1.0);
        let panel = centered_panel(
            viewport,
            BASE_PANEL_WIDTH * scale,
            BASE_PANEL_HEIGHT * scale,
        );
        let padding = PADDING * scale;
        let header = HEADER_HEIGHT * scale;
        let gutter = GUTTER * scale;
        let content = UiRect::new(
            panel.x + padding,
            panel.y + header,
            panel.width - padding * 2.0,
            panel.height - header - padding,
        );
        let left_width = (content.width - gutter) * LEFT_RATIO;
        let left = UiRect::new(content.x, content.y, left_width, content.height);
        let right = UiRect::new(
            left.right() + gutter,
            content.y,
            content.width - left_width - gutter,
            content.height,
        );

        let tab_gap = TAB_GAP * scale;
        let tab_height = TAB_HEIGHT * scale;
        let tabs: Vec<UiRect> = (0..tab_count.max(1))
            .map(|index| {
                let width = (left.width - tab_gap * (tab_count.max(1) - 1) as f32)
                    / tab_count.max(1) as f32;
                UiRect::new(
                    left.x + index as f32 * (width + tab_gap),
                    left.y,
                    width,
                    tab_height,
                )
            })
            .collect();

        let list_top = left.y + tab_height + LIST_TOP_GAP * scale;
        let list_height = left.bottom() - list_top;
        let gap = LIST_GAP * scale;
        let row_height = if recipe_count == 0 {
            0.0
        } else {
            ((list_height - gap * (recipe_count.saturating_sub(1)) as f32) / recipe_count as f32)
                .clamp(MIN_ROW_HEIGHT, ROW_HEIGHT * scale)
        };
        let fitted = if row_height <= 0.0 {
            0
        } else {
            (((list_height + gap) / (row_height + gap)).floor() as usize).min(recipe_count)
        };
        let rows: Vec<UiRect> = (0..fitted)
            .map(|index| {
                UiRect::new(
                    left.x,
                    list_top + index as f32 * (row_height + gap),
                    left.width,
                    row_height,
                )
            })
            .collect();

        let detail_icon = UiRect::new(
            right.x + 12.0,
            right.y + 12.0,
            DETAIL_ICON_SIZE * scale,
            DETAIL_ICON_SIZE * scale,
        );
        let status_y = detail_icon.bottom() + 14.0 * scale;
        let requirements_y = status_y + 22.0 * scale;
        let ingredients_y = requirements_y + 18.0 * scale;
        let craft = UiRect::new(
            right.right() - CRAFT_WIDTH * scale - 12.0,
            right.bottom() - CRAFT_HEIGHT * scale - 12.0,
            CRAFT_WIDTH * scale,
            CRAFT_HEIGHT * scale,
        );
        let message_y = craft.y - 22.0 * scale;

        Some(Self {
            panel,
            tabs,
            rows,
            right,
            detail_icon,
            status_y,
            requirements_y,
            ingredients_y,
            craft,
            message_y,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::recipe_registry;
    use crate::item::ItemId;

    fn viewport() -> (f32, f32) {
        (1280.0, 720.0)
    }

    fn layout_for(ui: &CraftingUi) -> Layout {
        let filters = filters(recipe_registry::all());
        let visible = visible_recipes(recipe_registry::all(), ui.filter);
        Layout::compute(viewport(), filters.len(), visible.len()).expect("视口有效")
    }

    fn row_center(rect: UiRect) -> (f32, f32) {
        (rect.x + rect.width * 0.5, rect.y + rect.height * 0.5)
    }

    #[test]
    fn opening_and_closing_resets_transient_state() {
        let mut ui = CraftingUi::new();
        assert!(!ui.open);
        ui.open();
        assert!(ui.open);
        ui.show_success(ItemStack::new(ItemId::STICK, 2).expect("合法产出"));
        assert!(ui.message.is_some());
        ui.close();
        assert!(!ui.open);
        assert!(ui.message.is_none());
    }

    #[test]
    fn categories_filter_the_recipe_list() {
        let mut ui = CraftingUi::new();
        ui.open();
        let filters = filters(recipe_registry::all());
        let layout = layout_for(&ui);
        let all_count = visible_recipes(recipe_registry::all(), Filter::All).len();
        assert_eq!(layout.rows.len(), all_count, "1280×720 应显示全部配方");

        let tools_index = filters
            .iter()
            .position(|filter| *filter == Filter::Category(RecipeCategory::Tools))
            .expect("工具分类存在");
        ui.click_at(
            recipe_registry::all(),
            viewport(),
            row_center(layout.tabs[tools_index]),
        );
        assert_eq!(ui.filter, Filter::Category(RecipeCategory::Tools));
        assert!(
            visible_recipes(recipe_registry::all(), ui.filter).len() < all_count,
            "工具分类只应显示工具配方"
        );
        assert!(ui.selected.is_none(), "切换分类后自动选中该分类首项");
    }

    #[test]
    fn clicking_a_recipe_row_selects_it() {
        let mut ui = CraftingUi::new();
        ui.open();
        let layout = layout_for(&ui);
        let visible = visible_recipes(recipe_registry::all(), ui.filter);
        let target = visible[3].id;
        ui.click_at(
            recipe_registry::all(),
            viewport(),
            row_center(layout.rows[3]),
        );
        assert_eq!(ui.selected, Some(target));
    }

    #[test]
    fn the_craft_button_returns_the_selected_recipe() {
        let mut ui = CraftingUi::new();
        ui.open();
        let layout = layout_for(&ui);
        let visible = visible_recipes(recipe_registry::all(), ui.filter);
        let expected = visible[0].id;
        assert_eq!(
            ui.click_at(recipe_registry::all(), viewport(), row_center(layout.craft)),
            Some(expected),
            "点击合成按钮应返回当前选中的配方"
        );
    }

    #[test]
    fn clicking_outside_interactive_areas_does_nothing() {
        let mut ui = CraftingUi::new();
        ui.open();
        assert_eq!(
            ui.click_at(recipe_registry::all(), viewport(), (2.0, 2.0)),
            None
        );
    }

    #[test]
    fn material_counts_update_immediately_from_the_inventory() {
        let stick = recipe_registry::by_id(RecipeId::STICK).expect("木棍配方存在");
        assert_eq!(
            crafting::check(stick, &Inventory::new()),
            Err(CraftError::InsufficientMaterials)
        );
        let mut inventory = Inventory::new();
        inventory.set_slot(0, Some(ItemStack::new(ItemId::WOOD, 2).expect("合法堆叠")));
        assert_eq!(crafting::check(stick, &inventory), Ok(()), "实时检查应通过");
    }

    #[test]
    fn success_messages_fade_and_expire() {
        let mut ui = CraftingUi::new();
        ui.open();
        ui.show_success(ItemStack::new(ItemId::STICK, 2).expect("合法产出"));
        assert_eq!(message_alpha(MESSAGE_DURATION), 1.0);
        assert!((message_alpha(MESSAGE_FADE * 0.5) - 0.5).abs() < 0.01);
        ui.update(MESSAGE_DURATION + 0.1);
        assert!(ui.message.is_none());
        assert!(!ui.flash.is_active());
    }

    #[test]
    fn failure_messages_show_a_specific_reason() {
        let mut ui = CraftingUi::new();
        ui.open();
        ui.show_failure(CraftError::InsufficientMaterials);
        assert_eq!(
            ui.message,
            Some(CraftMessage::Failure(CraftError::InsufficientMaterials))
        );
    }

    #[test]
    fn geometry_is_only_generated_while_open() {
        let mut ui = CraftingUi::new();
        let mut vertices = Vec::new();
        let inventory = Inventory::new();
        ui.build(
            &mut vertices,
            recipe_registry::all(),
            &inventory,
            &ItemTextureMap::empty(),
            viewport(),
        );
        assert!(vertices.is_empty());

        ui.open();
        ui.build(
            &mut vertices,
            recipe_registry::all(),
            &inventory,
            &ItemTextureMap::empty(),
            viewport(),
        );
        assert_eq!(vertices.len() % 6, 0);
        for vertex in &vertices {
            assert!((-1.0..=1.0).contains(&vertex.position[0]));
            assert!((-1.0..=1.0).contains(&vertex.position[1]));
        }
    }

    #[test]
    fn layout_fits_supported_resolutions() {
        for viewport in [(1280.0, 720.0), (1600.0, 900.0), (1920.0, 1080.0)] {
            let layout = Layout::compute(viewport, 3, 10).expect("支持的分辨率");
            assert!(layout.panel.x >= 0.0 && layout.panel.y >= 0.0);
            assert!(layout.panel.right() <= viewport.0);
            assert!(layout.panel.bottom() <= viewport.1);
            assert!(layout.craft.bottom() <= layout.right.bottom());
            assert_eq!(layout.rows.len(), 10);
        }
    }

    #[test]
    fn rows_are_clipped_when_the_panel_is_too_small() {
        let layout = Layout::compute((480.0, 200.0), 3, 10).expect("小视口也应有布局");
        assert!(layout.rows.len() < 10, "放不下的配方行不应生成点击区域");
        for row in &layout.rows {
            assert!(row.bottom() <= layout.tabs[0].bottom() + 200.0 || row.y > layout.tabs[0].y);
            assert!(layout.panel.contains(row.x, row.y));
        }
    }
}
