//! 实体定义注册表：类型 ID → 静态定义（尺寸、生命、移速、掉落、生成权重、外观）。
//!
//! 定义是只读常量，实例状态全部保存在 [`crate::entity::Entity`] 中；
//! 渲染、AI、生成等模块只通过本注册表查询配置，避免把数值硬编码在逻辑里。

use glam::Vec3;

use super::creature_registry;
use super::entity::EntityTypeId;
use crate::item::ItemId;

/// 一条实体掉落定义。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemDrop {
    /// 掉落物品种类。
    pub item: ItemId,
    /// 最少数量。
    pub min: u32,
    /// 最多数量。
    pub max: u32,
}

impl ItemDrop {
    /// 基于实体 ID 的稳定数量，保证同一实体死亡结算重复调用时数量一致。
    pub fn count_for(&self, id: u64) -> u32 {
        if self.max <= self.min {
            return self.min;
        }
        self.min + (id % (self.max - self.min + 1) as u64) as u32
    }
}

/// 实体使用的模型类型，渲染模块据此选择盒状拼装形状。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatureModel {
    /// 猪。
    Pig,
    /// 牛。
    Cow,
}

/// 实体类型定义。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityDefinition {
    /// 类型标识。
    pub kind: EntityTypeId,
    /// 展示名称，用于日志与调试。
    pub name: &'static str,
    /// 碰撞箱宽度。
    pub width: f32,
    /// 碰撞箱高度。
    pub height: f32,
    /// 生命上限。
    pub max_health: u32,
    /// 水平移动速度（格/秒）。
    pub move_speed: f32,
    /// 死亡掉落。
    pub drops: &'static [ItemDrop],
    /// 自然生成权重，0 表示不参与自然生成。
    pub spawn_weight: u32,
    /// 主体颜色。
    pub body_color: Vec3,
    /// 次要颜色（斑点、口鼻、角等）。
    pub accent_color: Vec3,
    /// 盒状模型类型。
    pub model: CreatureModel,
}

impl EntityDefinition {
    /// 碰撞尺寸。
    pub fn dimensions(&self) -> super::entity::EntityDimensions {
        super::entity::EntityDimensions::new(self.width, self.height)
    }
}

const DEFINITIONS: [EntityDefinition; 2] = [creature_registry::PIG, creature_registry::COW];

/// 所有已注册实体定义。
pub fn all() -> &'static [EntityDefinition] {
    &DEFINITIONS
}

/// 按类型查询定义；未注册类型返回 `None`。
pub fn definition(kind: EntityTypeId) -> Option<&'static EntityDefinition> {
    DEFINITIONS
        .iter()
        .find(|definition| definition.kind == kind)
}

/// 类型是否合法。
#[allow(dead_code)]
pub fn is_registered(kind: EntityTypeId) -> bool {
    definition(kind).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::item_registry;

    #[test]
    fn every_definition_is_self_consistent() {
        assert!(!all().is_empty());
        for def in all() {
            assert!(is_registered(def.kind));
            assert_eq!(definition(def.kind), Some(def));
            assert!(def.width > 0.0 && def.height > 0.0);
            assert!(def.max_health > 0);
            assert!(def.move_speed > 0.0);
            assert!(!def.name.is_empty());
        }
    }

    #[test]
    fn kinds_are_unique() {
        for (index, def) in all().iter().enumerate() {
            for other in all().iter().skip(index + 1) {
                assert_ne!(def.kind, other.kind, "实体类型 ID 不能重复");
            }
        }
    }

    #[test]
    fn drops_reference_valid_items() {
        for def in all() {
            for drop in def.drops {
                assert!(drop.item.is_valid(), "掉落必须是合法物品");
                assert!(drop.max >= drop.min, "掉落数量上限不能小于下限");
                assert!(item_registry::definition(drop.item).is_some());
                let first = drop.count_for(0);
                let second = drop.count_for(0);
                assert_eq!(first, second, "同一实体数量必须稳定");
                assert!(first >= drop.min && first <= drop.max);
            }
        }
    }

    #[test]
    fn unregistered_kind_rejected() {
        assert!(definition(EntityTypeId::new(999)).is_none());
    }
}
