//! 攻击状态机枚举与时间参数。

/// 一次攻击的阶段。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttackState {
    /// 可以发起新攻击。
    Idle,
    /// 前摇：已经锁定武器参数，但尚未判定命中。
    Windup,
    /// 判定窗口：命中只结算一次。
    Active,
    /// 后摇与冷却：不能发起新攻击。
    Recovery,
}

impl AttackState {
    /// 调试与日志使用的名称。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Windup => "windup",
            Self::Active => "active",
            Self::Recovery => "recovery",
        }
    }

    pub const fn is_idle(self) -> bool {
        matches!(self, Self::Idle)
    }
}

/// 攻击时序参数；所有时间单位为秒。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CombatConfig {
    /// 前摇时长。
    pub windup: f32,
    /// 判定窗口时长。
    pub active: f32,
}

impl Default for CombatConfig {
    fn default() -> Self {
        Self {
            windup: 0.10,
            active: 0.12,
        }
    }
}
