//! 伤害来源、伤害数值与统一入口。
//!
//! 所有伤害（跌落、环境、实体与自定义）都必须经过 [`apply`]；该入口负责
//! 校验数值、处理无敌帧并返回统一结果，避免各系统各自修改生命值。

use super::survival::{DamageOutcome, Survival};

/// 安全跌落高度：不超过该距离不产生伤害。
pub const SAFE_FALL_DISTANCE: f32 = 3.0;
/// 超过安全高度后每格造成的生命值（1 点 = 半颗心）。
pub const DAMAGE_PER_BLOCK: u32 = 1;

/// 伤害来源；用于死亡界面与后续统计。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageSource {
    Fall,
    /// 环境伤害（例如溺水、岩浆）；当前版本尚无环境伤害来源，预留给后续版本。
    #[allow(dead_code)]
    Environmental,
    /// 实体伤害；当前版本尚无攻击型实体，预留给后续版本。
    #[allow(dead_code)]
    Entity,
    /// 脚本或命令造成的伤害。
    #[allow(dead_code)]
    Custom,
}

impl DamageSource {
    /// 面向玩家的简短原因；字体仅覆盖 ASCII。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Fall => "FALL",
            Self::Environmental => "ENVIRONMENT",
            Self::Entity => "ENTITY",
            Self::Custom => "CUSTOM",
        }
    }
}

/// 一次伤害事件。构造函数会拒绝 0 或非有限数值，保证生命值单调下降。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Damage {
    amount: u32,
    source: DamageSource,
    bypass_invulnerability: bool,
}

impl Damage {
    /// 构造一次有效伤害；`amount` 为 0 时返回 `None`。
    pub fn new(amount: u32, source: DamageSource) -> Option<Self> {
        (amount > 0).then_some(Self {
            amount,
            source,
            bypass_invulnerability: false,
        })
    }

    pub const fn amount(self) -> u32 {
        self.amount
    }

    pub const fn source(self) -> DamageSource {
        self.source
    }

    pub const fn bypasses_invulnerability(self) -> bool {
        self.bypass_invulnerability
    }

    /// 标记本次伤害无视无敌帧；用于重生保护或脚本伤害。
    #[allow(dead_code)]
    pub const fn ignoring_invulnerability(mut self) -> Self {
        self.bypass_invulnerability = true;
        self
    }
}

/// 由实际下落距离换算跌落伤害；NaN、负值或安全高度内返回 `None`。
pub fn fall_damage(distance: f32) -> Option<Damage> {
    if !distance.is_finite() || distance <= SAFE_FALL_DISTANCE {
        return None;
    }
    let blocks = (distance - SAFE_FALL_DISTANCE).floor();
    if blocks <= 0.0 {
        return None;
    }
    Damage::new(blocks as u32 * DAMAGE_PER_BLOCK, DamageSource::Fall)
}

/// 统一伤害入口：检查存活状态、无敌帧与数值后扣血。
pub fn apply(survival: &mut Survival, damage: Damage) -> DamageOutcome {
    survival.receive_damage(damage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_and_invalid_damage_is_rejected() {
        assert!(Damage::new(0, DamageSource::Fall).is_none());
        assert_eq!(fall_damage(f32::NAN), None);
        assert_eq!(fall_damage(-4.0), None);
        assert_eq!(fall_damage(f32::INFINITY), None);
    }

    #[test]
    fn safe_falls_do_not_generate_damage() {
        assert_eq!(fall_damage(0.0), None);
        assert_eq!(fall_damage(3.0), None);
        assert_eq!(fall_damage(3.9), None, "不足额外一格不扣血");
    }

    #[test]
    fn fall_damage_scales_one_point_per_block_above_three() {
        for (distance, expected) in [(4.0, 1), (5.0, 2), (5.5, 2), (10.0, 7), (20.0, 17)] {
            let damage = fall_damage(distance).unwrap_or_else(|| panic!("{distance} 应产生伤害"));
            assert_eq!(damage.amount(), expected, "距离 {distance}");
            assert_eq!(damage.source(), DamageSource::Fall);
        }
    }

    #[test]
    fn damage_can_opt_out_of_invulnerability() {
        let damage = Damage::new(1, DamageSource::Custom).expect("有效伤害");
        assert!(!damage.bypasses_invulnerability());
        assert!(damage.ignoring_invulnerability().bypasses_invulnerability());
    }
}
