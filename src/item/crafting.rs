//! 合成执行：材料检查、产出空间检查与原子提交。
//!
//! 合成必须按“先完整检查、后一次性提交”的顺序执行。任何一步失败都不允许
//! 扣除材料或产生产出，避免重复点击或异常输入造成物品复制/丢失。

use super::inventory::Inventory;
use super::recipe::{Recipe, RecipeId};
use super::recipe_registry;
use super::ItemStack;

/// 合成失败原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CraftError {
    /// 配方 ID 不存在或配方已禁用。
    UnknownRecipe,
    /// 材料数量不足。
    InsufficientMaterials,
    /// 背包没有足够空间容纳产出。
    NoSpace,
}

impl CraftError {
    /// 面向玩家的简短说明；字体仅覆盖 ASCII。
    pub const fn message(self) -> &'static str {
        match self {
            Self::UnknownRecipe => "RECIPE UNAVAILABLE",
            Self::InsufficientMaterials => "NOT ENOUGH MATERIALS",
            Self::NoSpace => "INVENTORY FULL",
        }
    }
}

/// 检查配方能否在当前背包中完成，不产生任何修改。
pub fn check(recipe: &Recipe, inventory: &Inventory) -> Result<(), CraftError> {
    if !recipe.enabled || !recipe.is_valid() {
        return Err(CraftError::UnknownRecipe);
    }
    if !recipe_registry::has_materials(recipe, inventory) {
        return Err(CraftError::InsufficientMaterials);
    }

    // 用背包副本预演产出，确认至少有一种合法放置方式。
    let mut preview = *inventory;
    if preview.add_stack(recipe.output).is_some() {
        return Err(CraftError::NoSpace);
    }
    Ok(())
}

/// 按配方执行一次合成；成功返回产出堆叠，失败时背包保持不变。
///
/// 工具产出由 [`ItemStack::new`] 初始化满耐久；同种材料优先从已有堆叠中扣除。
pub fn craft(recipe: &Recipe, inventory: &mut Inventory) -> Result<ItemStack, CraftError> {
    check(recipe, inventory)?;

    let original = *inventory;
    for ingredient in recipe.ingredients {
        let removed = inventory.remove_item(ingredient.item, ingredient.count);
        if removed != ingredient.count {
            // 检查阶段已经确认数量充足，这里只做异常回滚保护。
            *inventory = original;
            return Err(CraftError::InsufficientMaterials);
        }
    }

    if let Some(remaining) = inventory.add_stack(recipe.output) {
        // 预检通过后仍放不下说明状态异常；回滚到合成前，避免材料丢失。
        *inventory = original;
        tracing::warn!(
            recipe = ?recipe.id,
            remaining = remaining.count(),
            "合成产出无法放入背包，已回滚"
        );
        return Err(CraftError::NoSpace);
    }

    Ok(recipe.output)
}

/// 按 ID 查询并执行合成；未知 ID 安全失败。
pub fn craft_by_id(id: RecipeId, inventory: &mut Inventory) -> Result<ItemStack, CraftError> {
    let recipe = recipe_registry::by_id(id).ok_or(CraftError::UnknownRecipe)?;
    craft(recipe, inventory)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::recipe::{Ingredient, Recipe, RecipeCategory, RecipeType};
    use crate::item::{ItemId, ItemStack};

    // 配方持有静态材料表，因此测试用材料也必须是常量。
    const PICKAXE_INGREDIENTS: &[Ingredient] = &[
        Ingredient::new(ItemId::WOOD, 3),
        Ingredient::new(ItemId::STICK, 2),
    ];
    const STICK_INGREDIENTS: &[Ingredient] = &[Ingredient::new(ItemId::WOOD, 2)];

    fn recipe(id: RecipeId, output: ItemStack, ingredients: &'static [Ingredient]) -> Recipe {
        Recipe {
            id,
            output,
            ingredients,
            category: RecipeCategory::Tools,
            recipe_type: RecipeType::HandCrafting,
            enabled: true,
        }
    }

    fn inventory_with(items: &[(ItemId, u32)]) -> Inventory {
        let mut inventory = Inventory::new();
        for (item, count) in items {
            assert!(
                inventory
                    .add_stack(ItemStack::new(*item, *count).expect("合法堆叠"))
                    .is_none(),
                "背包应有多余空间"
            );
        }
        inventory
    }

    #[test]
    fn missing_materials_do_not_change_the_inventory() {
        let recipe = recipe(
            RecipeId::WOODEN_PICKAXE,
            ItemStack::new(ItemId::WOODEN_PICKAXE, 1).expect("合法产出"),
            PICKAXE_INGREDIENTS,
        );
        let mut inventory = inventory_with(&[(ItemId::WOOD, 3)]);
        assert_eq!(
            craft(&recipe, &mut inventory),
            Err(CraftError::InsufficientMaterials)
        );
        assert_eq!(inventory.count_of(ItemId::WOOD), 3);
        assert_eq!(inventory.count_of(ItemId::WOODEN_PICKAXE), 0);
    }

    #[test]
    fn a_successful_craft_consumes_materials_and_adds_the_output() {
        let recipe = recipe(
            RecipeId::WOODEN_PICKAXE,
            ItemStack::new(ItemId::WOODEN_PICKAXE, 1).expect("合法产出"),
            PICKAXE_INGREDIENTS,
        );
        let mut inventory = inventory_with(&[(ItemId::WOOD, 5), (ItemId::STICK, 4)]);
        let output = craft(&recipe, &mut inventory).expect("合成应成功");
        assert_eq!(output.item(), ItemId::WOODEN_PICKAXE);
        assert_eq!(output.durability(), output.max_durability());
        assert_eq!(inventory.count_of(ItemId::WOOD), 2);
        assert_eq!(inventory.count_of(ItemId::STICK), 2);
        assert_eq!(inventory.count_of(ItemId::WOODEN_PICKAXE), 1);
    }

    #[test]
    fn a_full_inventory_rejects_the_craft_without_consuming_materials() {
        let recipe = recipe(
            RecipeId::STICK,
            ItemStack::new(ItemId::STICK, 2).expect("合法产出"),
            STICK_INGREDIENTS,
        );
        let mut inventory = inventory_with(&[(ItemId::WOOD, 2)]);

        // 用不可堆叠的工具填满剩余槽位，使产出无处安放。
        let filler = ItemStack::new(ItemId::WOODEN_PICKAXE, 1).expect("合法工具");
        for index in 0..crate::item::TOTAL_SLOTS {
            if inventory.slot(index).is_none() {
                inventory.set_slot(index, Some(filler));
            }
        }

        assert_eq!(craft(&recipe, &mut inventory), Err(CraftError::NoSpace));
        assert_eq!(inventory.count_of(ItemId::WOOD), 2, "材料不得被扣除");
        assert_eq!(inventory.count_of(ItemId::STICK), 0);
    }

    #[test]
    fn an_unknown_recipe_id_is_rejected_safely() {
        let mut inventory = inventory_with(&[(ItemId::WOOD, 2)]);
        assert_eq!(
            craft_by_id(RecipeId::INVALID, &mut inventory),
            Err(CraftError::UnknownRecipe)
        );
        assert_eq!(inventory.count_of(ItemId::WOOD), 2);
    }

    #[test]
    fn repeated_crafts_require_repeated_materials() {
        let recipe = recipe(
            RecipeId::STICK,
            ItemStack::new(ItemId::STICK, 2).expect("合法产出"),
            STICK_INGREDIENTS,
        );
        let mut inventory = inventory_with(&[(ItemId::WOOD, 3)]);
        assert!(craft(&recipe, &mut inventory).is_ok());
        assert_eq!(inventory.count_of(ItemId::WOOD), 1);
        assert_eq!(
            craft(&recipe, &mut inventory),
            Err(CraftError::InsufficientMaterials),
            "第二次合成不得重复领取产出"
        );
        assert_eq!(inventory.count_of(ItemId::STICK), 2);
    }
}
