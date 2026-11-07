//! 方块快捷栏：只负责选择当前放置的方块类型，不修改世界，也不代表物品库存。
//!
//! 槽位数量与内容集中在这里定义；数字键、鼠标滚轮与右键放置都通过该状态解耦。

use crate::world::Block;

/// 快捷栏槽位数量。
pub const HOTBAR_SLOTS: usize = 5;

/// 默认槽位：按当前已支持的方块的 grass / dirt / stone 循环排布。
pub const DEFAULT_SLOTS: [Block; HOTBAR_SLOTS] = [
    Block::Grass,
    Block::Dirt,
    Block::Stone,
    Block::Grass,
    Block::Dirt,
];

/// 当前选中的方块槽位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotbar {
    slots: [Block; HOTBAR_SLOTS],
    selected: usize,
}

impl Default for Hotbar {
    fn default() -> Self {
        Self::new()
    }
}

impl Hotbar {
    /// 使用默认槽位创建快捷栏，默认选中第一格。
    pub fn new() -> Self {
        Self {
            slots: DEFAULT_SLOTS,
            selected: 0,
        }
    }

    /// 全部槽位。
    pub fn slots(&self) -> &[Block; HOTBAR_SLOTS] {
        &self.slots
    }

    /// 当前选中槽位的下标。
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// 当前选中槽位的方块类型，始终是可放置方块。
    pub fn selected_block(&self) -> Block {
        self.slots[self.selected]
    }

    /// 选择指定槽位；越界或该槽位不是可放置方块时返回 `false` 并保持原选择。
    pub fn select_slot(&mut self, index: usize) -> bool {
        let Some(block) = self.slots.get(index).copied() else {
            return false;
        };
        if !block.is_placeable() {
            return false;
        }
        self.selected = index;
        true
    }

    /// 按滚轮方向切换槽位：`delta > 0` 向前一格，`delta < 0` 向后一格，循环选择。
    pub fn scroll(&mut self, delta: f32) {
        if delta > 0.0 {
            self.selected = (self.selected + HOTBAR_SLOTS - 1) % HOTBAR_SLOTS;
        } else if delta < 0.0 {
            self.selected = (self.selected + 1) % HOTBAR_SLOTS;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_hotbar_is_selectable_and_placeable() {
        let hotbar = Hotbar::new();
        assert_eq!(hotbar.selected_index(), 0);
        assert_eq!(hotbar.selected_block(), Block::Grass);
        for block in hotbar.slots() {
            assert!(block.is_placeable(), "槽位包含不可放置方块");
        }
    }

    #[test]
    fn slot_selection_updates_the_place_block() {
        let mut hotbar = Hotbar::new();
        assert!(hotbar.select_slot(2));
        assert_eq!(hotbar.selected_index(), 2);
        assert_eq!(hotbar.selected_block(), Block::Stone);
    }

    #[test]
    fn out_of_range_slots_are_rejected() {
        let mut hotbar = Hotbar::new();
        assert!(!hotbar.select_slot(HOTBAR_SLOTS));
        assert!(!hotbar.select_slot(usize::MAX));
        assert_eq!(hotbar.selected_index(), 0);
    }

    #[test]
    fn air_cannot_be_selected_even_if_present_in_a_slot() {
        let mut hotbar = Hotbar {
            slots: [Block::Air; HOTBAR_SLOTS],
            selected: 0,
        };
        assert!(!hotbar.select_slot(0));
        assert_eq!(hotbar.selected_index(), 0);
    }

    #[test]
    fn scrolling_wraps_in_both_directions() {
        let mut hotbar = Hotbar::new();
        hotbar.scroll(1.0);
        assert_eq!(hotbar.selected_index(), HOTBAR_SLOTS - 1);
        hotbar.scroll(-1.0);
        assert_eq!(hotbar.selected_index(), 0);
        hotbar.scroll(-1.0);
        assert_eq!(hotbar.selected_index(), 1);
        hotbar.scroll(0.0);
        assert_eq!(hotbar.selected_index(), 1);
    }
}
