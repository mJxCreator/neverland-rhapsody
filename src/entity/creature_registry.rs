//! 生物静态数据：猪与牛的定义常量，以及可自然生成生物的选择。
//!
//! 只放数据（尺寸、生命、掉落、颜色、生成权重），不放行为逻辑。

use glam::Vec3;

use super::entity::EntityTypeId;
use super::entity_registry::{CreatureModel, EntityDefinition, HostileCombat, ItemDrop};
use crate::item::ItemId;

/// 猪的掉落：生猪肉 1–2 个。
pub const PIG_DROPS: [ItemDrop; 1] = [ItemDrop {
    item: ItemId::RAW_PORKCHOP,
    min: 1,
    max: 2,
}];

/// 牛的掉落：生牛肉 1–2 个、皮革 0–1 个。
pub const COW_DROPS: [ItemDrop; 2] = [
    ItemDrop {
        item: ItemId::RAW_BEEF,
        min: 1,
        max: 2,
    },
    ItemDrop {
        item: ItemId::LEATHER,
        min: 0,
        max: 1,
    },
];

/// 猪：矮胖体型、10 点生命、移动较快、生成权重更高。
pub const PIG: EntityDefinition = EntityDefinition {
    kind: EntityTypeId::PIG,
    name: "Pig",
    width: 0.9,
    height: 0.9,
    max_health: 10,
    move_speed: 1.6,
    drops: &PIG_DROPS,
    spawn_weight: 3,
    body_color: Vec3::new(0.95, 0.62, 0.64),
    accent_color: Vec3::new(0.83, 0.42, 0.47),
    model: CreatureModel::Pig,
    combat: None,
};

/// 牛：高大体型、14 点生命、移动稍慢、生成权重较低。
pub const COW: EntityDefinition = EntityDefinition {
    kind: EntityTypeId::COW,
    name: "Cow",
    width: 0.9,
    height: 1.4,
    max_health: 14,
    move_speed: 1.2,
    drops: &COW_DROPS,
    spawn_weight: 2,
    body_color: Vec3::new(0.93, 0.92, 0.88),
    accent_color: Vec3::new(0.24, 0.19, 0.16),
    model: CreatureModel::Cow,
    combat: None,
};

/// 僵尸的掉落：腐肉 0–1 个。
pub const ZOMBIE_DROPS: [ItemDrop; 1] = [ItemDrop {
    item: ItemId::ROTTEN_FLESH,
    min: 0,
    max: 1,
}];

/// 僵尸：人形敌对生物，近战攻击，白天也会生成。
pub const ZOMBIE: EntityDefinition = EntityDefinition {
    kind: EntityTypeId::ZOMBIE,
    name: "Zombie",
    width: 0.6,
    height: 1.8,
    max_health: 20,
    move_speed: 1.4,
    drops: &ZOMBIE_DROPS,
    spawn_weight: 2,
    body_color: Vec3::new(0.22, 0.48, 0.26),
    accent_color: Vec3::new(0.36, 0.62, 0.38),
    model: CreatureModel::Zombie,
    combat: Some(HostileCombat {
        attack_damage: 3,
        attack_range: 1.8,
        attack_cooldown: 1.0,
        detection_range: 12.0,
        lose_range: 20.0,
    }),
};

/// 参与自然生成的生物定义列表。
pub fn spawnable() -> impl Iterator<Item = &'static EntityDefinition> {
    super::entity_registry::all()
        .iter()
        .filter(|definition| definition.spawn_weight > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::entity_registry;

    #[test]
    fn pig_and_cow_are_registered() {
        assert_eq!(entity_registry::definition(EntityTypeId::PIG), Some(&PIG));
        assert_eq!(entity_registry::definition(EntityTypeId::COW), Some(&COW));
    }

    #[test]
    fn pig_and_cow_differ_in_shape() {
        assert_ne!(PIG.model, COW.model);
        let pig = entity_registry::definition(EntityTypeId::PIG).expect("猪应已注册");
        let cow = entity_registry::definition(EntityTypeId::COW).expect("牛应已注册");
        assert!(pig.height < cow.height);
        assert_ne!(PIG.spawn_weight, 0);
        assert_ne!(COW.spawn_weight, 0);
    }

    #[test]
    fn spawnable_contains_both_creatures() {
        let kinds: Vec<_> = spawnable().map(|definition| definition.kind).collect();
        assert!(kinds.contains(&EntityTypeId::PIG));
        assert!(kinds.contains(&EntityTypeId::COW));
    }
}
