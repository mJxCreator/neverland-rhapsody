//! 采集规则：判定方块是否满足采集条件，以及破坏后应生成哪些掉落。
//!
//! 采集判定同时检查“工具类别是否匹配”与“工具等级是否足够”，两者相互独立：
//! 类别不匹配或等级不足都不会生成正常掉落。判定结果与掉落表解耦，
//! 挖掘代码只消费 [`drops_for_harvest`]，不直接读取方块属性做条件分支。
//!
//! 同一方块每次破坏只调用一次掉落生成，避免重复掉落。

use crate::item::tool_registry;
use crate::item::{drops_for_block, BlockDrop, ItemId, ItemStack};
use crate::world::{Block, DropRule};

/// 一次采集判定所需的工具状态。
///
/// `item` 为 `None` 表示徒手；`broken` 只在物品确实是工具时才有意义。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HarvestTool {
    pub item: Option<ItemId>,
    pub broken: bool,
}

impl HarvestTool {
    /// 徒手状态。
    pub const HAND: Self = Self {
        item: None,
        broken: false,
    };

    /// 从当前手持堆叠解析；普通物品按徒手处理。
    pub fn from_stack(stack: Option<ItemStack>) -> Self {
        match stack {
            Some(stack) if stack.is_tool() => Self {
                item: Some(stack.item()),
                broken: stack.is_broken(),
            },
            _ => Self::HAND,
        }
    }
}

/// 方块在指定工具状态下能否获得正常掉落。
///
/// 工具类别必须与方块要求一致，且工具采集等级不低于方块要求；
/// 损坏的工具与不存在的工具等级都视为不满足。
pub fn can_harvest(block: Block, tool: HarvestTool) -> bool {
    let properties = block.properties();
    if !properties.breakable {
        return false;
    }

    let Some(required) = properties.required_tool else {
        return true;
    };
    if tool.broken {
        return false;
    }
    let Some(definition) = tool.item.and_then(tool_registry::definition) else {
        return false;
    };

    definition.tool_type == required && definition.harvest_level() >= properties.harvest_level
}

/// 方块被破坏后应生成的掉落列表；不满足采集条件时按掉落规则返回空列表。
pub fn drops_for_harvest(block: Block, tool: HarvestTool) -> &'static [BlockDrop] {
    let properties = block.properties();
    if !properties.breakable {
        return &[];
    }

    match properties.drop_rule {
        DropRule::Never => &[],
        DropRule::Always => drops_for_block(block),
        DropRule::RequiresCorrectTool => {
            if can_harvest(block, tool) {
                drops_for_block(block)
            } else {
                &[]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(item: ItemId) -> HarvestTool {
        HarvestTool {
            item: Some(item),
            broken: false,
        }
    }

    fn broken_tool(item: ItemId) -> HarvestTool {
        HarvestTool {
            item: Some(item),
            broken: true,
        }
    }

    #[test]
    fn hard_blocks_require_the_right_tool_type_and_tier() {
        assert!(!can_harvest(Block::Stone, HarvestTool::HAND));
        assert!(
            can_harvest(Block::Stone, tool(ItemId::WOODEN_PICKAXE)),
            "v0.14 起木镐即可采集石头"
        );
        assert!(
            !can_harvest(Block::Stone, tool(ItemId::STONE_SHOVEL)),
            "类别不匹配"
        );
        assert!(can_harvest(Block::Stone, tool(ItemId::STONE_PICKAXE)));
        assert!(can_harvest(Block::Stone, tool(ItemId::IRON_PICKAXE)));
        assert!(
            !can_harvest(Block::Stone, broken_tool(ItemId::IRON_PICKAXE)),
            "损坏工具不能采集"
        );
    }

    #[test]
    fn soft_blocks_drop_even_without_a_tool() {
        for block in [Block::Dirt, Block::Grass] {
            assert!(can_harvest(block, HarvestTool::HAND));
            let drops = drops_for_harvest(block, HarvestTool::HAND);
            assert_eq!(drops.len(), 1);
            assert_eq!(drops[0].count, 1);
        }
    }

    #[test]
    fn wood_can_be_gathered_by_hand_to_bootstrap_crafting() {
        assert!(can_harvest(Block::Wood, HarvestTool::HAND));
        let drops = drops_for_harvest(Block::Wood, HarvestTool::HAND);
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].item, ItemId::WOOD);
    }

    #[test]
    fn unmet_conditions_produce_no_drops_for_required_blocks() {
        assert!(drops_for_harvest(Block::Stone, HarvestTool::HAND).is_empty());

        let wood_pickaxe = drops_for_harvest(Block::Stone, tool(ItemId::WOODEN_PICKAXE));
        assert_eq!(wood_pickaxe.len(), 1);
        assert_eq!(wood_pickaxe[0].item, ItemId::STONE);

        let drops = drops_for_harvest(Block::Stone, tool(ItemId::STONE_PICKAXE));
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].item, ItemId::STONE);
    }

    #[test]
    fn air_never_drops_and_is_never_harvestable() {
        assert!(!can_harvest(Block::Air, HarvestTool::HAND));
        assert!(drops_for_harvest(Block::Air, tool(ItemId::IRON_PICKAXE)).is_empty());
    }

    #[test]
    fn non_tool_stacks_are_treated_as_bare_hands() {
        let dirt = ItemStack::new(ItemId::DIRT, 1).expect("合法堆叠");
        assert_eq!(HarvestTool::from_stack(Some(dirt)), HarvestTool::HAND);
        assert_eq!(HarvestTool::from_stack(None), HarvestTool::HAND);

        let pickaxe = ItemStack::new(ItemId::STONE_PICKAXE, 1)
            .expect("合法堆叠")
            .with_durability(0);
        assert_eq!(
            HarvestTool::from_stack(Some(pickaxe)),
            HarvestTool {
                item: Some(ItemId::STONE_PICKAXE),
                broken: true,
            }
        );
    }
}
