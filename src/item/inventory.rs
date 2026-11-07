//! 玩家背包：27 格主背包 + 9 格快捷栏，统一管理物品槽位、堆叠、添加与移除。
//!
//! 所有数量变化都必须经过本模块接口，保证不会出现负数、超过堆叠上限或无效物品 ID。
//! 背包空间不足时，未容纳的数量通过返回值交给调用方（掉落物拾取）继续保留。

use serde::{Deserialize, Serialize};

use super::item::{ItemId, ItemStack};
use super::item_registry::max_stack_size;
use super::tool::{self, Wear};

/// 主背包槽位数。
pub const MAIN_SLOTS: usize = 27;
/// 快捷栏槽位数；快捷栏是背包的最后 9 个槽位。
pub const HOTBAR_SLOTS: usize = 9;
/// 背包总槽位数。
pub const TOTAL_SLOTS: usize = MAIN_SLOTS + HOTBAR_SLOTS;
/// 快捷栏在背包数组中的起始下标。
pub const HOTBAR_OFFSET: usize = MAIN_SLOTS;

/// 一次添加物品的结果。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AddOutcome {
    /// 成功放入背包的数量。
    pub added: u32,
    /// 未能容纳、仍需保留的数量。
    pub remaining: u32,
}

/// 存档中的单个物品槽位；空槽位在外层用 `None` 表示。
///
/// `durability` 是工具实例的剩余耐久：普通物品为 `None`，缺失时按
/// “满耐久”兼容旧存档。该字段带 `serde(default)`，v1–v3 存档可直接读取。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedSlot {
    pub item: u16,
    pub count: u32,
    #[serde(default)]
    pub durability: Option<u32>,
}

/// 存档中的玩家背包与快捷栏选择。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedInventory {
    #[serde(default)]
    pub slots: Vec<Option<SavedSlot>>,
    #[serde(default)]
    pub selected_hotbar: usize,
}

/// 36 格玩家背包；前 27 格为主背包，后 9 格为快捷栏。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Inventory {
    slots: [Option<ItemStack>; TOTAL_SLOTS],
}

impl Default for Inventory {
    fn default() -> Self {
        Self {
            slots: [None; TOTAL_SLOTS],
        }
    }
}

impl Inventory {
    /// 创建全部为空的背包。
    pub fn new() -> Self {
        Self::default()
    }

    /// 读取指定槽位；越界或空槽返回 `None`。
    pub fn slot(&self, index: usize) -> Option<ItemStack> {
        self.slots.get(index).copied().flatten()
    }

    /// 写入指定槽位并返回原有物品；数量为 0 的堆叠按空槽处理。
    pub fn set_slot(&mut self, index: usize, stack: Option<ItemStack>) -> Option<ItemStack> {
        let Some(slot) = self.slots.get_mut(index) else {
            return stack;
        };
        let stack = stack.filter(|stack| stack.item().is_valid() && stack.count() > 0);
        std::mem::replace(slot, stack)
    }

    /// 取出指定槽位的全部物品。
    pub fn take_slot(&mut self, index: usize) -> Option<ItemStack> {
        self.set_slot(index, None)
    }

    /// 交换两个槽位的物品；越界时不做任何修改。
    ///
    /// 背包界面在交换不同物品时使用等价语义；该接口保留给后续拖拽整理。
    #[allow(dead_code)]
    pub fn swap_slots(&mut self, first: usize, second: usize) -> bool {
        if first >= TOTAL_SLOTS || second >= TOTAL_SLOTS {
            return false;
        }
        self.slots.swap(first, second);
        true
    }

    /// 把一份物品堆叠放入指定槽位，返回无法放入的剩余部分。
    ///
    /// 相同物品会先合并到槽位现有堆叠，其他物品不接受合并。
    pub fn insert_stack(&mut self, index: usize, stack: ItemStack) -> Option<ItemStack> {
        let Some(slot) = self.slots.get_mut(index) else {
            return Some(stack);
        };
        match *slot {
            None => {
                *slot = Some(stack);
                None
            }
            Some(existing) if existing.can_merge_with(stack) => {
                let moved = existing.room().min(stack.count());
                *slot = existing.with_count(existing.count() + moved);
                stack.with_count(stack.count() - moved)
            }
            Some(_) => Some(stack),
        }
    }

    /// 向指定槽位加入一个物品；槽位为空、物品不同或堆叠已满时返回 `false`。
    pub fn add_one(&mut self, index: usize) -> bool {
        let Some(slot) = self.slots.get_mut(index) else {
            return false;
        };
        let Some(existing) = *slot else {
            return false;
        };
        if existing.room() == 0 {
            return false;
        }
        *slot = existing.with_count(existing.count() + 1);
        true
    }

    /// 从指定槽位分出一半（向上取整）的物品，剩余部分留在原槽位。
    ///
    /// 使用 [`ItemStack::with_count`] 保留耐久，避免拆分工具时耐久被重置。
    pub fn split_half(&mut self, index: usize) -> Option<ItemStack> {
        let stack = self.slots.get(index).copied().flatten()?;
        let taken = stack.count().div_ceil(2);
        *self.slots.get_mut(index)? = stack.with_count(stack.count() - taken);
        stack.with_count(taken)
    }

    /// 向背包添加物品：先合并同类堆叠，再填充空槽位，超过上限自动拆分。
    ///
    /// 返回成功数量与剩余数量；剩余数量不会被丢弃，由调用方保留。
    pub fn add_item(&mut self, item: ItemId, count: u32) -> AddOutcome {
        let Some(max) = max_stack_size(item) else {
            return AddOutcome {
                added: 0,
                remaining: count,
            };
        };
        if count == 0 {
            return AddOutcome::default();
        }

        let mut remaining = count;

        // 优先填充已有的同类物品堆叠。
        for slot in &mut self.slots {
            if remaining == 0 {
                break;
            }
            let Some(stack) = slot else {
                continue;
            };
            if stack.item() != item {
                continue;
            }
            let moved = stack.room().min(remaining);
            if moved > 0 {
                *slot = stack.with_count(stack.count() + moved);
                remaining -= moved;
            }
        }

        // 再填充空槽位：优先 1→9 号快捷栏，随后主背包；单格放不下的继续拆分。
        for index in (HOTBAR_OFFSET..TOTAL_SLOTS).chain(0..HOTBAR_OFFSET) {
            if remaining == 0 {
                break;
            }
            if self.slots[index].is_some() {
                continue;
            }
            let filled = max.min(remaining);
            self.slots[index] = ItemStack::new(item, filled);
            remaining -= filled;
        }

        AddOutcome {
            added: count - remaining,
            remaining,
        }
    }

    /// 把一整组物品放入背包，保留工具实例耐久；返回未能放入的剩余堆叠。
    ///
    /// 与 [`Inventory::add_item`] 的区别是本接口接收完整堆叠，因此不会把工具的
    /// 剩余耐久重置为满耐久；背包界面关闭、拾取回滚等场景必须使用本接口。
    pub fn add_stack(&mut self, stack: ItemStack) -> Option<ItemStack> {
        if !stack.item().is_valid() || stack.count() == 0 {
            return Some(stack);
        }

        let mut remaining = stack;

        // 优先合并到相同物品、相同耐久的已有堆叠。
        for index in 0..TOTAL_SLOTS {
            if remaining.count() == 0 {
                return None;
            }
            let Some(existing) = self.slots[index] else {
                continue;
            };
            if !existing.can_merge_with(remaining) {
                continue;
            }
            let moved = existing.room().min(remaining.count());
            if moved > 0 {
                self.slots[index] = existing.with_count(existing.count() + moved);
                let left = remaining.count() - moved;
                if left == 0 {
                    return None;
                }
                remaining = remaining.with_count(left)?;
            }
        }

        // 再放入空槽位：优先 1→9 号快捷栏，随后主背包。
        for index in (HOTBAR_OFFSET..TOTAL_SLOTS).chain(0..HOTBAR_OFFSET) {
            if self.slots[index].is_none() {
                self.slots[index] = Some(remaining);
                return None;
            }
        }

        Some(remaining)
    }

    /// 从背包移除指定数量的物品，返回实际移除数量。
    pub fn remove_item(&mut self, item: ItemId, count: u32) -> u32 {
        let mut remaining = count;
        for slot in &mut self.slots {
            if remaining == 0 {
                break;
            }
            let Some(stack) = *slot else {
                continue;
            };
            if stack.item() != item {
                continue;
            }
            let taken = stack.count().min(remaining);
            *slot = stack.with_count(stack.count() - taken);
            remaining -= taken;
        }
        count - remaining
    }

    /// 背包中指定物品的总数量；供统计与后续合成系统使用。
    #[allow(dead_code)]
    pub fn count_of(&self, item: ItemId) -> u32 {
        self.slots
            .iter()
            .flatten()
            .filter(|stack| stack.item() == item)
            .map(|stack| stack.count())
            .sum()
    }

    /// 快捷栏某个槽位的物品。
    pub fn hotbar_stack(&self, index: usize) -> Option<ItemStack> {
        if index >= HOTBAR_SLOTS {
            return None;
        }
        self.slot(HOTBAR_OFFSET + index)
    }

    /// 快捷栏 9 个槽位的快照，供 HUD 显示。
    pub fn hotbar_slots(&self) -> [Option<ItemStack>; HOTBAR_SLOTS] {
        std::array::from_fn(|index| self.slots[HOTBAR_OFFSET + index])
    }

    /// 是否所有槽位都为空。
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    /// 对指定槽位的工具施加耐久消耗；普通物品不受影响。
    ///
    /// 所有工具耐久变化都必须经过该接口或 [`super::tool::apply_wear`]，
    /// 避免多个系统各自扣减导致同一次挖掘被重复结算。
    pub fn damage_slot(&mut self, index: usize, amount: u32) -> Wear {
        let Some(stack) = self.slots.get(index).copied().flatten() else {
            return Wear::NotATool;
        };
        let mut damaged = stack;
        let wear = tool::apply_wear(&mut damaged, amount);
        if wear != Wear::NotATool {
            self.slots[index] = Some(damaged);
        }
        wear
    }

    /// 对快捷栏指定下标（0..9）施加工具耐久消耗。
    pub fn damage_hotbar_tool(&mut self, hotbar_index: usize, amount: u32) -> Wear {
        if hotbar_index >= HOTBAR_SLOTS {
            return Wear::NotATool;
        }
        self.damage_slot(HOTBAR_OFFSET + hotbar_index, amount)
    }

    /// 导出为存档结构。
    pub fn to_saved(self, selected_hotbar: usize) -> SavedInventory {
        SavedInventory {
            slots: self
                .slots
                .iter()
                .map(|slot| {
                    slot.map(|stack| SavedSlot {
                        item: stack.item().raw(),
                        count: stack.count(),
                        durability: stack.durability(),
                    })
                })
                .collect(),
            selected_hotbar: selected_hotbar.min(HOTBAR_SLOTS - 1),
        }
    }

    /// 从存档结构恢复背包；无效物品 ID、非法数量与越界槽位会被忽略。
    ///
    /// 工具缺少耐久字段（v1–v3 旧档或普通物品）时按满耐久恢复；
    /// 超出上限的耐久会被限制到工具定义的合法范围。
    pub fn from_saved(saved: &SavedInventory) -> (Self, usize) {
        let mut inventory = Self::default();
        for (index, slot) in saved.slots.iter().take(TOTAL_SLOTS).enumerate() {
            let Some(entry) = slot else {
                continue;
            };
            let Some(item) = ItemId::from_raw(entry.item) else {
                continue;
            };
            let mut stack = ItemStack::new(item, entry.count);
            if let Some(durability) = entry.durability {
                stack = stack.map(|restored| {
                    if restored.is_tool() {
                        restored.with_durability(durability)
                    } else {
                        restored
                    }
                });
            }
            inventory.slots[index] = stack;
        }
        (inventory, saved.selected_hotbar.min(HOTBAR_SLOTS - 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_inventory_has_36_empty_slots() {
        let inventory = Inventory::new();
        assert_eq!(TOTAL_SLOTS, 36);
        for index in 0..TOTAL_SLOTS {
            assert_eq!(inventory.slot(index), None);
        }
        assert!(inventory.is_empty());
    }

    #[test]
    fn adding_prefers_existing_stacks_then_hotbar_slots() {
        let mut inventory = Inventory::new();
        inventory.set_slot(
            3,
            Some(ItemStack::new(ItemId::STONE, 10).expect("合法堆叠")),
        );
        let outcome = inventory.add_item(ItemId::STONE, 60);
        assert_eq!(outcome.added, 60);
        assert_eq!(outcome.remaining, 0);
        assert_eq!(inventory.slot(3).map(ItemStack::count), Some(64));
        // 64 已在第 3 格，剩余 6 优先放入 1 号快捷栏槽位。
        assert_eq!(inventory.slot(HOTBAR_OFFSET).map(ItemStack::count), Some(6));
        assert_eq!(inventory.count_of(ItemId::STONE), 60 + 10);
    }

    #[test]
    fn empty_hotbar_slots_fill_from_left_to_right_before_main_inventory() {
        let mut inventory = Inventory::new();
        for offset in 0..HOTBAR_SLOTS {
            // 把当前快捷栏槽位放满，下一件物品应进入右侧相邻槽位。
            inventory.set_slot(
                HOTBAR_OFFSET + offset,
                Some(ItemStack::new(ItemId::STONE, 64).expect("合法堆叠")),
            );
            if offset + 1 < HOTBAR_SLOTS {
                inventory.add_item(ItemId::STONE, 1);
                assert_eq!(
                    inventory
                        .slot(HOTBAR_OFFSET + offset + 1)
                        .map(ItemStack::count),
                    Some(1),
                    "第 {} 个快捷栏槽位应先于主背包被填充",
                    offset + 2
                );
            }
        }

        // 快捷栏全部占满后才回落到主背包第一格。
        inventory.add_item(ItemId::STONE, 1);
        assert_eq!(inventory.slot(0).map(ItemStack::count), Some(1));
    }

    #[test]
    fn overflowing_amounts_split_across_slots() {
        let mut inventory = Inventory::new();
        let outcome = inventory.add_item(ItemId::DIRT, 130);
        assert_eq!(outcome.added, 130);
        assert_eq!(outcome.remaining, 0);
        assert_eq!(
            inventory.slot(HOTBAR_OFFSET).map(ItemStack::count),
            Some(64)
        );
        assert_eq!(
            inventory.slot(HOTBAR_OFFSET + 1).map(ItemStack::count),
            Some(64)
        );
        assert_eq!(
            inventory.slot(HOTBAR_OFFSET + 2).map(ItemStack::count),
            Some(2)
        );
    }

    #[test]
    fn a_full_inventory_keeps_the_remainder() {
        let mut inventory = Inventory::new();
        for index in 0..TOTAL_SLOTS {
            inventory.set_slot(
                index,
                Some(ItemStack::new(ItemId::STONE, 64).expect("合法堆叠")),
            );
        }
        let outcome = inventory.add_item(ItemId::STONE, 5);
        assert_eq!(outcome.added, 0);
        assert_eq!(outcome.remaining, 5, "剩余物品不能丢失");
    }

    #[test]
    fn different_items_never_share_a_slot() {
        let mut inventory = Inventory::new();
        inventory.add_item(ItemId::STONE, 10);
        inventory.add_item(ItemId::DIRT, 10);
        assert_eq!(
            inventory.slot(HOTBAR_OFFSET).map(ItemStack::item),
            Some(ItemId::STONE)
        );
        assert_eq!(
            inventory.slot(HOTBAR_OFFSET + 1).map(ItemStack::item),
            Some(ItemId::DIRT)
        );
        let leftover =
            inventory.insert_stack(HOTBAR_OFFSET, ItemStack::new(ItemId::DIRT, 3).unwrap());
        assert_eq!(leftover.map(ItemStack::item), Some(ItemId::DIRT));
    }

    #[test]
    fn removing_items_clears_empty_stacks() {
        let mut inventory = Inventory::new();
        inventory.add_item(ItemId::GRASS, 10);
        assert_eq!(inventory.remove_item(ItemId::GRASS, 10), 10);
        assert_eq!(inventory.slot(HOTBAR_OFFSET), None);
        assert!(inventory.is_empty());
        assert_eq!(inventory.remove_item(ItemId::GRASS, 3), 0);
    }

    #[test]
    fn slots_can_be_swapped_and_split() {
        let mut inventory = Inventory::new();
        inventory.set_slot(0, Some(ItemStack::new(ItemId::STONE, 7).expect("合法堆叠")));
        assert!(inventory.swap_slots(0, 27));
        assert_eq!(inventory.hotbar_stack(0).map(ItemStack::count), Some(7));

        let half = inventory.split_half(27).expect("应有可分出的物品");
        assert_eq!(half.count(), 4);
        assert_eq!(inventory.slot(27).map(ItemStack::count), Some(3));
        assert!(!inventory.swap_slots(0, TOTAL_SLOTS));
    }

    #[test]
    fn invalid_items_and_zero_counts_are_rejected() {
        let mut inventory = Inventory::new();
        let outcome = inventory.add_item(ItemId::INVALID, 5);
        assert_eq!(outcome.added, 0);
        assert_eq!(outcome.remaining, 5);
        assert!(inventory
            .set_slot(0, ItemStack::new(ItemId::STONE, 0))
            .is_none());
        assert_eq!(inventory.slot(0), None);

        // 存档中的非法数据必须被忽略。
        let saved = SavedInventory {
            slots: vec![
                Some(SavedSlot {
                    item: 1,
                    count: 0,
                    durability: None,
                }),
                Some(SavedSlot {
                    item: 99,
                    count: 1,
                    durability: None,
                }),
                Some(SavedSlot {
                    item: 2,
                    count: 5,
                    durability: None,
                }),
            ],
            selected_hotbar: 100,
        };
        let (restored, selected) = Inventory::from_saved(&saved);
        assert_eq!(restored.slot(0), None);
        assert_eq!(restored.slot(1), None);
        assert_eq!(restored.slot(2).map(ItemStack::count), Some(5));
        assert_eq!(selected, HOTBAR_SLOTS - 1);
    }

    #[test]
    fn saved_inventories_round_trip() {
        let mut inventory = Inventory::new();
        inventory.add_item(ItemId::STONE, 20);
        inventory.add_item(ItemId::GRASS, 3);
        let saved = inventory.to_saved(4);
        let (restored, selected) = Inventory::from_saved(&saved);
        assert_eq!(restored, inventory);
        assert_eq!(selected, 4);
    }

    #[test]
    fn tool_durability_survives_slot_moves_and_saving() {
        let mut inventory = Inventory::new();
        let pickaxe = ItemStack::new(ItemId::IRON_PICKAXE, 1)
            .expect("工具堆叠合法")
            .with_durability(77);
        inventory.set_slot(HOTBAR_OFFSET + 2, Some(pickaxe));
        assert!(inventory.swap_slots(HOTBAR_OFFSET + 2, 0));
        assert_eq!(inventory.slot(0).and_then(ItemStack::durability), Some(77));

        let saved = inventory.to_saved(2);
        let (restored, selected) = Inventory::from_saved(&saved);
        assert_eq!(restored, inventory);
        assert_eq!(selected, 2);
        assert_eq!(restored.slot(0).and_then(ItemStack::durability), Some(77));
    }

    #[test]
    fn old_saves_restore_tools_at_full_durability() {
        let saved = SavedInventory {
            slots: vec![Some(SavedSlot {
                item: ItemId::IRON_PICKAXE.raw(),
                count: 1,
                durability: None,
            })],
            selected_hotbar: 0,
        };
        let (restored, _) = Inventory::from_saved(&saved);
        assert_eq!(
            restored.slot(0).and_then(ItemStack::durability),
            Some(251),
            "缺失耐久字段的旧档按满耐久恢复"
        );
    }

    #[test]
    fn saved_durability_is_clamped_to_the_tool_maximum() {
        let saved = SavedInventory {
            slots: vec![Some(SavedSlot {
                item: ItemId::WOODEN_PICKAXE.raw(),
                count: 1,
                durability: Some(999),
            })],
            selected_hotbar: 0,
        };
        let (restored, _) = Inventory::from_saved(&saved);
        assert_eq!(restored.slot(0).and_then(ItemStack::durability), Some(60));
    }

    #[test]
    fn durability_damage_only_touches_tools() {
        let mut inventory = Inventory::new();
        inventory.set_slot(
            HOTBAR_OFFSET,
            Some(ItemStack::new(ItemId::WOODEN_PICKAXE, 1).expect("工具堆叠合法")),
        );
        inventory.set_slot(
            HOTBAR_OFFSET + 1,
            Some(ItemStack::new(ItemId::STONE, 3).expect("合法堆叠")),
        );

        assert_eq!(
            inventory.damage_hotbar_tool(0, 1),
            Wear::Damaged { remaining: 59 }
        );
        assert_eq!(inventory.damage_hotbar_tool(1, 1), Wear::NotATool);
        assert_eq!(
            inventory.slot(HOTBAR_OFFSET + 1).map(ItemStack::count),
            Some(3)
        );
        assert_eq!(inventory.damage_hotbar_tool(9, 1), Wear::NotATool);
    }

    #[test]
    fn splitting_a_tool_keeps_its_durability() {
        let mut inventory = Inventory::new();
        inventory.set_slot(
            0,
            Some(
                ItemStack::new(ItemId::WOODEN_AXE, 1)
                    .expect("工具堆叠合法")
                    .with_durability(12),
            ),
        );

        let half = inventory.split_half(0).expect("应有可分出的物品");
        assert_eq!(half.durability(), Some(12));
        assert_eq!(inventory.slot(0), None);
    }

    #[test]
    fn add_stack_preserves_tool_durability() {
        let mut inventory = Inventory::new();
        let pickaxe = ItemStack::new(ItemId::IRON_PICKAXE, 1)
            .expect("工具堆叠合法")
            .with_durability(9);

        assert_eq!(inventory.add_stack(pickaxe), None);
        assert_eq!(
            inventory
                .slot(HOTBAR_OFFSET)
                .and_then(ItemStack::durability),
            Some(9)
        );
    }
}
