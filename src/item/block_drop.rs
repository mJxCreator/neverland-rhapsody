//! 方块掉落映射：定义每种方块被破坏后生成的物品与数量。
//!
//! 掉落配置集中在方块 ID 维度的表里，避免在挖掘代码中硬编码物品。
//! `weight` 为后续概率/加权掉落预留，当前版本所有条目权重相同、必定掉落。

use super::item::ItemId;
use crate::world::Block;

/// 一条掉落定义。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockDrop {
    /// 掉落物品。
    pub item: ItemId,
    /// 掉落数量。
    pub count: u32,
    /// 相对权重，当前版本未使用随机规则。
    pub weight: u32,
}

const STONE_DROPS: [BlockDrop; 1] = [BlockDrop {
    item: ItemId::STONE,
    count: 1,
    weight: 1,
}];

const DIRT_DROPS: [BlockDrop; 1] = [BlockDrop {
    item: ItemId::DIRT,
    count: 1,
    weight: 1,
}];

const GRASS_DROPS: [BlockDrop; 1] = [BlockDrop {
    item: ItemId::GRASS,
    count: 1,
    weight: 1,
}];

const WOOD_DROPS: [BlockDrop; 1] = [BlockDrop {
    item: ItemId::WOOD,
    count: 1,
    weight: 1,
}];

const CAMPFIRE_DROPS: [BlockDrop; 1] = [BlockDrop {
    item: ItemId::CAMPFIRE,
    count: 1,
    weight: 1,
}];

const BED_DROPS: [BlockDrop; 1] = [BlockDrop {
    item: ItemId::BED,
    count: 1,
    weight: 1,
}];

const SAND_DROPS: [BlockDrop; 1] = [BlockDrop {
    item: ItemId::SAND,
    count: 1,
    weight: 1,
}];

const SANDSTONE_DROPS: [BlockDrop; 1] = [BlockDrop {
    item: ItemId::SANDSTONE,
    count: 1,
    weight: 1,
}];

const CACTUS_DROPS: [BlockDrop; 1] = [BlockDrop {
    item: ItemId::CACTUS,
    count: 1,
    weight: 1,
}];

const COAL_ORE_DROPS: [BlockDrop; 1] = [BlockDrop {
    item: ItemId::COAL,
    count: 1,
    weight: 1,
}];

const IRON_ORE_DROPS: [BlockDrop; 1] = [BlockDrop {
    item: ItemId::IRON_INGOT,
    count: 1,
    weight: 1,
}];

const GOLD_ORE_DROPS: [BlockDrop; 1] = [BlockDrop {
    item: ItemId::GOLD_INGOT,
    count: 1,
    weight: 1,
}];

const DIAMOND_ORE_DROPS: [BlockDrop; 1] = [BlockDrop {
    item: ItemId::DIAMOND,
    count: 1,
    weight: 1,
}];

/// 指定方块被破坏后的掉落列表；不可掉落方块返回空列表。
pub fn drops_for_block(block: Block) -> &'static [BlockDrop] {
    match block {
        Block::Air => &[],
        Block::Stone => &STONE_DROPS,
        Block::Dirt => &DIRT_DROPS,
        Block::Grass => &GRASS_DROPS,
        Block::Wood => &WOOD_DROPS,
        Block::Campfire => &CAMPFIRE_DROPS,
        Block::Bed => &BED_DROPS,
        Block::Sand => &SAND_DROPS,
        Block::Sandstone => &SANDSTONE_DROPS,
        Block::CoalOre => &COAL_ORE_DROPS,
        Block::IronOre => &IRON_ORE_DROPS,
        Block::GoldOre => &GOLD_ORE_DROPS,
        Block::DiamondOre => &DIAMOND_ORE_DROPS,
        Block::Cactus => &CACTUS_DROPS,
        Block::Water
        | Block::Leaves
        | Block::TallGrass
        | Block::FlowerRed
        | Block::FlowerYellow => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::item_registry;

    #[test]
    fn breakable_blocks_drop_their_matching_item() {
        for block in [
            Block::Stone,
            Block::Dirt,
            Block::Grass,
            Block::Wood,
            Block::Campfire,
            Block::Bed,
            Block::Sand,
            Block::Sandstone,
            Block::Cactus,
        ] {
            let drops = drops_for_block(block);
            assert!(!drops.is_empty(), "{block:?} 必须配置掉落");
            for drop in drops {
                assert!(drop.item.is_valid());
                assert!(drop.count > 0);
                assert_eq!(
                    item_registry::block_for_item(drop.item),
                    Some(block),
                    "掉落物品应与方块对应"
                );
            }
        }
    }

    #[test]
    fn air_drops_nothing() {
        assert!(drops_for_block(Block::Air).is_empty());
    }

    #[test]
    fn ores_drop_their_materials() {
        let cases = [
            (Block::CoalOre, ItemId::COAL),
            (Block::IronOre, ItemId::IRON_INGOT),
            (Block::GoldOre, ItemId::GOLD_INGOT),
            (Block::DiamondOre, ItemId::DIAMOND),
        ];
        for (block, item) in cases {
            let drops = drops_for_block(block);
            assert_eq!(drops.len(), 1);
            assert_eq!(drops[0].item, item);
        }
    }
}
