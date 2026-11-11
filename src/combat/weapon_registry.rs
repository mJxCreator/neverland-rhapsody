//! 武器注册表：剑类物品的伤害、攻击间隔与耐久。
//!
//! 武器属性集中在此登记，物品注册表只负责物品名称与堆叠规则；
//! 两者通过物品 ID 关联，避免在战斗逻辑中硬编码数值。

use std::sync::OnceLock;

use crate::item::ItemId;

use super::weapon::WeaponDefinition;

const WEAPONS: [WeaponDefinition; 4] = [
    WeaponDefinition::new(ItemId::WOODEN_SWORD, "Wooden Sword", 4, 0.8, 60),
    WeaponDefinition::new(ItemId::STONE_SWORD, "Stone Sword", 5, 0.8, 132),
    WeaponDefinition::new(ItemId::IRON_SWORD, "Iron Sword", 6, 0.8, 251),
    WeaponDefinition::new(ItemId::DIAMOND_SWORD, "Diamond Sword", 7, 0.7, 1024),
];

static DEFINITIONS: OnceLock<Vec<WeaponDefinition>> = OnceLock::new();

/// 全部武器定义。
pub fn definitions() -> &'static [WeaponDefinition] {
    DEFINITIONS.get_or_init(|| {
        for (index, definition) in WEAPONS.iter().enumerate() {
            assert!(
                definition.id.is_valid(),
                "武器 {:?} 缺少物品定义",
                definition.id
            );
            assert!(
                WEAPONS[..index]
                    .iter()
                    .all(|existing| existing.id != definition.id),
                "武器物品 {:?} 重复注册",
                definition.id
            );
            assert!(definition.damage > 0, "武器伤害必须大于 0");
            assert!(definition.attack_interval > 0.0);
            assert!(definition.range > 0.0);
            assert!((1.0..=360.0).contains(&definition.angle_deg));
            assert!(definition.max_durability > 0);
        }
        WEAPONS.to_vec()
    })
}

/// 按物品查询武器定义；非武器返回 `None`。
pub fn definition(item: ItemId) -> Option<&'static WeaponDefinition> {
    definitions()
        .iter()
        .find(|definition| definition.id == item)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_swords_are_registered() {
        assert_eq!(definitions().len(), 4);
        for item in [
            ItemId::WOODEN_SWORD,
            ItemId::STONE_SWORD,
            ItemId::IRON_SWORD,
            ItemId::DIAMOND_SWORD,
        ] {
            assert!(definition(item).is_some(), "{item:?} 应有武器定义");
        }
    }

    #[test]
    fn weapon_damage_grows_with_material() {
        let damage = |item| definition(item).expect("武器存在").damage;
        assert!(damage(ItemId::WOODEN_SWORD) < damage(ItemId::STONE_SWORD));
        assert!(damage(ItemId::STONE_SWORD) < damage(ItemId::IRON_SWORD));
        assert!(damage(ItemId::IRON_SWORD) < damage(ItemId::DIAMOND_SWORD));
    }

    #[test]
    fn tools_are_not_weapons() {
        assert!(definition(ItemId::IRON_PICKAXE).is_none());
        assert!(definition(ItemId::STONE).is_none());
    }
}
