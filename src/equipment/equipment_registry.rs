//! 护甲注册表：集中定义可装备护甲的槽位、防御力、材质与最大耐久。
//!
//! 护甲物品同时登记在物品注册表中；这里只描述装备属性，运行时通过
//! [`super::equipment::Equipment`] 管理实例耐久。

use std::sync::OnceLock;

use crate::item::ItemId;

use super::equipment::EquipmentSlot;

/// 护甲材质；用于伤害/耐久比例与后续音效表现。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArmorMaterial {
    Leather,
    Iron,
    Diamond,
}

impl ArmorMaterial {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Leather => "LEATHER",
            Self::Iron => "IRON",
            Self::Diamond => "DIAMOND",
        }
    }
}

/// 一件护甲的静态属性。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArmorDefinition {
    pub id: ItemId,
    pub name: &'static str,
    pub slot: EquipmentSlot,
    pub defense: u32,
    pub max_durability: u32,
    pub material: ArmorMaterial,
}

impl ArmorDefinition {
    const fn new(
        id: ItemId,
        name: &'static str,
        slot: EquipmentSlot,
        defense: u32,
        max_durability: u32,
        material: ArmorMaterial,
    ) -> Self {
        Self {
            id,
            name,
            slot,
            defense,
            max_durability,
            material,
        }
    }

    /// 展示用描述，例如 `IRON CHEST 6DEF 240/240`。
    pub fn summary(self, current: u32) -> String {
        format!(
            "{} {} {}DEF {}/{}",
            self.material.name(),
            match self.slot {
                EquipmentSlot::Head => "HELMET",
                EquipmentSlot::Chest => "CHEST",
                EquipmentSlot::Legs => "LEGGINGS",
                EquipmentSlot::Feet => "BOOTS",
            },
            self.defense,
            current,
            self.max_durability
        )
    }
}

const ARMOR: [ArmorDefinition; 12] = [
    ArmorDefinition::new(
        ItemId::LEATHER_HELMET,
        "Leather Helmet",
        EquipmentSlot::Head,
        1,
        55,
        ArmorMaterial::Leather,
    ),
    ArmorDefinition::new(
        ItemId::LEATHER_CHESTPLATE,
        "Leather Chestplate",
        EquipmentSlot::Chest,
        3,
        80,
        ArmorMaterial::Leather,
    ),
    ArmorDefinition::new(
        ItemId::LEATHER_LEGGINGS,
        "Leather Leggings",
        EquipmentSlot::Legs,
        2,
        75,
        ArmorMaterial::Leather,
    ),
    ArmorDefinition::new(
        ItemId::LEATHER_BOOTS,
        "Leather Boots",
        EquipmentSlot::Feet,
        1,
        65,
        ArmorMaterial::Leather,
    ),
    ArmorDefinition::new(
        ItemId::IRON_HELMET,
        "Iron Helmet",
        EquipmentSlot::Head,
        2,
        165,
        ArmorMaterial::Iron,
    ),
    ArmorDefinition::new(
        ItemId::IRON_CHESTPLATE,
        "Iron Chestplate",
        EquipmentSlot::Chest,
        6,
        240,
        ArmorMaterial::Iron,
    ),
    ArmorDefinition::new(
        ItemId::IRON_LEGGINGS,
        "Iron Leggings",
        EquipmentSlot::Legs,
        5,
        225,
        ArmorMaterial::Iron,
    ),
    ArmorDefinition::new(
        ItemId::IRON_BOOTS,
        "Iron Boots",
        EquipmentSlot::Feet,
        2,
        195,
        ArmorMaterial::Iron,
    ),
    ArmorDefinition::new(
        ItemId::DIAMOND_HELMET,
        "Diamond Helmet",
        EquipmentSlot::Head,
        3,
        363,
        ArmorMaterial::Diamond,
    ),
    ArmorDefinition::new(
        ItemId::DIAMOND_CHESTPLATE,
        "Diamond Chestplate",
        EquipmentSlot::Chest,
        8,
        528,
        ArmorMaterial::Diamond,
    ),
    ArmorDefinition::new(
        ItemId::DIAMOND_LEGGINGS,
        "Diamond Leggings",
        EquipmentSlot::Legs,
        6,
        495,
        ArmorMaterial::Diamond,
    ),
    ArmorDefinition::new(
        ItemId::DIAMOND_BOOTS,
        "Diamond Boots",
        EquipmentSlot::Feet,
        3,
        429,
        ArmorMaterial::Diamond,
    ),
];

static DEFINITIONS: OnceLock<Vec<ArmorDefinition>> = OnceLock::new();

/// 全部护甲定义。
pub fn definitions() -> &'static [ArmorDefinition] {
    DEFINITIONS.get_or_init(|| {
        for (index, definition) in ARMOR.iter().enumerate() {
            assert!(
                definition.id.is_valid(),
                "护甲 {:?} 缺少物品定义",
                definition.id
            );
            assert!(
                ARMOR[..index]
                    .iter()
                    .all(|existing| existing.id != definition.id),
                "护甲物品 {:?} 重复注册",
                definition.id
            );
            assert!(definition.max_durability > 0, "护甲最大耐久必须大于 0");
            assert!(definition.defense > 0, "护甲防御力必须大于 0");
        }
        ARMOR.to_vec()
    })
}

/// 查询某物品的护甲定义；非护甲返回 `None`。
pub fn definition(item: ItemId) -> Option<&'static ArmorDefinition> {
    definitions()
        .iter()
        .find(|definition| definition.id == item)
}

/// 护甲最大耐久；非护甲返回 `None`。
pub fn max_durability(item: ItemId) -> Option<u32> {
    definition(item).map(|definition| definition.max_durability)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_armor_item_has_a_matching_definition() {
        for definition in definitions() {
            assert!(definition.id.is_valid());
            assert!(definition.max_durability > 0);
            assert!(definition.defense > 0);
        }
    }

    #[test]
    fn full_iron_set_reaches_the_expected_defense() {
        let total: u32 = definitions()
            .iter()
            .filter(|definition| definition.material == ArmorMaterial::Iron)
            .map(|definition| definition.defense)
            .sum();
        assert_eq!(total, 15);
    }

    #[test]
    fn non_armor_items_are_rejected() {
        assert!(definition(ItemId::STONE).is_none());
        assert_eq!(max_durability(ItemId::STONE), None);
    }
}
