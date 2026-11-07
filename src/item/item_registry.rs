//! 物品注册表：集中定义物品名称、堆叠上限、图标与关联方块。
//!
//! 注册表按稳定物品 ID 索引；新增物品只能追加到末尾，不得调整已有 ID，
//! 否则旧存档中的物品数据会错位。

use glam::Vec3;

use super::item::{ItemDefinition, ItemIcon, ItemId, ItemType};
use crate::world::Block;

/// 基础方块物品的统一堆叠上限。
pub const DEFAULT_MAX_STACK_SIZE: u32 = 64;

const INVALID: ItemDefinition = ItemDefinition {
    id: ItemId::INVALID,
    name: "Air",
    max_stack_size: 0,
    icon: ItemIcon::NONE,
    associated_block: None,
    item_type: ItemType::Material,
};

const STONE: ItemDefinition = ItemDefinition {
    id: ItemId::STONE,
    name: "Stone",
    max_stack_size: DEFAULT_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.55, 0.55, 0.58),
    },
    associated_block: Some(Block::Stone),
    item_type: ItemType::Block,
};

const DIRT: ItemDefinition = ItemDefinition {
    id: ItemId::DIRT,
    name: "Dirt",
    max_stack_size: DEFAULT_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.45, 0.33, 0.22),
    },
    associated_block: Some(Block::Dirt),
    item_type: ItemType::Block,
};

const GRASS: ItemDefinition = ItemDefinition {
    id: ItemId::GRASS,
    name: "Grass",
    max_stack_size: DEFAULT_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.35, 0.62, 0.32),
    },
    associated_block: Some(Block::Grass),
    item_type: ItemType::Block,
};

/// 全部物品定义，下标必须等于物品 ID。
const DEFINITIONS: [ItemDefinition; 4] = [INVALID, STONE, DIRT, GRASS];

/// 按物品 ID 查询定义；无效 ID 返回 `None`。
pub fn definition(item: ItemId) -> Option<&'static ItemDefinition> {
    DEFINITIONS
        .get(item.raw() as usize)
        .filter(|definition| definition.id != ItemId::INVALID && definition.id == item)
}

/// 某种方块对应的物品；不可掉落的方块返回 `None`。
///
/// 当前由掉落映射与测试使用，作为方块到物品的反向查询保留。
#[allow(dead_code)]
pub fn item_for_block(block: Block) -> Option<ItemId> {
    DEFINITIONS
        .iter()
        .find(|definition| {
            definition.id != ItemId::INVALID && definition.associated_block == Some(block)
        })
        .map(|definition| definition.id)
}

/// 物品关联的可放置方块；非方块物品或无效 ID 返回 `None`。
pub fn block_for_item(item: ItemId) -> Option<Block> {
    let definition = definition(item)?;
    let block = definition.associated_block?;
    definition.is_placeable().then_some(block)
}

/// 物品的最大堆叠数量；无效 ID 返回 `None`。
pub fn max_stack_size(item: ItemId) -> Option<u32> {
    definition(item).map(|definition| definition.max_stack_size)
}

/// 物品展示名称；无效 ID 返回 `"?"`。
pub fn name(item: ItemId) -> &'static str {
    definition(item).map_or("?", |definition| definition.name)
}

/// 物品图标颜色；无效 ID 返回透明黑。
pub fn icon_color(item: ItemId) -> Vec3 {
    definition(item).map_or(Vec3::ZERO, |definition| definition.icon.color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_match_table_order() {
        let definitions = [STONE, DIRT, GRASS];
        for definition in definitions {
            assert_eq!(
                DEFINITIONS.get(definition.id.raw() as usize),
                Some(&definition)
            );
            assert_eq!(
                definition.id,
                item_for_block(definition.associated_block.expect("方块物品必须关联方块"))
                    .expect("方块必须有对应物品")
            );
        }
        let mut ids: Vec<u16> = DEFINITIONS
            .iter()
            .map(|definition| definition.id.raw())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), DEFINITIONS.len(), "物品 ID 必须唯一");
    }

    #[test]
    fn invalid_ids_are_rejected_by_every_lookup() {
        assert!(definition(ItemId::INVALID).is_none());
        assert!(block_for_item(ItemId::INVALID).is_none());
        assert!(max_stack_size(ItemId::INVALID).is_none());
        assert_eq!(name(ItemId::INVALID), "?");
        assert_eq!(icon_color(ItemId::INVALID), Vec3::ZERO);
    }

    #[test]
    fn block_items_map_both_directions() {
        assert_eq!(item_for_block(Block::Stone), Some(ItemId::STONE));
        assert_eq!(block_for_item(ItemId::STONE), Some(Block::Stone));
        assert_eq!(item_for_block(Block::Air), None);
        assert_eq!(block_for_item(ItemId::INVALID), None);
    }

    #[test]
    fn every_block_item_stacks_to_the_shared_limit() {
        for item in [ItemId::STONE, ItemId::DIRT, ItemId::GRASS] {
            let definition = definition(item).expect("物品应存在");
            assert_eq!(definition.max_stack_size, DEFAULT_MAX_STACK_SIZE);
            assert!(definition.is_placeable());
        }
    }
}
