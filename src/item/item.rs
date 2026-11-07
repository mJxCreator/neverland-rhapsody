//! 物品基础数据：稳定的物品 ID、物品类别、物品定义与物品堆叠。
//!
//! 物品独立于方块系统：方块描述世界中的实体，物品描述玩家能够持有与放置的内容。
//! 物品 ID 与注册表顺序解耦，只能追加，不得重排已有 ID。

use glam::Vec3;

use super::item_registry;
use crate::world::Block;

/// 稳定的物品 ID；`INVALID` 表示不可用的物品 ID。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemId(u16);

impl ItemId {
    /// 不可用物品 ID：任何接口收到该值都必须按“无物品”处理。
    pub const INVALID: Self = Self(0);
    pub const STONE: Self = Self(1);
    pub const DIRT: Self = Self(2);
    pub const GRASS: Self = Self(3);
    pub const WOODEN_PICKAXE: Self = Self(4);
    pub const STONE_PICKAXE: Self = Self(5);
    pub const IRON_PICKAXE: Self = Self(6);
    pub const WOODEN_AXE: Self = Self(7);
    pub const STONE_AXE: Self = Self(8);
    pub const IRON_AXE: Self = Self(9);
    pub const WOODEN_SHOVEL: Self = Self(10);
    pub const STONE_SHOVEL: Self = Self(11);
    pub const IRON_SHOVEL: Self = Self(12);

    /// 存档与跨模块传递使用的原始数值。
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// 由原始数值还原物品 ID；只有注册表中存在的 ID 才有效。
    pub fn from_raw(raw: u16) -> Option<Self> {
        let id = Self(raw);
        id.is_valid().then_some(id)
    }

    /// 该 ID 是否有对应的物品定义。
    pub fn is_valid(self) -> bool {
        item_registry::definition(self).is_some()
    }
}

/// 物品类别；工具与方块为当前实现，`Material` 与 `Consumable` 为后续版本预留。
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemType {
    Block,
    Tool,
    Material,
    Consumable,
}

/// 物品图标；当前用纯色表示，后续可替换为图集索引。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemIcon {
    pub color: Vec3,
}

impl ItemIcon {
    pub const NONE: Self = Self { color: Vec3::ZERO };
}

/// 一种物品的全部静态属性。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemDefinition {
    /// 稳定物品 ID。
    pub id: ItemId,
    /// 展示用名称。
    pub name: &'static str,
    /// 单个槽位的最大堆叠数量，必须大于 0。
    pub max_stack_size: u32,
    /// 图标信息。
    pub icon: ItemIcon,
    /// 关联方块；有值时该物品可用于放置。
    pub associated_block: Option<Block>,
    /// 物品类别。
    pub item_type: ItemType,
}

impl ItemDefinition {
    /// 该物品能否放置为方块。
    pub fn is_placeable(self) -> bool {
        matches!((self.item_type, self.associated_block), (ItemType::Block, Some(block)) if block.is_placeable())
    }
}

/// 某一物品及其数量；构造后数量始终位于 `1..=max_stack_size`。
///
/// 工具在 [`ItemStack`] 上携带独立耐久度：`durability` 为 `None` 表示普通物品，
/// `Some(n)` 表示工具实例的剩余耐久，`Some(0)` 表示已损坏。耐久属于实例状态，
/// 不会写入物品注册表。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemStack {
    item: ItemId,
    count: u32,
    durability: Option<u32>,
}

impl ItemStack {
    /// 创建物品堆叠；无效物品 ID、数量为 0 或超过堆叠上限时返回 `None`。
    ///
    /// 工具按最大耐久初始化；普通物品不携带耐久字段。
    pub fn new(item: ItemId, count: u32) -> Option<Self> {
        let max = item_registry::max_stack_size(item)?;
        if count == 0 || count > max {
            return None;
        }
        let durability =
            super::tool_registry::definition(item).map(|definition| definition.max_durability);
        Some(Self {
            item,
            count,
            durability,
        })
    }

    /// 以相同物品生成指定数量的堆叠；数量非法时返回 `None`，耐久保持不变。
    pub fn with_count(self, count: u32) -> Option<Self> {
        let max = item_registry::max_stack_size(self.item)?;
        if count == 0 || count > max {
            return None;
        }
        Some(Self { count, ..self })
    }

    /// 返回带指定耐久的副本；普通物品原样返回，数值会被限制在 `0..=最大耐久`。
    pub fn with_durability(self, durability: u32) -> Self {
        match self.max_durability() {
            Some(max) => Self {
                durability: Some(durability.min(max)),
                ..self
            },
            None => self,
        }
    }

    pub fn item(self) -> ItemId {
        self.item
    }

    pub fn count(self) -> u32 {
        self.count
    }

    /// 工具实例的剩余耐久；普通物品返回 `None`。
    pub fn durability(self) -> Option<u32> {
        self.durability
    }

    /// 该物品是否为工具。
    pub fn is_tool(self) -> bool {
        self.durability.is_some()
    }

    /// 工具的最大耐久；普通物品返回 `None`。
    pub fn max_durability(self) -> Option<u32> {
        super::tool_registry::definition(self.item).map(|definition| definition.max_durability)
    }

    /// 工具是否已经损坏；普通物品始终为 `false`。
    pub fn is_broken(self) -> bool {
        self.durability == Some(0)
    }

    /// 该物品的最大堆叠数量。
    pub fn max_stack_size(self) -> u32 {
        item_registry::max_stack_size(self.item).unwrap_or(0)
    }

    /// 当前堆叠还能容纳的数量。
    pub fn room(self) -> u32 {
        self.max_stack_size().saturating_sub(self.count)
    }

    /// 两种物品能否堆叠到同一槽位；工具还要求耐久完全一致。
    pub fn can_merge_with(self, other: Self) -> bool {
        self.item == other.item && self.durability == other.durability
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_ids_are_stable_and_validated() {
        assert_eq!(ItemId::STONE.raw(), 1);
        assert_eq!(ItemId::DIRT.raw(), 2);
        assert_eq!(ItemId::GRASS.raw(), 3);
        assert!(ItemId::STONE.is_valid());
        assert!(!ItemId::INVALID.is_valid());
        assert!(ItemId::from_raw(0).is_none());
        assert!(ItemId::from_raw(u16::MAX).is_none());
        assert_eq!(ItemId::from_raw(1), Some(ItemId::STONE));
    }

    #[test]
    fn stacks_reject_zero_and_overflow() {
        assert!(ItemStack::new(ItemId::STONE, 0).is_none());
        assert!(ItemStack::new(ItemId::STONE, 65).is_none());
        assert!(ItemStack::new(ItemId::INVALID, 1).is_none());
        let stack = ItemStack::new(ItemId::STONE, 64).expect("64 应合法");
        assert_eq!(stack.count(), 64);
        assert_eq!(stack.room(), 0);
    }

    #[test]
    fn stacks_only_merge_with_the_same_item() {
        let stone = ItemStack::new(ItemId::STONE, 1).expect("合法堆叠");
        let dirt = ItemStack::new(ItemId::DIRT, 1).expect("合法堆叠");
        assert!(stone.can_merge_with(stone));
        assert!(!stone.can_merge_with(dirt));
    }

    #[test]
    fn block_items_expose_their_associated_block() {
        let definition = item_registry::definition(ItemId::GRASS).expect("草物品应存在");
        assert_eq!(definition.associated_block, Some(Block::Grass));
        assert!(definition.is_placeable());
        assert_eq!(definition.item_type, ItemType::Block);
    }

    #[test]
    fn tools_carry_instance_durability_without_stacking() {
        let pickaxe = ItemStack::new(ItemId::IRON_PICKAXE, 1).expect("工具堆叠合法");
        assert!(pickaxe.is_tool());
        assert_eq!(pickaxe.durability(), Some(251));
        assert_eq!(pickaxe.max_durability(), Some(251));
        assert!(
            ItemStack::new(ItemId::IRON_PICKAXE, 2).is_none(),
            "工具不能堆叠"
        );

        let worn = pickaxe.with_durability(100);
        assert_eq!(worn.durability(), Some(100));
        assert!(!worn.can_merge_with(pickaxe), "不同耐久的工具不能合并");
        assert!(worn.can_merge_with(worn));

        let normal = ItemStack::new(ItemId::STONE, 3).expect("合法堆叠");
        assert!(!normal.is_tool());
        assert_eq!(normal.durability(), None);
        assert_eq!(normal.with_durability(5).durability(), None);
    }

    #[test]
    fn durability_is_clamped_to_the_maximum() {
        let pickaxe = ItemStack::new(ItemId::WOODEN_PICKAXE, 1).expect("工具堆叠合法");
        assert_eq!(pickaxe.with_durability(999).durability(), Some(60));
        assert!(pickaxe.with_durability(0).is_broken());
        assert!(!pickaxe.is_broken());
    }
}
