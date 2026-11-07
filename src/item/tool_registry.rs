//! 工具注册表：按物品 ID 集中注册工具类别、等级、挖掘倍率、适用方块与耐久上限。
//!
//! 工具定义通过 [`ItemId`] 与物品注册表关联，查询统一走 [`definition`]；
//! 无效物品 ID 或不存在的工具都返回 `None`，调用方按“徒手”处理。
//! 新增工具只能追加定义，不得修改已有数值，避免旧存档中的实例状态与新定义不一致。

use super::item::ItemId;
use super::tool::{ToolDefinition, ToolTier, ToolType};
use crate::world::MiningCategory;

const WOODEN_PICKAXE: ToolDefinition = ToolDefinition {
    id: ItemId::WOODEN_PICKAXE,
    tool_type: ToolType::Pickaxe,
    tier: ToolTier::Wood,
    mining_speed: 2.0,
    max_durability: 60,
    effective_category: Some(MiningCategory::Stone),
};

const STONE_PICKAXE: ToolDefinition = ToolDefinition {
    id: ItemId::STONE_PICKAXE,
    tool_type: ToolType::Pickaxe,
    tier: ToolTier::Stone,
    mining_speed: 4.0,
    max_durability: 132,
    effective_category: Some(MiningCategory::Stone),
};

const IRON_PICKAXE: ToolDefinition = ToolDefinition {
    id: ItemId::IRON_PICKAXE,
    tool_type: ToolType::Pickaxe,
    tier: ToolTier::Iron,
    mining_speed: 6.0,
    max_durability: 251,
    effective_category: Some(MiningCategory::Stone),
};

const WOODEN_AXE: ToolDefinition = ToolDefinition {
    id: ItemId::WOODEN_AXE,
    tool_type: ToolType::Axe,
    tier: ToolTier::Wood,
    mining_speed: 2.0,
    max_durability: 60,
    effective_category: Some(MiningCategory::Wood),
};

const STONE_AXE: ToolDefinition = ToolDefinition {
    id: ItemId::STONE_AXE,
    tool_type: ToolType::Axe,
    tier: ToolTier::Stone,
    mining_speed: 4.0,
    max_durability: 132,
    effective_category: Some(MiningCategory::Wood),
};

const IRON_AXE: ToolDefinition = ToolDefinition {
    id: ItemId::IRON_AXE,
    tool_type: ToolType::Axe,
    tier: ToolTier::Iron,
    mining_speed: 6.0,
    max_durability: 251,
    effective_category: Some(MiningCategory::Wood),
};

const WOODEN_SHOVEL: ToolDefinition = ToolDefinition {
    id: ItemId::WOODEN_SHOVEL,
    tool_type: ToolType::Shovel,
    tier: ToolTier::Wood,
    mining_speed: 2.0,
    max_durability: 60,
    effective_category: Some(MiningCategory::Dirt),
};

const STONE_SHOVEL: ToolDefinition = ToolDefinition {
    id: ItemId::STONE_SHOVEL,
    tool_type: ToolType::Shovel,
    tier: ToolTier::Stone,
    mining_speed: 4.0,
    max_durability: 132,
    effective_category: Some(MiningCategory::Dirt),
};

const IRON_SHOVEL: ToolDefinition = ToolDefinition {
    id: ItemId::IRON_SHOVEL,
    tool_type: ToolType::Shovel,
    tier: ToolTier::Iron,
    mining_speed: 6.0,
    max_durability: 251,
    effective_category: Some(MiningCategory::Dirt),
};

/// 全部工具定义，必须与物品注册表中的工具 ID 一一对应。
const DEFINITIONS: [ToolDefinition; 9] = [
    WOODEN_PICKAXE,
    STONE_PICKAXE,
    IRON_PICKAXE,
    WOODEN_AXE,
    STONE_AXE,
    IRON_AXE,
    WOODEN_SHOVEL,
    STONE_SHOVEL,
    IRON_SHOVEL,
];

/// 新建世界时放入快捷栏的示例工具，1→9 号槽位依次对应。
///
/// 本版本没有合成系统，示例工具保证工具系统可以直接实机验收。
pub const STARTER_TOOLS: [ItemId; 9] = [
    ItemId::WOODEN_PICKAXE,
    ItemId::STONE_PICKAXE,
    ItemId::IRON_PICKAXE,
    ItemId::WOODEN_AXE,
    ItemId::STONE_AXE,
    ItemId::IRON_AXE,
    ItemId::WOODEN_SHOVEL,
    ItemId::STONE_SHOVEL,
    ItemId::IRON_SHOVEL,
];

/// 按物品 ID 查询工具定义；非工具或无效 ID 返回 `None`。
pub fn definition(item: ItemId) -> Option<&'static ToolDefinition> {
    DEFINITIONS.iter().find(|definition| definition.id == item)
}

/// 全部已注册工具定义；供测试与后续合成/配方系统遍历。
#[allow(dead_code)]
pub fn definitions() -> &'static [ToolDefinition] {
    &DEFINITIONS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::item_registry;

    #[test]
    fn every_tool_id_is_unique_and_registered_as_an_item() {
        let mut ids: Vec<u16> = DEFINITIONS
            .iter()
            .map(|definition| definition.id.raw())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), DEFINITIONS.len(), "工具 ID 必须唯一");

        for definition in DEFINITIONS {
            let item = item_registry::definition(definition.id).expect("工具必须有物品定义");
            assert_eq!(item.max_stack_size, 1, "带耐久的工具不能堆叠");
            assert!(definition.mining_speed > 0.0);
            assert!(definition.max_durability > 0);
            assert!(definition.effective_category.is_some());
        }
    }

    #[test]
    fn lookups_reject_non_tools_and_invalid_ids() {
        assert!(definition(ItemId::INVALID).is_none());
        assert!(definition(ItemId::STONE).is_none());
        assert!(definition(ItemId::WOODEN_PICKAXE).is_some());
    }

    #[test]
    fn tiers_and_types_match_the_design_table() {
        let wood = definition(ItemId::WOODEN_PICKAXE).expect("木镐应已注册");
        let stone = definition(ItemId::STONE_PICKAXE).expect("石镐应已注册");
        let iron = definition(ItemId::IRON_PICKAXE).expect("铁镐应已注册");
        assert_eq!((wood.mining_speed, wood.max_durability), (2.0, 60));
        assert_eq!((stone.mining_speed, stone.max_durability), (4.0, 132));
        assert_eq!((iron.mining_speed, iron.max_durability), (6.0, 251));
        assert_eq!(wood.tier, ToolTier::Wood);
        assert_eq!(stone.tier, ToolTier::Stone);
        assert_eq!(iron.tier, ToolTier::Iron);
        assert_eq!(stone.tool_type, ToolType::Pickaxe);
    }

    #[test]
    fn starter_tools_are_all_registered_and_unique() {
        let mut raws: Vec<u16> = STARTER_TOOLS.iter().map(|item| item.raw()).collect();
        raws.sort_unstable();
        raws.dedup();
        assert_eq!(raws.len(), STARTER_TOOLS.len());
        for item in STARTER_TOOLS {
            assert!(definition(item).is_some());
        }
    }
}
