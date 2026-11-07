//! 通用物品使用入口：把“手上拿着什么”映射为明确的交互行为。
//!
//! 行为归属由物品类别决定，`app.rs` 只负责调用入口并执行 [`ItemUse`]，
//! 不在输入处理中堆叠物品分支。后续新增材料、消耗品等行为时，
//! 只需在这里补充分类，不必修改输入层。
//!
//! 本版本：工具支持主要使用（挖掘），方块物品保留次要使用（放置）；
//! 被动效果暂未实现，预留于后续版本。

use crate::item::{block_for_item, tool_registry, ItemStack};
use crate::world::Block;

/// 一次物品使用的结果语义。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemUse {
    /// 放置指定方块。
    PlaceBlock(Block),
    /// 工具使用：进入挖掘流程。
    UseTool,
    /// 当前物品没有可用行为。
    Nothing,
}

/// 主要使用（左键）：工具进入挖掘流程，其他物品无行为。
///
/// 已损坏的工具仍允许挥动，但挖掘效率会回退到徒手基准。
pub fn primary_use(stack: Option<ItemStack>) -> ItemUse {
    match stack {
        Some(stack) if tool_registry::definition(stack.item()).is_some() => ItemUse::UseTool,
        _ => ItemUse::Nothing,
    }
}

/// 次要使用（右键）：可放置方块执行放置，工具本版本没有右键行为。
pub fn secondary_use(stack: Option<ItemStack>) -> ItemUse {
    let Some(stack) = stack else {
        return ItemUse::Nothing;
    };
    if tool_registry::definition(stack.item()).is_some() {
        return ItemUse::Nothing;
    }
    match block_for_item(stack.item()) {
        Some(block) => ItemUse::PlaceBlock(block),
        None => ItemUse::Nothing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::ItemId;

    #[test]
    fn tools_use_primary_action_and_have_no_secondary_action() {
        let pickaxe = ItemStack::new(ItemId::WOODEN_PICKAXE, 1).expect("工具堆叠合法");
        assert_eq!(primary_use(Some(pickaxe)), ItemUse::UseTool);
        assert_eq!(secondary_use(Some(pickaxe)), ItemUse::Nothing);

        let broken = pickaxe.with_durability(0);
        assert_eq!(primary_use(Some(broken)), ItemUse::UseTool);
    }

    #[test]
    fn block_items_place_on_secondary_use() {
        let grass = ItemStack::new(ItemId::GRASS, 5).expect("合法堆叠");
        assert_eq!(
            secondary_use(Some(grass)),
            ItemUse::PlaceBlock(Block::Grass)
        );
        assert_eq!(primary_use(Some(grass)), ItemUse::Nothing);
    }

    #[test]
    fn empty_hand_and_invalid_stacks_do_nothing() {
        assert_eq!(primary_use(None), ItemUse::Nothing);
        assert_eq!(secondary_use(None), ItemUse::Nothing);
    }
}
