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

/// 物品类别；当前只实现方块物品，`Tool` 与 `Consumable` 为后续版本预留。
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemStack {
    item: ItemId,
    count: u32,
}

impl ItemStack {
    /// 创建物品堆叠；无效物品 ID、数量为 0 或超过堆叠上限时返回 `None`。
    pub fn new(item: ItemId, count: u32) -> Option<Self> {
        let max = item_registry::max_stack_size(item)?;
        if count == 0 || count > max {
            return None;
        }
        Some(Self { item, count })
    }

    /// 以相同物品生成指定数量的堆叠；数量非法时返回 `None`。
    pub fn with_count(self, count: u32) -> Option<Self> {
        Self::new(self.item, count)
    }

    pub fn item(self) -> ItemId {
        self.item
    }

    pub fn count(self) -> u32 {
        self.count
    }

    /// 该物品的最大堆叠数量。
    pub fn max_stack_size(self) -> u32 {
        item_registry::max_stack_size(self.item).unwrap_or(0)
    }

    /// 当前堆叠还能容纳的数量。
    pub fn room(self) -> u32 {
        self.max_stack_size().saturating_sub(self.count)
    }

    /// 两种物品能否堆叠到同一槽位。
    pub fn can_merge_with(self, other: Self) -> bool {
        self.item == other.item
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
}
