//! 装备槽位与装备实例。
//!
//! 装备只保存四个护甲槽位中的物品堆叠，不直接操作背包。界面层通过“手持物品”
//! 与装备槽交换，保证装备/卸下操作要么完整发生，要么完全不发生。

use serde::{Deserialize, Serialize};

use crate::item::inventory::SavedSlot;
use crate::item::{ItemId, ItemStack};

use super::equipment_registry;

/// 护甲槽位数量。
pub const EQUIPMENT_SLOT_COUNT: usize = 4;

/// 四个护甲槽位；顺序与界面从上到下一致。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EquipmentSlot {
    Head,
    Chest,
    Legs,
    Feet,
}

impl EquipmentSlot {
    /// 按界面顺序排列的全部槽位。
    pub const ALL: [Self; EQUIPMENT_SLOT_COUNT] = [Self::Head, Self::Chest, Self::Legs, Self::Feet];

    /// 稳定编码，用于存档。
    pub const fn code(self) -> u8 {
        match self {
            Self::Head => 0,
            Self::Chest => 1,
            Self::Legs => 2,
            Self::Feet => 3,
        }
    }

    /// 由存档编码还原槽位；未知编码返回 `None`。
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Head),
            1 => Some(Self::Chest),
            2 => Some(Self::Legs),
            3 => Some(Self::Feet),
            _ => None,
        }
    }

    /// 槽位在装备数组中的下标。
    pub const fn index(self) -> usize {
        self.code() as usize
    }

    /// 展示用名称；字体仅覆盖 ASCII。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Head => "HEAD",
            Self::Chest => "CHEST",
            Self::Legs => "LEGS",
            Self::Feet => "FEET",
        }
    }
}

/// 装备操作失败原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EquipError {
    /// 物品没有护甲定义。
    NotArmor,
    /// 物品不能放进该槽位。
    WrongSlot,
    /// 堆叠数量不为 1。
    NotSingle,
}

/// 存档中的装备槽位；顺序固定为头、胸、腿、脚。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedEquipment {
    #[serde(default)]
    pub slots: Vec<Option<SavedSlot>>,
}

/// 四槽装备实例；每个槽位最多一件护甲。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Equipment {
    slots: [Option<ItemStack>; EQUIPMENT_SLOT_COUNT],
}

impl Equipment {
    /// 读取槽位内容。
    pub fn get(&self, slot: EquipmentSlot) -> Option<ItemStack> {
        self.slots[slot.index()]
    }

    /// 装备一件护甲；成功后返回该槽位原有物品（若有）。
    ///
    /// 校验物品定义、槽位匹配与单件数量，失败时物品原样返回给调用方，
    /// 因此不会出现“物品消失”的中间状态。
    pub fn equip(
        &mut self,
        slot: EquipmentSlot,
        stack: ItemStack,
    ) -> Result<Option<ItemStack>, EquipError> {
        if stack.count() != 1 {
            return Err(EquipError::NotSingle);
        }
        let definition =
            equipment_registry::definition(stack.item()).ok_or(EquipError::NotArmor)?;
        if definition.slot != slot {
            return Err(EquipError::WrongSlot);
        }
        Ok(self.slots[slot.index()].replace(stack))
    }

    /// 卸下槽位物品。
    pub fn unequip(&mut self, slot: EquipmentSlot) -> Option<ItemStack> {
        self.slots[slot.index()].take()
    }

    /// 当前防御力总和；损坏护甲不提供防御。
    pub fn defense(&self) -> u32 {
        self.slots
            .iter()
            .flatten()
            .filter(|stack| !stack.is_broken())
            .filter_map(|stack| equipment_registry::definition(stack.item()))
            .map(|definition| definition.defense)
            .sum()
    }

    /// 对全部未损坏护甲各消耗 1 点耐久；返回耐久实际发生变化的件数。
    pub fn wear_all(&mut self) -> usize {
        let mut worn = 0;
        for stack in self.slots.iter_mut().flatten() {
            let Some(current) = stack.durability() else {
                continue;
            };
            if current == 0 {
                continue;
            }
            *stack = stack.with_durability(current - 1);
            worn += 1;
        }
        worn
    }

    /// 判断某物品是否可装备到指定槽位。
    pub fn accepts(slot: EquipmentSlot, item: ItemId) -> bool {
        equipment_registry::definition(item).is_some_and(|definition| definition.slot == slot)
    }

    /// 导出为存档结构。
    pub fn to_saved(&self) -> SavedEquipment {
        SavedEquipment {
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
        }
    }

    /// 从存档恢复装备。
    ///
    /// 非法物品、数量不为 1 或槽位不匹配的记录会被丢弃，不会写入错误的槽位。
    pub fn from_saved(saved: &SavedEquipment) -> Self {
        let mut equipment = Self::default();
        for (index, slot) in saved.slots.iter().take(EQUIPMENT_SLOT_COUNT).enumerate() {
            let Some(entry) = slot else {
                continue;
            };
            let Some(item) = ItemId::from_raw(entry.item) else {
                continue;
            };
            let Some(slot_kind) = EquipmentSlot::from_code(index as u8) else {
                continue;
            };
            if entry.count != 1 || !Self::accepts(slot_kind, item) {
                continue;
            }
            let Some(stack) = ItemStack::new(item, 1) else {
                continue;
            };
            let stack = match entry.durability {
                Some(durability) => stack.with_durability(durability),
                None => stack,
            };
            equipment.slots[index] = Some(stack);
        }
        equipment
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::ItemId;

    fn armor(item: ItemId) -> ItemStack {
        ItemStack::new(item, 1).expect("护甲单件")
    }

    #[test]
    fn equipment_validates_slot_and_item() {
        let mut equipment = Equipment::default();
        assert_eq!(
            equipment.equip(EquipmentSlot::Head, armor(ItemId::IRON_CHESTPLATE)),
            Err(EquipError::WrongSlot)
        );
        assert_eq!(
            equipment.equip(EquipmentSlot::Head, armor(ItemId::STONE)),
            Err(EquipError::NotArmor)
        );
        assert!(equipment
            .equip(EquipmentSlot::Head, armor(ItemId::IRON_HELMET))
            .is_ok());
    }

    #[test]
    fn equipping_returns_the_previous_item_without_losing_either() {
        let mut equipment = Equipment::default();
        let leather = armor(ItemId::LEATHER_HELMET);
        let iron = armor(ItemId::IRON_HELMET);
        assert_eq!(equipment.equip(EquipmentSlot::Head, leather), Ok(None));
        assert_eq!(
            equipment.equip(EquipmentSlot::Head, iron).expect("可替换"),
            Some(leather)
        );
        assert_eq!(equipment.get(EquipmentSlot::Head), Some(iron));
    }

    #[test]
    fn defense_sums_active_armor_and_ignores_broken_pieces() {
        let mut equipment = Equipment::default();
        equipment
            .equip(EquipmentSlot::Head, armor(ItemId::IRON_HELMET))
            .unwrap();
        equipment
            .equip(EquipmentSlot::Chest, armor(ItemId::IRON_CHESTPLATE))
            .unwrap();
        assert_eq!(equipment.defense(), 8);

        let broken = armor(ItemId::IRON_HELMET).with_durability(0);
        equipment
            .equip(EquipmentSlot::Head, broken)
            .expect("损坏护甲仍可装备");
        assert_eq!(equipment.defense(), 6);
    }

    #[test]
    fn invalid_saved_entries_are_dropped() {
        use crate::item::inventory::SavedSlot;

        let saved = SavedEquipment {
            slots: vec![
                Some(SavedSlot {
                    item: ItemId::STONE.raw(),
                    count: 1,
                    durability: None,
                }),
                Some(SavedSlot {
                    item: ItemId::IRON_HELMET.raw(),
                    count: 1,
                    durability: None,
                }),
                Some(SavedSlot {
                    item: ItemId::IRON_LEGGINGS.raw(),
                    count: 2,
                    durability: None,
                }),
                Some(SavedSlot {
                    item: ItemId::IRON_BOOTS.raw(),
                    count: 1,
                    durability: None,
                }),
            ],
        };
        let equipment = Equipment::from_saved(&saved);
        assert_eq!(equipment.get(EquipmentSlot::Head), None);
        assert_eq!(equipment.get(EquipmentSlot::Chest), None);
        assert_eq!(equipment.get(EquipmentSlot::Legs), None);
        assert!(
            equipment.get(EquipmentSlot::Feet).is_some(),
            "合法条目应保留"
        );
    }

    #[test]
    fn wear_consumes_one_durability_from_each_piece() {
        let mut equipment = Equipment::default();
        equipment
            .equip(
                EquipmentSlot::Head,
                armor(ItemId::DIAMOND_HELMET).with_durability(10),
            )
            .unwrap();
        assert_eq!(equipment.wear_all(), 1);
        assert_eq!(
            equipment
                .get(EquipmentSlot::Head)
                .and_then(ItemStack::durability),
            Some(9)
        );
        assert_eq!(equipment.wear_all(), 1);
    }
}
