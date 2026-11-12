//! 装备可视化层：把 `Equipment` 与手持物品解析为挂载模型。
//!
//! 视觉层只读取既有装备/背包数据，不另建装备状态；每帧重建挂载列表，
//! 装备、卸下、交换或耐久耗尽都会在同一帧反映到角色外观上。

pub mod builtin;
pub mod visual_registry;

use std::collections::HashSet;

use crate::attachment::AttachmentManager;
use crate::equipment::{Equipment, EquipmentSlot};
use crate::item::{ItemId, ItemStack};
use crate::model::ModelRegistry;

pub use self::visual_registry::{definition, is_handheld, DisplayMode};

/// 按当前装备与手持物品重建挂载列表。
///
/// 损坏的护甲不参与展示（与 `Equipment::defense` 的有效性判定一致），
/// 模型缺失时跳过该部件并只告警一次。
pub fn apply_current(
    manager: &mut AttachmentManager,
    equipment: &Equipment,
    held: Option<ItemStack>,
    models: &ModelRegistry,
    warned: &mut HashSet<String>,
) {
    manager.clear();
    if let Some(stack) = held {
        if is_handheld(stack.item()) && !stack.is_broken() {
            install(
                manager,
                stack.item(),
                models,
                warned,
                AttachmentSlot::HeldOnly,
            );
        }
    }
    for slot in EquipmentSlot::ALL {
        let Some(stack) = equipment.get(slot) else {
            continue;
        };
        if stack.is_broken() {
            continue;
        }
        install(manager, stack.item(), models, warned, AttachmentSlot::Full);
    }
}

/// 手持物品只取第一个部位（右手）；护甲取全部部位。
#[derive(Clone, Copy, PartialEq, Eq)]
enum AttachmentSlot {
    HeldOnly,
    Full,
}

fn install(
    manager: &mut AttachmentManager,
    item: ItemId,
    models: &ModelRegistry,
    warned: &mut HashSet<String>,
    mode: AttachmentSlot,
) {
    let Some(definition) = definition(item) else {
        return;
    };
    for (index, part) in definition.parts.iter().enumerate() {
        if mode == AttachmentSlot::HeldOnly && index > 0 {
            break;
        }
        let Some(model) = models.by_name(&part.model) else {
            if warned.insert(part.model.clone()) {
                tracing::warn!(
                    item = ?item,
                    model = %part.model,
                    "装备模型缺失，已跳过该部件；可放置同名 GLB 或等待内置资源注册"
                );
            }
            continue;
        };
        manager.attach(part.attachment, model, part.local);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attachment::AttachmentId;
    use crate::model::ModelRegistry;

    fn models() -> ModelRegistry {
        let mut models = ModelRegistry::new();
        builtin::register(&mut models);
        models
    }

    #[test]
    fn full_iron_armor_and_sword_produce_all_attachments() {
        let models = models();
        let mut equipment = Equipment::default();
        for (slot, item) in [
            (EquipmentSlot::Head, ItemId::IRON_HELMET),
            (EquipmentSlot::Chest, ItemId::IRON_CHESTPLATE),
            (EquipmentSlot::Legs, ItemId::IRON_LEGGINGS),
            (EquipmentSlot::Feet, ItemId::IRON_BOOTS),
        ] {
            equipment
                .equip(slot, ItemStack::new(item, 1).expect("护甲堆叠"))
                .expect("护甲槽位匹配");
        }
        let held = ItemStack::new(ItemId::IRON_SWORD, 1);
        let mut manager = AttachmentManager::new();
        let mut warned = HashSet::new();
        apply_current(&mut manager, &equipment, held, &models, &mut warned);
        assert!(warned.is_empty());
        assert_eq!(manager.len(), 8);
        assert!(manager.get(AttachmentId::RightHand).is_some());
        assert!(manager.get(AttachmentId::Head).is_some());
        assert!(manager.get(AttachmentId::Chest).is_some());
        assert!(manager.get(AttachmentId::Legs).is_some());
        assert!(manager.get(AttachmentId::LeftLeg).is_some());
        assert!(manager.get(AttachmentId::RightLeg).is_some());
        assert!(manager.get(AttachmentId::LeftFoot).is_some());
        assert!(manager.get(AttachmentId::RightFoot).is_some());
    }

    #[test]
    fn unequipping_and_switching_update_immediately() {
        let models = models();
        let mut equipment = Equipment::default();
        equipment
            .equip(
                EquipmentSlot::Head,
                ItemStack::new(ItemId::IRON_HELMET, 1).unwrap(),
            )
            .unwrap();
        let mut manager = AttachmentManager::new();
        let mut warned = HashSet::new();
        apply_current(&mut manager, &equipment, None, &models, &mut warned);
        assert_eq!(manager.len(), 1);

        equipment.unequip(EquipmentSlot::Head);
        apply_current(&mut manager, &equipment, None, &models, &mut warned);
        assert!(manager.is_empty());

        equipment
            .equip(
                EquipmentSlot::Head,
                ItemStack::new(ItemId::DIAMOND_HELMET, 1).unwrap(),
            )
            .unwrap();
        apply_current(&mut manager, &equipment, None, &models, &mut warned);
        let id = models.by_name("diamond_helmet").unwrap();
        assert_eq!(
            manager.get(AttachmentId::Head).map(|item| item.model),
            Some(id)
        );
    }

    #[test]
    fn broken_armor_is_not_displayed() {
        let models = models();
        let mut equipment = Equipment::default();
        let mut stack = ItemStack::new(ItemId::IRON_CHESTPLATE, 1).unwrap();
        stack = stack.with_durability(0);
        equipment.equip(EquipmentSlot::Chest, stack).unwrap();
        let mut manager = AttachmentManager::new();
        let mut warned = HashSet::new();
        apply_current(&mut manager, &equipment, None, &models, &mut warned);
        assert!(manager.is_empty());
    }

    #[test]
    fn held_non_handheld_item_does_not_attach() {
        let models = models();
        let equipment = Equipment::default();
        let held = ItemStack::new(ItemId::STONE, 5);
        let mut manager = AttachmentManager::new();
        let mut warned = HashSet::new();
        apply_current(&mut manager, &equipment, held, &models, &mut warned);
        assert!(manager.is_empty());
    }

    #[test]
    fn missing_model_is_warned_once_and_skipped() {
        let models = ModelRegistry::new();
        let equipment = Equipment::default();
        let held = ItemStack::new(ItemId::IRON_SWORD, 1);
        let mut manager = AttachmentManager::new();
        let mut warned = HashSet::new();
        apply_current(&mut manager, &equipment, held, &models, &mut warned);
        apply_current(&mut manager, &equipment, held, &models, &mut warned);
        assert!(manager.is_empty());
        assert_eq!(warned.len(), 1);
    }
}
