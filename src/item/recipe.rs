//! 合成配方数据：稳定配方 ID、材料需求、产出与分类。
//!
//! 配方只描述“用什么换什么”，不直接修改背包；实际执行与原子性保证由
//! [`super::crafting`] 负责。配方定义在 [`super::recipe_registry`] 中集中注册。

use super::item::ItemId;
use super::ItemStack;

/// 稳定的配方 ID；`INVALID` 表示不可用的配方 ID。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RecipeId(u16);

impl RecipeId {
    pub const INVALID: Self = Self(0);
    pub const STICK: Self = Self(1);
    pub const WOODEN_PICKAXE: Self = Self(2);
    pub const STONE_PICKAXE: Self = Self(3);
    pub const IRON_PICKAXE: Self = Self(4);
    pub const WOODEN_AXE: Self = Self(5);
    pub const STONE_AXE: Self = Self(6);
    pub const IRON_AXE: Self = Self(7);
    pub const WOODEN_SHOVEL: Self = Self(8);
    pub const STONE_SHOVEL: Self = Self(9);
    pub const IRON_SHOVEL: Self = Self(10);
    pub const WOODEN_SWORD: Self = Self(11);
    pub const STONE_SWORD: Self = Self(12);
    pub const IRON_SWORD: Self = Self(13);
    pub const DIAMOND_SWORD: Self = Self(14);
    pub const LEATHER_HELMET: Self = Self(15);
    pub const LEATHER_CHESTPLATE: Self = Self(16);
    pub const LEATHER_LEGGINGS: Self = Self(17);
    pub const LEATHER_BOOTS: Self = Self(18);
    pub const IRON_HELMET: Self = Self(19);
    pub const IRON_CHESTPLATE: Self = Self(20);
    pub const IRON_LEGGINGS: Self = Self(21);
    pub const IRON_BOOTS: Self = Self(22);
    pub const DIAMOND_HELMET: Self = Self(23);
    pub const DIAMOND_CHESTPLATE: Self = Self(24);
    pub const DIAMOND_LEGGINGS: Self = Self(25);
    pub const DIAMOND_BOOTS: Self = Self(26);
    pub const CAMPFIRE: Self = Self(27);
    pub const BED: Self = Self(28);
}

/// 配方分类：用于合成界面的分类查询与后续解锁规则。
///
/// 当前版本只使用了材料与工具两类，其余分类为后续版本预留。
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RecipeCategory {
    Materials,
    Tools,
    Utility,
    Weapons,
    Armor,
    Consumables,
    Building,
}

impl RecipeCategory {
    /// 展示用英文标签；字体仅覆盖 ASCII。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Materials => "MATERIALS",
            Self::Tools => "TOOLS",
            Self::Utility => "UTILITY",
            Self::Weapons => "WEAPONS",
            Self::Armor => "ARMOR",
            Self::Consumables => "CONSUMABLES",
            Self::Building => "BUILDING",
        }
    }
}

/// 合成类型；后续工作台等条件化配方在此扩展。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecipeType {
    /// 无需工作台，随身合成。
    HandCrafting,
    /// 需要靠近工作台；当前版本尚未提供工作台，预留给后续配方。
    #[allow(dead_code)]
    CraftingTable,
}

/// 一种材料需求：物品与所需数量（数量必须大于 0）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ingredient {
    pub item: ItemId,
    pub count: u32,
}

impl Ingredient {
    pub const fn new(item: ItemId, count: u32) -> Self {
        Self { item, count }
    }
}

/// 一条合成配方。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Recipe {
    pub id: RecipeId,
    /// 配方产出；工具在构造时即带有满耐久。
    pub output: ItemStack,
    /// 材料需求；不得为空，且每项物品必须有效。
    pub ingredients: &'static [Ingredient],
    pub category: RecipeCategory,
    pub recipe_type: RecipeType,
    /// 是否已解锁并可参与合成。
    pub enabled: bool,
}

impl Recipe {
    /// 产出数量。
    pub fn output_count(self) -> u32 {
        self.output.count()
    }

    /// 配方结构是否合法：ID 非空、材料与产出均有效。
    pub fn is_valid(self) -> bool {
        if self.id == RecipeId::INVALID
            || self.output_count() == 0
            || self.output.item() == ItemId::INVALID
            || self.ingredients.is_empty()
        {
            return false;
        }
        self.ingredients.iter().all(|ingredient| {
            ingredient.count > 0
                && ingredient.item != ItemId::INVALID
                && ingredient.item.is_valid()
                && self.output.item().is_valid()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::recipe_registry;

    #[test]
    fn invalid_recipe_ids_resolve_to_none() {
        assert!(recipe_registry::by_id(RecipeId::INVALID).is_none());
        assert!(recipe_registry::by_id(RecipeId::WOODEN_SHOVEL).is_some());
    }

    #[test]
    fn every_registered_recipe_is_structurally_valid() {
        for recipe in recipe_registry::all() {
            assert!(recipe.is_valid(), "配方 {:?} 结构非法", recipe.id);
            assert!(recipe.enabled, "当前版本所有配方都应可用");
        }
    }
}
