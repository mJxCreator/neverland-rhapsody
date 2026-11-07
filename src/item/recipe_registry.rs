//! 合成配方注册表：集中定义基础配方，并提供按 ID / 类别查询。
//!
//! 注册表在首次访问时校验全部配方，避免非法配方进入运行时。当前版本没有
//! 存档持久化的解锁状态，`enabled` 字段为后续版本预留。

use std::sync::OnceLock;

use super::inventory::Inventory;
use super::item::ItemId;
use super::recipe::{Ingredient, Recipe, RecipeCategory, RecipeId, RecipeType};
use super::ItemStack;

const WOOD_TWO: [Ingredient; 1] = [Ingredient::new(ItemId::WOOD, 2)];
const WOOD_STICK_TWO: [Ingredient; 2] = [
    Ingredient::new(ItemId::WOOD, 3),
    Ingredient::new(ItemId::STICK, 2),
];
const STONE_STICK_TWO: [Ingredient; 2] = [
    Ingredient::new(ItemId::STONE, 3),
    Ingredient::new(ItemId::STICK, 2),
];
const IRON_STICK_TWO: [Ingredient; 2] = [
    Ingredient::new(ItemId::IRON_INGOT, 3),
    Ingredient::new(ItemId::STICK, 2),
];
const WOOD_STICK_TWO_SHOVEL: [Ingredient; 2] = [
    Ingredient::new(ItemId::WOOD, 1),
    Ingredient::new(ItemId::STICK, 2),
];
const STONE_STICK_TWO_SHOVEL: [Ingredient; 2] = [
    Ingredient::new(ItemId::STONE, 1),
    Ingredient::new(ItemId::STICK, 2),
];
const IRON_STICK_TWO_SHOVEL: [Ingredient; 2] = [
    Ingredient::new(ItemId::IRON_INGOT, 1),
    Ingredient::new(ItemId::STICK, 2),
];

/// 全部配方；顺序决定合成界面的展示顺序。
static RECIPES: OnceLock<Vec<Recipe>> = OnceLock::new();

/// 构造一条随身合成配方；注册阶段保证参数合法。
fn hand_recipe(
    id: RecipeId,
    output: ItemId,
    output_count: u32,
    ingredients: &'static [Ingredient],
    category: RecipeCategory,
) -> Recipe {
    Recipe {
        id,
        output: ItemStack::new(output, output_count).expect("配方产出必须合法"),
        ingredients,
        category,
        recipe_type: RecipeType::HandCrafting,
        enabled: true,
    }
}

fn build_recipes() -> Vec<Recipe> {
    let tools = RecipeCategory::Tools;
    let materials = RecipeCategory::Materials;
    let recipes = vec![
        hand_recipe(RecipeId::STICK, ItemId::STICK, 2, &WOOD_TWO, materials),
        hand_recipe(
            RecipeId::WOODEN_PICKAXE,
            ItemId::WOODEN_PICKAXE,
            1,
            &WOOD_STICK_TWO,
            tools,
        ),
        hand_recipe(
            RecipeId::STONE_PICKAXE,
            ItemId::STONE_PICKAXE,
            1,
            &STONE_STICK_TWO,
            tools,
        ),
        hand_recipe(
            RecipeId::IRON_PICKAXE,
            ItemId::IRON_PICKAXE,
            1,
            &IRON_STICK_TWO,
            tools,
        ),
        hand_recipe(
            RecipeId::WOODEN_AXE,
            ItemId::WOODEN_AXE,
            1,
            &WOOD_STICK_TWO,
            tools,
        ),
        hand_recipe(
            RecipeId::STONE_AXE,
            ItemId::STONE_AXE,
            1,
            &STONE_STICK_TWO,
            tools,
        ),
        hand_recipe(
            RecipeId::IRON_AXE,
            ItemId::IRON_AXE,
            1,
            &IRON_STICK_TWO,
            tools,
        ),
        hand_recipe(
            RecipeId::WOODEN_SHOVEL,
            ItemId::WOODEN_SHOVEL,
            1,
            &WOOD_STICK_TWO_SHOVEL,
            tools,
        ),
        hand_recipe(
            RecipeId::STONE_SHOVEL,
            ItemId::STONE_SHOVEL,
            1,
            &STONE_STICK_TWO_SHOVEL,
            tools,
        ),
        hand_recipe(
            RecipeId::IRON_SHOVEL,
            ItemId::IRON_SHOVEL,
            1,
            &IRON_STICK_TWO_SHOVEL,
            tools,
        ),
    ];

    // 防御性校验：重复 ID、非法材料或非法产出都不允许进入注册表。
    for (index, recipe) in recipes.iter().enumerate() {
        assert!(recipe.is_valid(), "配方 {:?} 结构非法", recipe.id);
        assert!(
            recipes[..index]
                .iter()
                .all(|existing| existing.id != recipe.id),
            "配方 ID {:?} 重复",
            recipe.id
        );
        assert!(
            recipe
                .ingredients
                .iter()
                .all(|ingredient| ingredient.item.is_valid()),
            "配方 {:?} 含无效材料",
            recipe.id
        );
        assert!(
            recipe.output.item().is_valid(),
            "配方 {:?} 的产出无效",
            recipe.id
        );
    }

    recipes
}

/// 全部已注册配方。
pub fn all() -> &'static [Recipe] {
    RECIPES.get_or_init(build_recipes)
}

/// 按稳定 ID 查询配方；未知 ID 返回 `None`，调用方必须安全处理。
pub fn by_id(id: RecipeId) -> Option<&'static Recipe> {
    all().iter().find(|recipe| recipe.id == id)
}

/// 按类别查询已启用配方。
pub fn by_category(category: RecipeCategory) -> impl Iterator<Item = &'static Recipe> {
    all()
        .iter()
        .filter(move |recipe| recipe.enabled && recipe.category == category)
}

/// 当前版本所有已启用配方。
pub fn enabled() -> impl Iterator<Item = &'static Recipe> {
    all().iter().filter(|recipe| recipe.enabled)
}

/// 背包是否满足某条配方的全部材料需求。
pub fn has_materials(recipe: &Recipe, inventory: &Inventory) -> bool {
    recipe
        .ingredients
        .iter()
        .all(|ingredient| inventory.count_of(ingredient.item) >= ingredient.count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_defines_the_basic_tool_recipes() {
        assert!(by_id(RecipeId::STICK).is_some());
        assert!(by_id(RecipeId::WOODEN_PICKAXE).is_some());
        assert!(by_id(RecipeId::STONE_PICKAXE).is_some());
        assert!(by_id(RecipeId::IRON_PICKAXE).is_some());
        assert!(by_id(RecipeId::WOODEN_AXE).is_some());
        assert!(by_id(RecipeId::STONE_SHOVEL).is_some());
    }

    #[test]
    fn unknown_ids_are_safe() {
        assert!(by_id(RecipeId::INVALID).is_none());
        assert!(by_id(RecipeId::IRON_SHOVEL).is_some());
    }

    #[test]
    fn category_query_returns_only_matching_recipes() {
        let tools: Vec<_> = by_category(RecipeCategory::Tools).collect();
        assert!(!tools.is_empty());
        assert!(tools
            .iter()
            .all(|recipe| recipe.category == RecipeCategory::Tools));

        let materials: Vec<_> = by_category(RecipeCategory::Materials).collect();
        assert_eq!(materials.len(), 1);
        assert_eq!(materials[0].id, RecipeId::STICK);
    }

    #[test]
    fn material_checks_follow_the_recipe_counts() {
        let mut inventory = Inventory::new();
        assert!(!has_materials(
            by_id(RecipeId::WOODEN_PICKAXE).expect("配方存在"),
            &inventory
        ));
        assert!(inventory
            .add_stack(ItemStack::new(ItemId::WOOD, 3).expect("合法堆叠"))
            .is_none());
        assert!(!has_materials(
            by_id(RecipeId::WOODEN_PICKAXE).expect("配方存在"),
            &inventory
        ));
        assert!(inventory
            .add_stack(ItemStack::new(ItemId::STICK, 2).expect("合法堆叠"))
            .is_none());
        assert!(has_materials(
            by_id(RecipeId::WOODEN_PICKAXE).expect("配方存在"),
            &inventory
        ));
    }
}
