//! 进食状态机：进食进度、中断与原子化消耗。
//!
//! 只有完成进食时才扣除物品并恢复饥饿值；任何中断都直接丢弃进度，不产生
//! 部分恢复，从而避免界面或输入层自行修改背包。

use super::food_registry::{self, FoodEffect};
use super::hunger::Hunger;
use super::rng::GameRng;
use crate::item::inventory::HOTBAR_OFFSET;
use crate::item::{Inventory, ItemId};

/// 进食状态机的一次更新结果。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EatingUpdate {
    Idle,
    /// 进度比例 `[0, 1)`。
    Progress(f32),
    Completed,
}

/// 当前进食状态。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EatingState {
    item: Option<ItemId>,
    slot: usize,
    elapsed: f32,
    duration: f32,
}

impl Default for EatingState {
    fn default() -> Self {
        Self {
            item: None,
            slot: 0,
            elapsed: 0.0,
            duration: 0.0,
        }
    }
}

impl EatingState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_eating(self) -> bool {
        self.item.is_some()
    }

    #[allow(dead_code)]
    pub fn item(self) -> Option<ItemId> {
        self.item
    }

    #[allow(dead_code)]
    pub fn slot(self) -> Option<usize> {
        self.item.map(|_| self.slot)
    }

    /// 当前进度比例；未进食时为 `None`。
    pub fn progress(self) -> Option<f32> {
        self.item
            .map(|_| (self.elapsed / self.duration).clamp(0.0, 1.0))
    }

    /// 开始进食；不可食用或槽位越界时返回 `false`。
    pub fn start(&mut self, item: ItemId, slot: usize) -> bool {
        if slot >= crate::item::inventory::TOTAL_SLOTS {
            return false;
        }
        let Some(food) = food_registry::definition(item) else {
            return false;
        };
        self.item = Some(item);
        self.slot = slot;
        self.elapsed = 0.0;
        self.duration = food.eat_seconds.max(f32::EPSILON);
        true
    }

    /// 中断进食，不产生任何恢复。
    pub fn cancel(&mut self) {
        self.item = None;
        self.elapsed = 0.0;
        self.duration = 0.0;
    }

    /// 推进进食进度。
    pub fn update(&mut self, delta_seconds: f32) -> EatingUpdate {
        if self.item.is_none() {
            return EatingUpdate::Idle;
        }
        if delta_seconds.is_finite() && delta_seconds > 0.0 {
            self.elapsed += delta_seconds;
        }
        if self.elapsed >= self.duration {
            EatingUpdate::Completed
        } else {
            EatingUpdate::Progress(self.progress().unwrap_or(0.0))
        }
    }

    /// 完成进食：校验槽位内容后扣除 1 个物品、恢复饥饿值并结算特殊效果。
    ///
    /// 槽位内容已变化时返回 `None`，不会恢复任何数值。
    pub fn consume(
        &mut self,
        inventory: &mut Inventory,
        hunger: &mut Hunger,
        rng: &mut GameRng,
    ) -> Option<ItemId> {
        let item = self.item?;
        if self.elapsed < self.duration {
            return None;
        }
        let stack = inventory.slot(self.slot)?;
        if stack.item() != item {
            self.cancel();
            return None;
        }
        let food = food_registry::definition(item)?;
        let remaining = stack.count().saturating_sub(1);
        let _ = inventory.set_slot(self.slot, stack.with_count(remaining));
        hunger.eat(food.hunger, food.saturation);
        if let FoodEffect::Hunger { chance, duration } = food.effect {
            if rng.next_f32() < chance {
                hunger.apply_hunger_effect(duration);
            }
        }
        self.cancel();
        Some(item)
    }

    /// 快捷栏槽位对应的背包绝对下标。
    #[allow(dead_code)]
    pub fn hotbar_slot(index: usize) -> usize {
        HOTBAR_OFFSET + index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::ItemStack;

    fn inventory_with(item: ItemId, count: u32) -> Inventory {
        let mut inventory = Inventory::new();
        let _ = inventory.set_slot(HOTBAR_OFFSET, ItemStack::new(item, count));
        inventory
    }

    fn seed_below(limit: f32) -> u64 {
        (1..100_000)
            .find(|seed| GameRng::new(*seed).next_f32() < limit)
            .expect("应能在小范围内找到满足条件的种子")
    }

    fn seed_at_least(limit: f32) -> u64 {
        (1..100_000)
            .find(|seed| GameRng::new(*seed).next_f32() >= limit)
            .expect("应能在小范围内找到满足条件的种子")
    }

    #[test]
    fn non_food_cannot_start() {
        let mut eating = EatingState::new();
        assert!(!eating.start(ItemId::STONE, 0));
        assert!(!eating.is_eating());
    }

    #[test]
    fn progress_and_completion() {
        let mut eating = EatingState::new();
        assert!(eating.start(ItemId::APPLE, 0));
        assert_eq!(eating.update(0.6), EatingUpdate::Progress(0.5));
        assert_eq!(eating.update(0.6), EatingUpdate::Completed);
    }

    #[test]
    fn cancel_discards_progress() {
        let mut eating = EatingState::new();
        assert!(eating.start(ItemId::APPLE, 0));
        eating.update(0.6);
        eating.cancel();
        assert!(!eating.is_eating());
        let mut inventory = inventory_with(ItemId::APPLE, 2);
        let mut hunger = Hunger::new();
        hunger.drain(10.0);
        let before = hunger.value();
        let mut rng = GameRng::new(1);
        assert_eq!(eating.consume(&mut inventory, &mut hunger, &mut rng), None);
        assert_eq!(hunger.value(), before);
    }

    #[test]
    fn consume_removes_one_and_restores_hunger() {
        let mut eating = EatingState::new();
        assert!(eating.start(ItemId::BREAD, HOTBAR_OFFSET));
        eating.update(2.0);
        let mut inventory = inventory_with(ItemId::BREAD, 2);
        let mut hunger = Hunger::new();
        hunger.drain(10.0);
        let before = hunger.value();
        let mut rng = GameRng::new(1);
        assert_eq!(
            eating.consume(&mut inventory, &mut hunger, &mut rng),
            Some(ItemId::BREAD)
        );
        assert_eq!(inventory.slot(HOTBAR_OFFSET).unwrap().count(), 1);
        assert!((hunger.value() - (before + 5.0)).abs() < 1e-5);
        assert!(!eating.is_eating());
    }

    #[test]
    fn changed_slot_aborts_atomically() {
        let mut eating = EatingState::new();
        assert!(eating.start(ItemId::BREAD, HOTBAR_OFFSET));
        eating.update(2.0);
        let mut inventory = inventory_with(ItemId::APPLE, 2);
        let mut hunger = Hunger::new();
        hunger.drain(10.0);
        let before = hunger.value();
        let mut rng = GameRng::new(1);
        assert_eq!(eating.consume(&mut inventory, &mut hunger, &mut rng), None);
        assert_eq!(hunger.value(), before);
        assert_eq!(inventory.slot(HOTBAR_OFFSET).unwrap().count(), 2);
    }

    #[test]
    fn rotten_flesh_triggers_effect_only_below_chance() {
        let mut inventory = inventory_with(ItemId::ROTTEN_FLESH, 2);
        let mut hunger = Hunger::new();
        hunger.drain(10.0);
        let mut eating = EatingState::new();
        eating.start(ItemId::ROTTEN_FLESH, HOTBAR_OFFSET);
        eating.update(2.0);
        let mut rng = GameRng::new(seed_below(0.2));
        eating.consume(&mut inventory, &mut hunger, &mut rng);
        assert!(hunger.hunger_effect_active());

        let mut inventory = inventory_with(ItemId::ROTTEN_FLESH, 2);
        let mut hunger = Hunger::new();
        hunger.drain(10.0);
        let mut eating = EatingState::new();
        eating.start(ItemId::ROTTEN_FLESH, HOTBAR_OFFSET);
        eating.update(2.0);
        let mut rng = GameRng::new(seed_at_least(0.2));
        eating.consume(&mut inventory, &mut hunger, &mut rng);
        assert!(!hunger.hunger_effect_active());
    }
}
