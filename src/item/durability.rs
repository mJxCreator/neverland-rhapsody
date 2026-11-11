//! 物品耐久统一查询：工具、武器与护甲共享同一套实例耐久字段。

use super::item::ItemId;

/// 某物品的最大耐久；普通物品返回 `None`。
pub fn max_durability(item: ItemId) -> Option<u32> {
    if let Some(definition) = super::tool_registry::definition(item) {
        return Some(definition.max_durability);
    }
    if let Some(definition) = crate::combat::weapon_registry::definition(item) {
        return Some(definition.max_durability);
    }
    crate::equipment::equipment_registry::max_durability(item)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_weapons_and_armor_expose_durability() {
        assert_eq!(max_durability(ItemId::WOODEN_PICKAXE), Some(60));
        assert_eq!(max_durability(ItemId::IRON_SWORD), Some(251));
        assert_eq!(max_durability(ItemId::DIAMOND_HELMET), Some(363));
    }

    #[test]
    fn ordinary_items_have_no_durability() {
        assert_eq!(max_durability(ItemId::STONE), None);
        assert_eq!(max_durability(ItemId::INVALID), None);
    }
}
