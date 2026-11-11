//! 营火烹饪：生肉到熟肉的连续加工、进度、输入输出与持久化。
//!
//! 每个营火独立保存输入、输出与进度；加工按固定时长逐个转换物品，输出槽满
//! 时暂停但不会丢失输入。营火被破坏时由主循环把整个状态取出并安全掉落。

use std::collections::HashMap;

use glam::IVec3;
use serde::{Deserialize, Serialize};

use crate::item::inventory::SavedSlot;
use crate::item::{ItemId, ItemStack};

/// 单个物品的烹饪时长（秒）。
pub const COOK_SECONDS: f32 = 5.0;

/// 一条烹饪配方。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CookingRecipe {
    pub input: ItemId,
    pub output: ItemId,
}

const RECIPES: [CookingRecipe; 2] = [
    CookingRecipe {
        input: ItemId::RAW_PORKCHOP,
        output: ItemId::COOKED_PORKCHOP,
    },
    CookingRecipe {
        input: ItemId::RAW_BEEF,
        output: ItemId::COOKED_BEEF,
    },
];

/// 查询生肉对应的烹饪配方。
pub fn recipe_for(input: ItemId) -> Option<CookingRecipe> {
    RECIPES.iter().copied().find(|recipe| recipe.input == input)
}

/// 存档中的营火烹饪状态。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedCampfire {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    #[serde(default)]
    pub input: Option<SavedSlot>,
    #[serde(default)]
    pub output: Option<SavedSlot>,
    #[serde(default)]
    pub progress: f32,
}

/// 单个营火的运行时状态。
#[derive(Clone, Debug, PartialEq)]
pub struct CampfireState {
    pos: IVec3,
    input: Option<ItemStack>,
    output: Option<ItemStack>,
    progress: f32,
}

impl CampfireState {
    pub fn empty(pos: IVec3) -> Self {
        Self {
            pos,
            input: None,
            output: None,
            progress: 0.0,
        }
    }

    pub fn from_saved(saved: SavedCampfire) -> Option<Self> {
        let input = saved.input.and_then(stack_from_saved);
        let output = saved.output.and_then(stack_from_saved);
        if let Some(stack) = input {
            recipe_for(stack.item())?;
        }
        let progress = if saved.progress.is_finite() {
            saved.progress.clamp(0.0, COOK_SECONDS)
        } else {
            0.0
        };
        Some(Self {
            pos: IVec3::new(saved.x, saved.y, saved.z),
            input,
            output,
            progress,
        })
    }

    pub fn to_saved(&self) -> SavedCampfire {
        SavedCampfire {
            x: self.pos.x,
            y: self.pos.y,
            z: self.pos.z,
            input: self.input.map(stack_to_saved),
            output: self.output.map(stack_to_saved),
            progress: self.progress,
        }
    }

    pub fn pos(&self) -> IVec3 {
        self.pos
    }

    pub fn input(&self) -> Option<ItemStack> {
        self.input
    }

    pub fn output(&self) -> Option<ItemStack> {
        self.output
    }

    /// 当前进度比例 `[0, 1]`。
    pub fn progress_ratio(&self) -> f32 {
        if self.input.is_none() {
            0.0
        } else {
            (self.progress / COOK_SECONDS).clamp(0.0, 1.0)
        }
    }

    /// 是否正在加工。
    pub fn is_cooking(&self) -> bool {
        self.input.is_some()
    }

    /// 状态是否完全为空；空状态不写入存档。
    pub fn is_empty(&self) -> bool {
        self.input.is_none() && self.output.is_none() && self.progress == 0.0
    }

    /// 向输入槽放入可烹饪物品；返回未能放入的剩余堆叠。
    pub fn add_input(&mut self, stack: ItemStack) -> Option<ItemStack> {
        if recipe_for(stack.item()).is_none() {
            return Some(stack);
        }
        match self.input {
            None => {
                let moved = stack.count().min(stack.max_stack_size());
                self.input = stack.with_count(moved);
                stack.with_count(stack.count() - moved)
            }
            Some(existing) if existing.item() == stack.item() => {
                let moved = existing.room().min(stack.count());
                if moved == 0 {
                    return Some(stack);
                }
                self.input = existing.with_count(existing.count() + moved);
                stack.with_count(stack.count() - moved)
            }
            Some(_) => Some(stack),
        }
    }

    /// 把产出放回输出槽（背包放不下时的回滚）；返回未能放入的剩余堆叠。
    pub fn deposit_output(&mut self, stack: ItemStack) -> Option<ItemStack> {
        match self.output {
            None => {
                self.output = Some(stack);
                None
            }
            Some(existing) if existing.can_merge_with(stack) => {
                let moved = existing.room().min(stack.count());
                if moved == 0 {
                    return Some(stack);
                }
                self.output = existing.with_count(existing.count() + moved);
                stack.with_count(stack.count() - moved)
            }
            Some(_) => Some(stack),
        }
    }

    /// 取出全部产出。
    pub fn take_output(&mut self) -> Option<ItemStack> {
        self.output.take()
    }

    /// 推进烹饪；返回本帧状态是否发生变化。
    pub fn update(&mut self, delta_seconds: f32) -> bool {
        if !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return false;
        }
        let Some(input) = self.input else {
            self.progress = 0.0;
            return false;
        };
        let Some(recipe) = recipe_for(input.item()) else {
            self.progress = 0.0;
            return false;
        };
        if !self.can_store_output(recipe.output) {
            return false;
        }

        self.progress += delta_seconds;
        let mut changed = false;
        while self.progress >= COOK_SECONDS && self.can_store_output(recipe.output) {
            if !self.convert_one(recipe.output) {
                break;
            }
            self.progress -= COOK_SECONDS;
            changed = true;
        }
        if self.input.is_none() {
            self.progress = 0.0;
        } else if !self.can_store_output(recipe.output) {
            // 输出已满：保留当前进度，等待玩家取走产出后继续。
            self.progress = self.progress.min(COOK_SECONDS);
        }
        changed
    }

    fn can_store_output(&self, output: ItemId) -> bool {
        match self.output {
            None => true,
            Some(existing) => existing.item() == output && existing.room() > 0,
        }
    }

    fn convert_one(&mut self, output: ItemId) -> bool {
        let Some(input) = self.input else {
            return false;
        };
        let next_output = match self.output {
            None => ItemStack::new(output, 1),
            Some(existing) => existing.with_count(existing.count() + 1),
        };
        let Some(next_output) = next_output else {
            return false;
        };
        self.input = input.with_count(input.count() - 1);
        self.output = Some(next_output);
        true
    }
}

/// 全部营火的烹饪状态；按营火坐标索引。
#[derive(Debug, Default)]
pub struct CookingRegistry {
    fires: HashMap<IVec3, CampfireState>,
    dirty: bool,
}

impl CookingRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.fires.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.fires.is_empty()
    }

    /// 获取指定营火的状态。
    pub fn state(&self, pos: IVec3) -> Option<&CampfireState> {
        self.fires.get(&pos)
    }

    /// 获取指定营火的可变状态；不存在时返回 `None`。
    pub fn state_mut(&mut self, pos: IVec3) -> Option<&mut CampfireState> {
        self.fires.get_mut(&pos)
    }

    /// 获取或创建指定营火的状态。
    pub fn ensure(&mut self, pos: IVec3) -> &mut CampfireState {
        self.fires
            .entry(pos)
            .or_insert_with(|| CampfireState::empty(pos))
    }

    /// 向指定营火加入输入；返回剩余堆叠。
    #[allow(dead_code)]
    pub fn add_input(&mut self, pos: IVec3, stack: ItemStack) -> Option<ItemStack> {
        let leftover = self.ensure(pos).add_input(stack);
        if leftover != Some(stack) {
            self.dirty = true;
        }
        leftover
    }

    /// 取出指定营火的产出。
    #[allow(dead_code)]
    pub fn take_output(&mut self, pos: IVec3) -> Option<ItemStack> {
        let output = self.ensure(pos).take_output();
        if output.is_some() {
            self.dirty = true;
        }
        output
    }

    /// 背包放不下时把产出放回营火。
    #[allow(dead_code)]
    pub fn deposit_output(&mut self, pos: IVec3, stack: ItemStack) -> Option<ItemStack> {
        self.ensure(pos).deposit_output(stack)
    }

    /// 移除营火并返回其状态；破坏方块时用于安全掉落。
    pub fn remove(&mut self, pos: IVec3) -> Option<CampfireState> {
        let state = self.fires.remove(&pos);
        if state.is_some() {
            self.dirty = true;
        }
        state
    }

    /// 推进全部营火；返回发生进度变化的营火坐标。
    pub fn update(&mut self, delta_seconds: f32) -> Vec<IVec3> {
        let mut changed = Vec::new();
        for (pos, fire) in &mut self.fires {
            if fire.update(delta_seconds) {
                changed.push(*pos);
            }
        }
        if !changed.is_empty() {
            self.dirty = true;
        }
        changed
    }

    /// 是否需要在下一存档写入营火状态。
    pub fn take_dirty(&mut self) -> bool {
        let dirty = self.dirty;
        self.dirty = false;
        dirty
    }

    /// 导出存档数据；空状态不写入。
    pub fn to_saved(&self) -> Vec<SavedCampfire> {
        let mut saved: Vec<_> = self
            .fires
            .values()
            .filter(|fire| !fire.is_empty())
            .map(CampfireState::to_saved)
            .collect();
        saved.sort_by_key(|fire| (fire.x, fire.y, fire.z));
        saved
    }

    /// 从存档恢复；返回成功恢复的营火数量。
    pub fn restore(&mut self, saved: &[SavedCampfire]) -> usize {
        self.fires.clear();
        let mut restored = 0;
        for entry in saved {
            if let Some(state) = CampfireState::from_saved(entry.clone()) {
                self.fires.insert(state.pos(), state);
                restored += 1;
            }
        }
        self.dirty = restored > 0;
        restored
    }
}

fn stack_from_saved(slot: SavedSlot) -> Option<ItemStack> {
    let item = ItemId::from_raw(slot.item)?;
    let stack = ItemStack::new(item, slot.count)?;
    Some(match slot.durability {
        Some(durability) => stack.with_durability(durability),
        None => stack,
    })
}

fn stack_to_saved(stack: ItemStack) -> SavedSlot {
    SavedSlot {
        item: stack.item().raw(),
        count: stack.count(),
        durability: stack.durability(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos() -> IVec3 {
        IVec3::new(1, 2, 3)
    }

    fn raw_pork(count: u32) -> ItemStack {
        ItemStack::new(ItemId::RAW_PORKCHOP, count).unwrap()
    }

    #[test]
    fn recipe_only_accepts_raw_meat() {
        assert_eq!(
            recipe_for(ItemId::RAW_PORKCHOP).map(|recipe| recipe.output),
            Some(ItemId::COOKED_PORKCHOP)
        );
        assert_eq!(
            recipe_for(ItemId::RAW_BEEF).map(|recipe| recipe.output),
            Some(ItemId::COOKED_BEEF)
        );
        assert!(recipe_for(ItemId::APPLE).is_none());
    }

    #[test]
    fn continuous_cooking_converts_one_at_a_time() {
        let mut fire = CampfireState::empty(pos());
        assert!(fire.add_input(raw_pork(2)).is_none());
        assert!(fire.update(COOK_SECONDS));
        assert_eq!(fire.input().unwrap().count(), 1);
        assert_eq!(fire.output().unwrap().count(), 1);
        assert!((fire.progress_ratio() - 0.0).abs() < 1e-5);
        assert!(fire.update(COOK_SECONDS));
        assert!(fire.input().is_none());
        assert_eq!(fire.output().unwrap().count(), 2);
        assert_eq!(fire.progress_ratio(), 0.0);
    }

    #[test]
    fn progress_accumulates_across_frames() {
        let mut fire = CampfireState::empty(pos());
        fire.add_input(raw_pork(1));
        assert!(!fire.update(2.5));
        assert!((fire.progress_ratio() - 0.5).abs() < 1e-5);
        assert!(fire.update(2.5));
        assert_eq!(fire.output().unwrap().count(), 1);
    }

    #[test]
    fn full_output_pauses_without_losing_input() {
        let mut fire = CampfireState::empty(pos());
        fire.output = Some(ItemStack::new(ItemId::COOKED_PORKCHOP, 64).unwrap());
        fire.add_input(raw_pork(1));
        assert!(!fire.update(10.0));
        assert_eq!(fire.input().unwrap().count(), 1);
        assert_eq!(fire.output().unwrap().count(), 64);
    }

    #[test]
    fn incompatible_input_is_rejected() {
        let mut fire = CampfireState::empty(pos());
        assert!(fire
            .add_input(ItemStack::new(ItemId::APPLE, 1).unwrap())
            .is_some());
        assert!(fire.input().is_none());
    }

    #[test]
    fn different_input_is_rejected() {
        let mut fire = CampfireState::empty(pos());
        fire.add_input(raw_pork(1));
        let leftover = fire.add_input(ItemStack::new(ItemId::RAW_BEEF, 1).unwrap());
        assert!(leftover.is_some());
        assert_eq!(fire.input().unwrap().item(), ItemId::RAW_PORKCHOP);
    }

    #[test]
    fn take_output_clears_slot() {
        let mut fire = CampfireState::empty(pos());
        fire.add_input(raw_pork(1));
        fire.update(COOK_SECONDS);
        assert!(fire.take_output().is_some());
        assert!(fire.output().is_none());
    }

    #[test]
    fn registry_tracks_dirty_and_saved_state() {
        let mut registry = CookingRegistry::new();
        registry.add_input(pos(), raw_pork(2));
        assert!(registry.take_dirty());
        registry.update(COOK_SECONDS);
        let saved = registry.to_saved();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].input.as_ref().unwrap().count, 1);
        assert_eq!(saved[0].output.as_ref().unwrap().count, 1);
    }

    #[test]
    fn saved_state_round_trips() {
        let mut fire = CampfireState::empty(pos());
        fire.add_input(raw_pork(2));
        fire.update(2.0);
        let saved = fire.to_saved();
        let restored = CampfireState::from_saved(saved).unwrap();
        assert_eq!(restored.input(), fire.input());
        assert!((restored.progress_ratio() - fire.progress_ratio()).abs() < 1e-5);
    }

    #[test]
    fn invalid_saved_input_is_rejected() {
        let saved = SavedCampfire {
            x: 0,
            y: 0,
            z: 0,
            input: Some(SavedSlot {
                item: ItemId::APPLE.raw(),
                count: 1,
                durability: None,
            }),
            output: None,
            progress: 0.0,
        };
        assert!(CampfireState::from_saved(saved).is_none());
    }

    #[test]
    fn empty_states_are_not_persisted() {
        let mut registry = CookingRegistry::new();
        registry.ensure(pos());
        assert!(registry.to_saved().is_empty());
    }
}
