//! 工具定义与工具实例：工具类别、等级、挖掘倍率、适用方块与耐久磨损。
//!
//! 工具定义描述一类工具的固定属性，按 [`ItemId`] 集中在 [`tool_registry`] 注册；
//! 工具实例的动态状态（当前耐久、是否损坏）保存在 [`ItemStack`] 的 `durability` 字段上。
//! 两者严格分离：注册表永远不保存运行中的耐久数据。
//!
//! 所有耐久修改都必须经过 [`apply_wear`]，避免 Mining、Inventory 与 HUD 各自扣减。

use crate::world::{Block, MiningCategory};

use super::item::{ItemId, ItemStack};

/// 工具类别；决定能有效加速与采集哪一类方块。
// 锄在本版本没有对应物品，仅保留类别定义供后续扩展。
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolType {
    Pickaxe,
    Axe,
    Shovel,
    /// 锄在本版本只保留定义能力，不参与采集判定。
    Hoe,
}

impl ToolType {
    /// 展示用名称（HUD 字体只支持大写）。
    pub fn name(self) -> &'static str {
        match self {
            Self::Pickaxe => "PICKAXE",
            Self::Axe => "AXE",
            Self::Shovel => "SHOVEL",
            Self::Hoe => "HOE",
        }
    }
}

/// 工具等级；决定挖掘倍率、可采集等级与最大耐久。
// 钻石等级本版本没有对应工具，仅保留等级定义供后续扩展。
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ToolTier {
    Wood,
    Stone,
    Iron,
    Diamond,
}

impl ToolTier {
    /// 展示用名称（HUD 字体只支持大写）。
    pub fn name(self) -> &'static str {
        match self {
            Self::Wood => "WOOD",
            Self::Stone => "STONE",
            Self::Iron => "IRON",
            Self::Diamond => "DIAMOND",
        }
    }

    /// 采集等级；与方块 `harvest_level` 比较，低于要求时无法获得正常掉落。
    pub fn harvest_level(self) -> u8 {
        match self {
            Self::Wood => 0,
            Self::Stone => 1,
            Self::Iron => 2,
            Self::Diamond => 3,
        }
    }
}

/// 一类工具的固定属性；实例状态不在此结构中。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToolDefinition {
    /// 对应的物品 ID。
    pub id: ItemId,
    /// 工具类别，与工具等级相互独立。
    pub tool_type: ToolType,
    /// 工具等级。
    pub tier: ToolTier,
    /// 相对徒手的基础挖掘速度倍率，必须大于 0。
    pub mining_speed: f32,
    /// 最大耐久度，必须大于 0。
    pub max_durability: u32,
    /// 能有效加速的方块挖掘类别；锄在本版本为 `None`。
    pub effective_category: Option<MiningCategory>,
}

impl ToolDefinition {
    /// 工具等级对应的采集等级。
    pub fn harvest_level(self) -> u8 {
        self.tier.harvest_level()
    }

    /// 该工具是否适用于指定方块的挖掘类别。
    pub fn applies_to(self, block: Block) -> bool {
        matches!(self.effective_category, Some(category) if category == block.properties().mining_category)
    }
}

/// 徒手与不适用工具的基准速度倍率。
pub const HAND_SPEED: f32 = 1.0;

/// 当前手持物品对指定方块的挖掘速度倍率。
///
/// 徒手、普通物品、损坏工具以及类别不适用的工具都只获得徒手基准倍率；
/// 只有类别匹配且未损坏的工具才会应用自身倍率。
pub fn mining_speed(stack: Option<ItemStack>, block: Block) -> f32 {
    let Some(stack) = stack else {
        return HAND_SPEED;
    };
    let Some(definition) = super::tool_registry::definition(stack.item()) else {
        return HAND_SPEED;
    };
    if stack.is_broken() || !definition.applies_to(block) {
        return HAND_SPEED;
    }
    definition.mining_speed
}

/// 一次耐久消耗的结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wear {
    /// 不是带独立耐久的工具，耐久逻辑不介入。
    NotATool,
    /// 仍可继续使用，`remaining` 为剩余耐久。
    Damaged { remaining: u32 },
    /// 本次消耗后耐久归零，工具进入损坏状态。
    Broken,
}

/// 统一的耐久消耗入口；普通物品不会被修改。
///
/// `amount` 为 0 时按一次最小磨损处理，防止调用方传入 0 导致“无消耗挖掘”。
pub fn apply_wear(stack: &mut ItemStack, amount: u32) -> Wear {
    let Some(max) = stack.max_durability() else {
        return Wear::NotATool;
    };
    let current = stack.durability().unwrap_or(max);
    if current == 0 {
        return Wear::Broken;
    }

    let next = current.saturating_sub(amount.max(1));
    *stack = stack.with_durability(next);
    if next == 0 {
        Wear::Broken
    } else {
        Wear::Damaged { remaining: next }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::ItemId;
    use crate::world::Block;

    fn tool(item: ItemId, durability: u32) -> ItemStack {
        ItemStack::new(item, 1)
            .expect("工具堆叠合法")
            .with_durability(durability)
    }

    #[test]
    fn tool_types_and_tiers_are_distinct() {
        assert_eq!(ToolType::Pickaxe.name(), "PICKAXE");
        assert_ne!(ToolType::Pickaxe, ToolType::Hoe);
        assert_eq!(ToolTier::Wood.harvest_level(), 0);
        assert_eq!(ToolTier::Stone.harvest_level(), 1);
        assert_eq!(ToolTier::Iron.harvest_level(), 2);
        assert_eq!(ToolTier::Diamond.harvest_level(), 3);
    }

    #[test]
    fn definitions_only_apply_to_their_mining_category() {
        let pickaxe =
            super::super::tool_registry::definition(ItemId::WOODEN_PICKAXE).expect("木镐应已注册");
        assert!(pickaxe.applies_to(Block::Stone));
        assert!(!pickaxe.applies_to(Block::Dirt));
        assert_eq!(pickaxe.harvest_level(), 0);
    }

    #[test]
    fn mining_speed_falls_back_to_hand_for_invalid_cases() {
        assert_eq!(mining_speed(None, Block::Stone), HAND_SPEED);
        assert_eq!(
            mining_speed(
                Some(ItemStack::new(ItemId::STONE, 1).expect("合法堆叠")),
                Block::Stone
            ),
            HAND_SPEED,
            "普通物品不能提速"
        );
        assert_eq!(
            mining_speed(
                Some(ItemStack::new(ItemId::WOODEN_PICKAXE, 1).expect("工具堆叠合法")),
                Block::Stone
            ),
            2.0
        );
        assert_eq!(
            mining_speed(Some(tool(ItemId::WOODEN_PICKAXE, 0)), Block::Stone),
            HAND_SPEED,
            "损坏工具不能提供效率加成"
        );
        assert_eq!(
            mining_speed(
                Some(ItemStack::new(ItemId::WOODEN_PICKAXE, 1).expect("工具堆叠合法")),
                Block::Dirt
            ),
            HAND_SPEED,
            "类别不适用时只获得徒手倍率"
        );
    }

    #[test]
    fn wear_only_affects_tools_and_saturates_at_zero() {
        let mut stone = ItemStack::new(ItemId::STONE, 1).expect("合法堆叠");
        assert_eq!(apply_wear(&mut stone, 1), Wear::NotATool);
        assert_eq!(stone.durability(), None);

        let mut pickaxe = tool(ItemId::STONE_PICKAXE, 3);
        assert_eq!(apply_wear(&mut pickaxe, 1), Wear::Damaged { remaining: 2 });
        assert_eq!(apply_wear(&mut pickaxe, 5), Wear::Broken);
        assert_eq!(pickaxe.durability(), Some(0));
        assert_eq!(apply_wear(&mut pickaxe, 1), Wear::Broken, "已损坏不再扣减");
    }

    #[test]
    fn zero_wear_amount_still_consumes_one_point() {
        let mut pickaxe = tool(ItemId::WOODEN_PICKAXE, 60);
        assert_eq!(apply_wear(&mut pickaxe, 0), Wear::Damaged { remaining: 59 });
    }
}
