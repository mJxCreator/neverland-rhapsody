//! 快捷栏：只负责“当前选中的槽位下标”，物品内容完全来自背包，不再单独存储。
//!
//! 数字键、鼠标滚轮与右键放置都通过该状态解耦；选中空槽位是合法的，
//! 但放置时由背包与放置规则共同决定是否成功。

use crate::item::HOTBAR_SLOTS;

/// 当前选中的快捷栏槽位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotbar {
    selected: usize,
}

impl Default for Hotbar {
    fn default() -> Self {
        Self::new()
    }
}

impl Hotbar {
    /// 默认选中第一格。
    pub fn new() -> Self {
        Self { selected: 0 }
    }

    /// 当前选中槽位的下标，始终位于 `0..HOTBAR_SLOTS`。
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// 选择指定槽位；越界时返回 `false` 并保持原选择。
    pub fn select_slot(&mut self, index: usize) -> bool {
        if index >= HOTBAR_SLOTS {
            return false;
        }
        self.selected = index;
        true
    }

    /// 恢复存档中的选择；越界值会被限制到最后一个槽位。
    pub fn set_selected(&mut self, index: usize) {
        self.selected = index.min(HOTBAR_SLOTS - 1);
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
    fn default_selection_is_the_first_slot() {
        let hotbar = Hotbar::new();
        assert_eq!(hotbar.selected_index(), 0);
    }

    #[test]
    fn any_slot_can_be_selected_including_empty_ones() {
        let mut hotbar = Hotbar::new();
        assert!(hotbar.select_slot(8));
        assert_eq!(hotbar.selected_index(), 8);
    }

    #[test]
    fn out_of_range_slots_are_rejected() {
        let mut hotbar = Hotbar::new();
        assert!(!hotbar.select_slot(HOTBAR_SLOTS));
        assert!(!hotbar.select_slot(usize::MAX));
        assert_eq!(hotbar.selected_index(), 0);
    }

    #[test]
    fn saved_selection_is_clamped() {
        let mut hotbar = Hotbar::new();
        hotbar.set_selected(usize::MAX);
        assert_eq!(hotbar.selected_index(), HOTBAR_SLOTS - 1);
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
