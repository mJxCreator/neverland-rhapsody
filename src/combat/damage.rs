//! 统一伤害类型与护甲减免入口。
//!
//! 玩家与生物都通过同一套伤害类型描述伤害来源；是否经过护甲由类型决定，
//! 坠落与环境伤害永远绕过护甲。

use crate::equipment::reduce_damage;

/// 玩家受击后的无敌帧时长（秒）；生物使用同一数值。
pub const HURT_INVULNERABILITY: f32 = 0.5;

/// 伤害类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageKind {
    /// 近战武器/生物攻击。
    Melee,
    /// 坠落；当前由生存系统的跌落结算直接处理，保留在统一类型中。
    #[allow(dead_code)]
    Fall,
    /// 环境伤害；当前版本尚无环境伤害来源，预留。
    #[allow(dead_code)]
    Environmental,
    /// 投射物；当前版本尚无投射物，预留。
    #[allow(dead_code)]
    Projectile,
}

impl DamageKind {
    /// 是否经过护甲减免。
    pub const fn affected_by_armor(self) -> bool {
        matches!(self, Self::Melee | Self::Projectile)
    }
}

/// 按伤害类型与防御力折算最终伤害。
pub fn mitigate(amount: u32, kind: DamageKind, defense: u32) -> u32 {
    if kind.affected_by_armor() {
        reduce_damage(amount, defense)
    } else {
        amount
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_physical_damage_is_reduced_by_armor() {
        assert_eq!(mitigate(10, DamageKind::Melee, 5), 8);
        assert_eq!(mitigate(10, DamageKind::Fall, 5), 10);
        assert_eq!(mitigate(10, DamageKind::Environmental, 10), 10);
        assert_eq!(mitigate(10, DamageKind::Projectile, 5), 8);
    }

    #[test]
    fn melee_damage_never_drops_to_zero() {
        assert_eq!(mitigate(1, DamageKind::Melee, 20), 1);
    }
}
