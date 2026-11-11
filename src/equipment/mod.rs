//! 装备系统：护甲槽位、护甲注册表、减伤计算与装备实例管理。

pub mod armor;
#[allow(clippy::module_inception)]
pub mod equipment;
pub mod equipment_registry;

pub use armor::reduce_damage;
pub use equipment::{Equipment, EquipmentSlot, SavedEquipment, EQUIPMENT_SLOT_COUNT};
