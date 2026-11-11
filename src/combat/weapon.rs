//! 武器静态数据与运行时攻击参数。
//!
//! 武器定义集中登记剑类物品；损坏的剑按空手参数处理，但不会从物品栏消失。

use crate::item::{ItemId, ItemStack};

/// 武器种类；当前只有剑。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponType {
    Sword,
}

impl WeaponType {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sword => "SWORD",
        }
    }
}

/// 一件武器的静态属性。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponDefinition {
    pub id: ItemId,
    pub name: &'static str,
    pub weapon_type: WeaponType,
    /// 基础伤害（1 点 = 半颗心）。
    pub damage: u32,
    /// 两次攻击之间的完整间隔（秒）。
    pub attack_interval: f32,
    /// 有效攻击距离（格）。
    pub range: f32,
    /// 攻击锥角（度）。
    pub angle_deg: f32,
    /// 命中击退速度（格/秒）。
    pub knockback: f32,
    /// 最大耐久。
    pub max_durability: u32,
}

impl WeaponDefinition {
    pub const fn new(
        id: ItemId,
        name: &'static str,
        damage: u32,
        attack_interval: f32,
        max_durability: u32,
    ) -> Self {
        Self {
            id,
            name,
            weapon_type: WeaponType::Sword,
            damage,
            attack_interval,
            range: 3.0,
            angle_deg: 120.0,
            knockback: 4.5,
            max_durability,
        }
    }
}

/// 空手攻击参数：伤害低、间隔长、无耐久。
pub const UNARMED_DAMAGE: u32 = 1;
/// 空手攻击间隔（秒）。
pub const UNARMED_INTERVAL: f32 = 1.0;
/// 空手攻击距离（格）。
pub const UNARMED_RANGE: f32 = 3.0;
/// 空手攻击锥角（度）。
pub const UNARMED_ANGLE: f32 = 120.0;
/// 空手命中击退速度（格/秒）。
pub const UNARMED_KNOCKBACK: f32 = 3.0;

/// 一次攻击实际使用的参数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponStats {
    pub damage: u32,
    pub attack_interval: f32,
    pub range: f32,
    pub angle_deg: f32,
    pub knockback: f32,
    /// 本次攻击是否来自可损坏的武器；空手为 `false`。
    pub weapon: Option<ItemId>,
    pub weapon_type: Option<WeaponType>,
}

impl WeaponStats {
    /// 空手参数。
    pub const fn unarmed() -> Self {
        Self {
            damage: UNARMED_DAMAGE,
            attack_interval: UNARMED_INTERVAL,
            range: UNARMED_RANGE,
            angle_deg: UNARMED_ANGLE,
            knockback: UNARMED_KNOCKBACK,
            weapon: None,
            weapon_type: None,
        }
    }

    /// 由当前手持物品解析参数；损坏武器与普通物品按空手处理。
    pub fn for_stack(stack: Option<ItemStack>) -> Self {
        let Some(stack) = stack else {
            return Self::unarmed();
        };
        let Some(definition) = crate::combat::weapon_registry::definition(stack.item()) else {
            return Self::unarmed();
        };
        if stack.is_broken() {
            return Self::unarmed();
        }
        Self {
            damage: definition.damage,
            attack_interval: definition.attack_interval,
            range: definition.range,
            angle_deg: definition.angle_deg,
            knockback: definition.knockback,
            weapon: Some(definition.id),
            weapon_type: Some(definition.weapon_type),
        }
    }

    pub fn is_armed(self) -> bool {
        self.weapon.is_some()
    }

    /// 消耗 1 点武器耐久；空手或已损坏时原样返回。
    pub fn wear(self, stack: ItemStack) -> ItemStack {
        if self.weapon != Some(stack.item()) {
            return stack;
        }
        match stack.durability() {
            Some(current) if current > 0 => stack.with_durability(current - 1),
            _ => stack,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_and_broken_weapons_fall_back_to_unarmed() {
        assert_eq!(WeaponStats::for_stack(None), WeaponStats::unarmed());
        let broken = ItemStack::new(ItemId::IRON_SWORD, 1)
            .expect("剑合法")
            .with_durability(0);
        assert_eq!(WeaponStats::for_stack(Some(broken)), WeaponStats::unarmed());
        let material = ItemStack::new(ItemId::STONE, 1).expect("方块合法");
        assert_eq!(
            WeaponStats::for_stack(Some(material)),
            WeaponStats::unarmed()
        );
    }

    #[test]
    fn intact_sword_uses_registry_values() {
        let sword = ItemStack::new(ItemId::IRON_SWORD, 1).expect("剑合法");
        let stats = WeaponStats::for_stack(Some(sword));
        assert_eq!(stats.damage, 6);
        assert_eq!(stats.range, 3.0);
        assert_eq!(stats.angle_deg, 120.0);
        assert!(stats.is_armed());
    }

    #[test]
    fn wear_only_affects_the_matching_weapon() {
        let sword = ItemStack::new(ItemId::WOODEN_SWORD, 1).expect("剑合法");
        let stats = WeaponStats::for_stack(Some(sword));
        assert_eq!(stats.wear(sword).durability(), Some(59));

        let pickaxe = ItemStack::new(ItemId::WOODEN_PICKAXE, 1).expect("镐合法");
        assert_eq!(stats.wear(pickaxe).durability(), Some(60));
    }
}
