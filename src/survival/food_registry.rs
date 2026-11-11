//! 食物注册表：可食用物品的饥饿恢复、饱食度恢复、进食时长与特殊效果。

use crate::item::ItemId;

/// 食物特殊效果；当前只包含腐肉触发的饥饿状态。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FoodEffect {
    None,
    /// 每次进食以给定概率触发持续若干秒的饥饿状态。
    Hunger {
        chance: f32,
        duration: f32,
    },
}

/// 一种食物的静态定义。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FoodDefinition {
    pub item: ItemId,
    pub hunger: f32,
    pub saturation: f32,
    /// 完成进食所需的秒数。
    pub eat_seconds: f32,
    pub effect: FoodEffect,
}

const RAW_PORKCHOP: FoodDefinition = FoodDefinition {
    item: ItemId::RAW_PORKCHOP,
    hunger: 3.0,
    saturation: 1.8,
    eat_seconds: 1.6,
    effect: FoodEffect::None,
};

const RAW_BEEF: FoodDefinition = FoodDefinition {
    item: ItemId::RAW_BEEF,
    hunger: 3.0,
    saturation: 1.8,
    eat_seconds: 1.6,
    effect: FoodEffect::None,
};

const COOKED_PORKCHOP: FoodDefinition = FoodDefinition {
    item: ItemId::COOKED_PORKCHOP,
    hunger: 8.0,
    saturation: 8.0,
    eat_seconds: 1.6,
    effect: FoodEffect::None,
};

const COOKED_BEEF: FoodDefinition = FoodDefinition {
    item: ItemId::COOKED_BEEF,
    hunger: 8.0,
    saturation: 8.0,
    eat_seconds: 1.6,
    effect: FoodEffect::None,
};

const APPLE: FoodDefinition = FoodDefinition {
    item: ItemId::APPLE,
    hunger: 4.0,
    saturation: 2.4,
    eat_seconds: 1.2,
    effect: FoodEffect::None,
};

const BREAD: FoodDefinition = FoodDefinition {
    item: ItemId::BREAD,
    hunger: 5.0,
    saturation: 4.0,
    eat_seconds: 1.4,
    effect: FoodEffect::None,
};

const ROTTEN_FLESH: FoodDefinition = FoodDefinition {
    item: ItemId::ROTTEN_FLESH,
    hunger: 2.0,
    saturation: 0.5,
    eat_seconds: 1.2,
    effect: FoodEffect::Hunger {
        chance: 0.2,
        duration: 8.0,
    },
};

/// 全部可食用物品。
const FOODS: [FoodDefinition; 7] = [
    RAW_PORKCHOP,
    RAW_BEEF,
    COOKED_PORKCHOP,
    COOKED_BEEF,
    APPLE,
    BREAD,
    ROTTEN_FLESH,
];

/// 查询某种物品的食物定义；不可食用时返回 `None`。
pub fn definition(item: ItemId) -> Option<&'static FoodDefinition> {
    FOODS.iter().find(|food| food.item == item)
}

/// 物品是否可食用。
#[allow(dead_code)]
pub fn is_food(item: ItemId) -> bool {
    definition(item).is_some()
}

/// 全部食物定义，供界面展示与测试使用。
#[allow(dead_code)]
pub fn all() -> &'static [FoodDefinition] {
    &FOODS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_foods_have_valid_ids() {
        for food in all() {
            assert!(food.item.is_valid(), "食物 ID 无效: {:?}", food.item);
            assert!(food.hunger > 0.0);
            assert!(food.saturation >= 0.0);
            assert!(food.eat_seconds > 0.0);
        }
    }

    #[test]
    fn cooked_meat_restores_more_than_raw() {
        let raw = definition(ItemId::RAW_PORKCHOP).unwrap();
        let cooked = definition(ItemId::COOKED_PORKCHOP).unwrap();
        assert!(cooked.hunger > raw.hunger);
        let raw_beef = definition(ItemId::RAW_BEEF).unwrap();
        let cooked_beef = definition(ItemId::COOKED_BEEF).unwrap();
        assert!(cooked_beef.hunger > raw_beef.hunger);
    }

    #[test]
    fn rotten_flesh_has_configured_effect() {
        let food = definition(ItemId::ROTTEN_FLESH).unwrap();
        assert_eq!(
            food.effect,
            FoodEffect::Hunger {
                chance: 0.2,
                duration: 8.0
            }
        );
    }

    #[test]
    fn non_food_items_are_rejected() {
        assert!(!is_food(ItemId::STONE));
        assert!(!is_food(ItemId::INVALID));
    }
}
