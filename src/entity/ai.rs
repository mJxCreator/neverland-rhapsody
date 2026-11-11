//! 生物 AI 的通用框架：状态枚举、可调参数与状态码转换。
//!
//! 具体状态迁移与地形判断在 [`super::creature_ai`] 中实现，本模块只提供数据。

/// 生物 AI 状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiState {
    /// 原地停留。
    Idle,
    /// 随机游荡。
    Wandering,
    /// 遇到障碍后的短暂调整。
    Avoiding,
    /// 受击后逃离伤害来源。
    Fleeing,
    /// 敌对生物在出生区域巡逻。
    Patrol,
    /// 敌对生物追击玩家。
    Chase,
    /// 敌对生物进入攻击动作。
    Attack,
    /// 敌对生物脱战后返回活动区域。
    Return,
    /// 敌对生物受击硬直。
    Hurt,
    /// 已死亡。
    Dead,
}

impl AiState {
    /// 转为存档使用的状态码。
    pub const fn code(self) -> u8 {
        match self {
            Self::Idle => 0,
            Self::Wandering => 1,
            Self::Avoiding => 2,
            Self::Fleeing => 3,
            Self::Dead => 4,
            Self::Patrol => 5,
            Self::Chase => 6,
            Self::Attack => 7,
            Self::Return => 8,
            Self::Hurt => 9,
        }
    }

    /// 从存档状态码恢复；未知值回退为 [`Self::Idle`]。
    pub const fn from_code(code: u8) -> Self {
        match code {
            1 => Self::Wandering,
            2 => Self::Avoiding,
            3 => Self::Fleeing,
            4 => Self::Dead,
            5 => Self::Patrol,
            6 => Self::Chase,
            7 => Self::Attack,
            8 => Self::Return,
            9 => Self::Hurt,
            _ => Self::Idle,
        }
    }

    /// 是否处于会主动移动的状态。
    pub const fn is_moving(self) -> bool {
        matches!(
            self,
            Self::Wandering | Self::Fleeing | Self::Patrol | Self::Chase | Self::Return
        )
    }
}

/// 生物 AI 参数；所有时间单位为秒，距离单位为格。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AiConfig {
    /// 停留最短时间。
    pub idle_seconds_min: f32,
    /// 停留最长时间。
    pub idle_seconds_max: f32,
    /// 单次游荡最短时间。
    pub wander_seconds_min: f32,
    /// 单次游荡最长时间。
    pub wander_seconds_max: f32,
    /// 障碍调整持续时间。
    pub avoid_seconds: f32,
    /// 受击逃离持续时间。
    pub flee_seconds: f32,
    /// 活动半径：离出生锚点超过该距离后主动返回。
    pub activity_radius: f32,
    /// 玩家在此距离内视为近处，使用更高的决策频率。
    pub near_distance: f32,
    /// 近处决策间隔。
    pub near_decision_interval: f32,
    /// 远处决策间隔。
    pub far_decision_interval: f32,
    /// 前方障碍探测距离。
    pub obstacle_probe: f32,
    /// 前方悬崖探测的向下深度。
    pub cliff_probe_depth: f32,
    /// 敌对生物的活动半径。
    pub hostile_activity_radius: f32,
    /// 受击硬直时间。
    pub hurt_seconds: f32,
    /// 敌对生物攻击前摇。
    pub attack_windup: f32,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            idle_seconds_min: 1.0,
            idle_seconds_max: 3.0,
            wander_seconds_min: 1.5,
            wander_seconds_max: 4.0,
            avoid_seconds: 0.7,
            flee_seconds: 3.0,
            activity_radius: 8.0,
            near_distance: 16.0,
            near_decision_interval: 0.2,
            far_decision_interval: 0.6,
            obstacle_probe: 0.8,
            cliff_probe_depth: 0.7,
            hostile_activity_radius: 16.0,
            hurt_seconds: 0.35,
            attack_windup: 0.35,
        }
    }
}

impl AiConfig {
    /// 根据与玩家的距离选择决策间隔：靠近玩家时更频繁，远处降低更新频率。
    pub fn decision_interval(&self, distance_to_player: f32) -> f32 {
        if distance_to_player <= self.near_distance {
            self.near_decision_interval
        } else {
            self.far_decision_interval
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_codes_roundtrip() {
        for state in [
            AiState::Idle,
            AiState::Wandering,
            AiState::Avoiding,
            AiState::Fleeing,
            AiState::Patrol,
            AiState::Chase,
            AiState::Attack,
            AiState::Return,
            AiState::Hurt,
            AiState::Dead,
        ] {
            assert_eq!(AiState::from_code(state.code()), state);
        }
        assert_eq!(AiState::from_code(99), AiState::Idle);
    }

    #[test]
    fn decision_interval_scales_with_distance() {
        let config = AiConfig::default();
        let near = config.decision_interval(3.0);
        let far = config.decision_interval(40.0);
        assert!(near < far, "靠近玩家时决策应更频繁");
    }

    #[test]
    fn default_ranges_are_ordered() {
        let config = AiConfig::default();
        assert!(config.idle_seconds_min <= config.idle_seconds_max);
        assert!(config.wander_seconds_min <= config.wander_seconds_max);
        assert!(config.activity_radius > 0.0);
        assert!(config.near_decision_interval > 0.0);
    }
}
