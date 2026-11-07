//! 合成界面：配方列表、材料持有量、分类标签、悬停高亮与点击合成。
//!
//! UI 只负责展示与命中检测；实际合成由 [`crate::item::crafting`] 执行，
//! 因此刷新界面不会重复产出物品。

use crate::item::{crafting::CraftError, icon_color, item_name, Inventory, Recipe, RecipeId};
use crate::renderer::font;
use crate::renderer::hud::{push_quad, HudRect, HudVertex};

const PANEL_MAX_WIDTH: f32 = 620.0;
const PANEL_MARGIN: f32 = 20.0;
const PANEL_PADDING: f32 = 16.0;
const HEADER_HEIGHT: f32 = 52.0;
const ROW_HEIGHT: f32 = 40.0;
const ROW_GAP: f32 = 4.0;
const FOOTER_HEIGHT: f32 = 40.0;
const ICON_SIZE: f32 = 24.0;

const OVERLAY: [f32; 4] = [0.03, 0.04, 0.06, 0.78];
const PANEL: [f32; 4] = [0.10, 0.11, 0.14, 0.96];
const PANEL_BORDER: [f32; 4] = [0.42, 0.44, 0.52, 1.0];
const ROW: [f32; 4] = [0.15, 0.16, 0.20, 1.0];
const ROW_HOVER: [f32; 4] = [0.22, 0.26, 0.34, 1.0];
const ROW_BORDER: [f32; 4] = [0.30, 0.32, 0.38, 1.0];
const TITLE: [f32; 4] = [1.0, 0.96, 0.86, 1.0];
const MUTED: [f32; 4] = [0.70, 0.74, 0.82, 1.0];
const INGREDIENT_OK: [f32; 4] = [0.56, 0.86, 0.52, 1.0];
const INGREDIENT_MISSING: [f32; 4] = [0.95, 0.42, 0.38, 1.0];
const SUCCESS: [f32; 4] = [0.60, 0.92, 0.58, 1.0];
const FAILURE: [f32; 4] = [0.98, 0.46, 0.42, 1.0];

/// 合成后的反馈信息。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CraftMessage {
    Success { name: &'static str, count: u32 },
    Failure(CraftError),
}

impl CraftMessage {
    fn text(self) -> String {
        match self {
            Self::Success { name, count } => format!("CRAFTED {} X{}", name.to_uppercase(), count),
            Self::Failure(error) => error.message().to_string(),
        }
    }

    fn color(self) -> [f32; 4] {
        match self {
            Self::Success { .. } => SUCCESS,
            Self::Failure(_) => FAILURE,
        }
    }
}

/// 合成界面状态。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CraftingUi {
    open: bool,
    cursor: (f32, f32),
    message: Option<CraftMessage>,
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
            message: None,
        }
    }

    pub const fn is_open(self) -> bool {
        self.open
    }

    pub fn open(&mut self) {
        self.open = true;
        self.message = None;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.message = None;
    }

    pub fn set_cursor(&mut self, cursor: (f32, f32)) {
        if cursor.0.is_finite() && cursor.1.is_finite() {
            self.cursor = cursor;
        }
    }

    pub const fn cursor(self) -> (f32, f32) {
        self.cursor
    }

    /// 记录一次合成成功；界面只消费结果，不参与物品变更。
    pub fn show_success(&mut self, output: crate::item::ItemStack) {
        self.message = Some(CraftMessage::Success {
            name: item_name(output.item()),
            count: output.count(),
        });
    }

    /// 记录一次合成失败并展示原因。
    pub fn show_failure(&mut self, error: CraftError) {
        self.message = Some(CraftMessage::Failure(error));
    }

    /// 命中检测：返回被点击的配方 ID；点击空白处返回 `None`。
    pub fn click_at(
        &self,
        recipes: &[Recipe],
        viewport: (f32, f32),
        cursor: (f32, f32),
    ) -> Option<RecipeId> {
        if !self.open {
            return None;
        }
        let layout = Layout::new(recipes.len(), viewport, self.message);
        for (index, stored) in layout.rows.iter().enumerate() {
            if contains(stored, cursor) {
                return recipes.get(index).map(|recipe| recipe.id);
            }
        }
        None
    }

    /// 生成合成界面顶点。
    pub fn build(
        &self,
        out: &mut Vec<HudVertex>,
        recipes: &[Recipe],
        inventory: &Inventory,
        viewport: (f32, f32),
    ) {
        if !self.open {
            return;
        }
        let layout = Layout::new(recipes.len(), viewport, self.message);
        push_quad(
            out,
            viewport,
            HudRect {
                x: 0.0,
                y: 0.0,
                width: viewport.0,
                height: viewport.1,
            },
            OVERLAY,
        );
        push_quad(out, viewport, layout.panel, PANEL);
        push_border(out, viewport, layout.panel, PANEL_BORDER, 2.0);

        let title_y = layout.panel.y + 16.0;
        font::push_text(
            out,
            viewport,
            layout.panel.x + PANEL_PADDING,
            title_y,
            2.5,
            TITLE,
            "CRAFTING",
        );
        let close_text = "C TO CLOSE";
        font::push_text(
            out,
            viewport,
            layout.panel.x + layout.panel.width - PANEL_PADDING - font::text_width(close_text, 1.5),
            title_y + 4.0,
            1.5,
            MUTED,
            close_text,
        );

        for (index, row) in layout.rows.iter().enumerate() {
            push_row(
                out,
                viewport,
                row,
                recipes.get(index),
                inventory,
                contains(row, self.cursor),
            );
        }

        if let Some(message) = self.message {
            font::push_text(
                out,
                viewport,
                layout.panel.x + PANEL_PADDING,
                layout.footer_y,
                1.5,
                message.color(),
                &message.text(),
            );
        }
    }
}

/// 面板与配方行的布局；构建与点击检测共用，避免两套坐标漂移。
struct Layout {
    panel: HudRect,
    rows: Vec<HudRect>,
    footer_y: f32,
}

impl Layout {
    fn new(recipe_count: usize, viewport: (f32, f32), message: Option<CraftMessage>) -> Self {
        let (width, height) = viewport;
        let panel_width = PANEL_MAX_WIDTH.min((width - PANEL_MARGIN * 2.0).max(240.0));
        let footer = if message.is_some() {
            FOOTER_HEIGHT
        } else {
            8.0
        };
        let content_height = HEADER_HEIGHT
            + recipe_count as f32 * ROW_HEIGHT
            + recipe_count.saturating_sub(1) as f32 * ROW_GAP
            + footer
            + PANEL_PADDING;
        let panel_height = content_height.min((height - PANEL_MARGIN * 2.0).max(240.0));
        let panel = HudRect {
            x: (width - panel_width) * 0.5,
            y: (height - panel_height) * 0.5,
            width: panel_width,
            height: panel_height,
        };

        let row_width = panel.width - PANEL_PADDING * 2.0;
        let mut rows = Vec::with_capacity(recipe_count);
        for index in 0..recipe_count {
            rows.push(HudRect {
                x: panel.x + PANEL_PADDING,
                y: panel.y + HEADER_HEIGHT + index as f32 * (ROW_HEIGHT + ROW_GAP),
                width: row_width,
                height: ROW_HEIGHT,
            });
        }

        Self {
            panel,
            rows,
            footer_y: panel.y + panel.height - PANEL_PADDING - 12.0,
        }
    }
}

fn push_row(
    out: &mut Vec<HudVertex>,
    viewport: (f32, f32),
    row: &HudRect,
    recipe: Option<&Recipe>,
    inventory: &Inventory,
    hovered: bool,
) {
    push_quad(out, viewport, *row, if hovered { ROW_HOVER } else { ROW });
    push_border(out, viewport, *row, ROW_BORDER, 1.0);

    let Some(recipe) = recipe else {
        return;
    };
    let icon = HudRect {
        x: row.x + 8.0,
        y: row.y + (row.height - ICON_SIZE) * 0.5,
        width: ICON_SIZE,
        height: ICON_SIZE,
    };
    push_quad(out, viewport, icon, {
        let color = icon_color(recipe.output.item());
        [color.x, color.y, color.z, 0.95]
    });
    push_border(out, viewport, icon, PANEL_BORDER, 1.0);

    let text_x = icon.x + ICON_SIZE + 10.0;
    let name = format!(
        "{} X{}",
        item_name(recipe.output.item()).to_uppercase(),
        recipe.output_count()
    );
    font::push_text(out, viewport, text_x, row.y + 6.0, 1.5, TITLE, &name);

    // 分类标签靠右显示，为后续类别筛选保留可视化入口。
    let category = recipe.category.label();
    let category_x = row.x + row.width - 8.0 - font::text_width(category, 1.2);
    font::push_text(out, viewport, category_x, row.y + 8.0, 1.2, MUTED, category);

    let mut cursor_x = text_x;
    let ingredient_y = row.y + row.height - 15.0;
    for ingredient in recipe.ingredients {
        let owned = inventory.count_of(ingredient.item);
        let enough = owned >= ingredient.count;
        let text = format!(
            "{} {}/{}",
            item_name(ingredient.item).to_uppercase(),
            owned,
            ingredient.count
        );
        font::push_text(
            out,
            viewport,
            cursor_x,
            ingredient_y,
            1.2,
            if enough {
                INGREDIENT_OK
            } else {
                INGREDIENT_MISSING
            },
            &text,
        );
        cursor_x += font::text_width(&text, 1.2) + 12.0;
    }
}

fn push_border(
    out: &mut Vec<HudVertex>,
    viewport: (f32, f32),
    rect: HudRect,
    color: [f32; 4],
    thickness: f32,
) {
    push_quad(
        out,
        viewport,
        HudRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: thickness,
        },
        color,
    );
    push_quad(
        out,
        viewport,
        HudRect {
            x: rect.x,
            y: rect.y + rect.height - thickness,
            width: rect.width,
            height: thickness,
        },
        color,
    );
    push_quad(
        out,
        viewport,
        HudRect {
            x: rect.x,
            y: rect.y + thickness,
            width: thickness,
            height: rect.height - thickness * 2.0,
        },
        color,
    );
    push_quad(
        out,
        viewport,
        HudRect {
            x: rect.x + rect.width - thickness,
            y: rect.y + thickness,
            width: thickness,
            height: rect.height - thickness * 2.0,
        },
        color,
    );
}

fn contains(rect: &HudRect, point: (f32, f32)) -> bool {
    point.0 >= rect.x
        && point.0 <= rect.x + rect.width
        && point.1 >= rect.y
        && point.1 <= rect.y + rect.height
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::recipe_registry;
    use crate::item::{ItemId, ItemStack};

    fn viewport() -> (f32, f32) {
        (1280.0, 720.0)
    }

    fn stocked_inventory() -> Inventory {
        let mut inventory = Inventory::new();
        for (item, count) in [
            (ItemId::WOOD, 20),
            (ItemId::STICK, 10),
            (ItemId::STONE, 20),
            (ItemId::IRON_INGOT, 10),
        ] {
            assert!(
                inventory
                    .add_stack(ItemStack::new(item, count).expect("合法堆叠"))
                    .is_none(),
                "背包有空位"
            );
        }
        inventory
    }

    fn first_row_center() -> (f32, f32) {
        let layout = Layout::new(recipe_registry::all().len(), viewport(), None);
        let row = layout.rows[0];
        (row.x + row.width * 0.5, row.y + row.height * 0.5)
    }

    #[test]
    fn ui_opens_and_closes_cleanly() {
        let mut ui = CraftingUi::new();
        assert!(!ui.is_open());
        ui.open();
        assert!(ui.is_open());
        ui.show_failure(CraftError::NoSpace);
        ui.close();
        assert!(!ui.is_open());
    }

    #[test]
    fn clicking_a_recipe_returns_its_id_once_per_click() {
        let ui = {
            let mut ui = CraftingUi::new();
            ui.open();
            ui
        };
        let recipes = recipe_registry::all();
        let cursor = first_row_center();
        let clicked = ui.click_at(recipes, viewport(), cursor);
        assert_eq!(clicked, Some(recipes[0].id));
        assert_eq!(ui.click_at(recipes, viewport(), (5.0, 5.0)), None);
    }

    #[test]
    fn a_closed_ui_never_reports_clicks() {
        let ui = CraftingUi::new();
        assert_eq!(
            ui.click_at(recipe_registry::all(), viewport(), first_row_center()),
            None
        );
    }

    #[test]
    fn the_panel_renders_inside_the_viewport() {
        let mut ui = CraftingUi::new();
        ui.open();
        let inventory = stocked_inventory();
        let mut vertices = Vec::new();
        ui.build(
            &mut vertices,
            recipe_registry::all(),
            &inventory,
            viewport(),
        );

        assert!(!vertices.is_empty());
        assert_eq!(vertices.len() % 6, 0);
        for vertex in &vertices {
            assert!((-1.0..=1.0).contains(&vertex.position[0]));
            assert!((-1.0..=1.0).contains(&vertex.position[1]));
        }
    }

    #[test]
    fn feedback_adds_visible_text() {
        let mut with_message = CraftingUi::new();
        with_message.open();
        with_message.show_success(ItemStack::new(ItemId::STICK, 2).expect("合法产出"));
        let inventory = stocked_inventory();

        let mut plain = CraftingUi::new();
        plain.open();

        let mut with_vertices = Vec::new();
        let mut plain_vertices = Vec::new();
        with_message.build(
            &mut with_vertices,
            recipe_registry::all(),
            &inventory,
            viewport(),
        );
        plain.build(
            &mut plain_vertices,
            recipe_registry::all(),
            &inventory,
            viewport(),
        );
        assert!(
            with_vertices.len() > plain_vertices.len(),
            "成功反馈应额外绘制文字"
        );
    }
}
