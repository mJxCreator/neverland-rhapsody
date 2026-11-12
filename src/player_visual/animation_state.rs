//! 玩家动画状态与状态机。
//!
//! 动画只依据真实物理与游戏状态判断，不读取键盘输入：
//! 角色被墙挡住时水平速度接近 0，会回到 Idle 而不是继续播放走路。

/// 玩家可播放的动画状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlayerAnimationState {
    Idle,
    Walk,
    Run,
    Jump,
    Fall,
    Land,
    Attack,
    Mining,
    Eat,
    Hurt,
    Death,
}

impl PlayerAnimationState {
    /// 模型片段名；与内置玩家模型及资源规范一致。
    pub const fn clip_name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Walk => "walk",
            Self::Run => "run",
            Self::Jump => "jump",
            Self::Fall => "fall",
            Self::Land => "land",
            Self::Attack => "attack",
            Self::Mining => "mining",
            Self::Eat => "eat",
            Self::Hurt => "hurt",
            Self::Death => "death",
        }
    }

    /// 是否循环播放。
    pub const fn looping(self) -> bool {
        matches!(
            self,
            Self::Idle | Self::Walk | Self::Run | Self::Mining | Self::Eat
        )
    }
}

/// 驱动动画所需的游戏状态快照。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationContext {
    pub grounded: bool,
    /// 垂直速度（格/秒），向上为正。
    pub vertical_speed: f32,
    /// 实际水平速度（格/秒），来自物理结果而非输入。
    pub horizontal_speed: f32,
    pub sprinting: bool,
    pub mining: bool,
    pub eating: bool,
    pub attacking: bool,
    pub dead: bool,
    /// 本帧真实落地事件。
    pub landed: bool,
    /// 本帧受到伤害。
    pub damaged: bool,
}

impl Default for AnimationContext {
    fn default() -> Self {
        Self {
            grounded: true,
            vertical_speed: 0.0,
            horizontal_speed: 0.0,
            sprinting: false,
            mining: false,
            eating: false,
            attacking: false,
            dead: false,
            landed: false,
            damaged: false,
        }
    }
}

/// 状态机输出：目标状态、是否强制重播与播放速率。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationUpdate {
    pub state: PlayerAnimationState,
    /// 同一片段需要从头重播（连续攻击、连续受击、落地缓冲）。
    pub restart: bool,
    pub speed: f32,
}

/// 落地与受击缓冲时长（秒）。
pub const LAND_DURATION: f32 = 0.22;
pub const HURT_DURATION: f32 = 0.30;
/// 冲刺动画的最低水平速度。
const RUN_SPEED_THRESHOLD: f32 = 4.8;
/// 行走动画的最低水平速度。
const WALK_SPEED_THRESHOLD: f32 = 0.15;
/// 起跳动画的最低向上速度。
const JUMP_SPEED_THRESHOLD: f32 = 0.25;

/// 玩家动画控制器：把游戏状态转换为动画状态并维护短时事件窗口。
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerAnimationController {
    state: PlayerAnimationState,
    land_remaining: f32,
    hurt_remaining: f32,
    was_attacking: bool,
}

impl Default for PlayerAnimationController {
    fn default() -> Self {
        Self::new()
    }
}

impl PlayerAnimationController {
    pub fn new() -> Self {
        Self {
            state: PlayerAnimationState::Idle,
            land_remaining: 0.0,
            hurt_remaining: 0.0,
            was_attacking: false,
        }
    }

    /// 重生或切换世界后回到初始状态。
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// 推进状态机并返回目标状态。
    pub fn update(&mut self, context: &AnimationContext, delta: f32) -> AnimationUpdate {
        let delta = if delta.is_finite() {
            delta.max(0.0)
        } else {
            0.0
        };
        // 先消耗上一帧的缓冲时间，再接入本帧新事件，避免一帧大 delta 把
        // 刚触发的落地/受击动作直接吞掉。
        self.land_remaining = (self.land_remaining - delta).max(0.0);
        self.hurt_remaining = (self.hurt_remaining - delta).max(0.0);
        if context.landed {
            self.land_remaining = LAND_DURATION;
        }
        if context.damaged {
            self.hurt_remaining = HURT_DURATION;
        }

        let state = if context.dead {
            PlayerAnimationState::Death
        } else if self.hurt_remaining > 0.0 {
            PlayerAnimationState::Hurt
        } else if context.attacking {
            PlayerAnimationState::Attack
        } else if self.land_remaining > 0.0 {
            PlayerAnimationState::Land
        } else if !context.grounded {
            if context.vertical_speed > JUMP_SPEED_THRESHOLD {
                PlayerAnimationState::Jump
            } else {
                PlayerAnimationState::Fall
            }
        } else if context.mining {
            PlayerAnimationState::Mining
        } else if context.eating {
            PlayerAnimationState::Eat
        } else if context.sprinting && context.horizontal_speed >= RUN_SPEED_THRESHOLD {
            PlayerAnimationState::Run
        } else if context.horizontal_speed >= WALK_SPEED_THRESHOLD {
            PlayerAnimationState::Walk
        } else {
            PlayerAnimationState::Idle
        };

        // 连续攻击时片段不会被“切换”，需要显式从头播放；
        // 受击与落地同理，否则第二次事件看不到动作。
        let restart = context.damaged
            || (context.attacking && !self.was_attacking)
            || (context.landed && state == PlayerAnimationState::Land);

        self.state = state;
        self.was_attacking = context.attacking;

        AnimationUpdate {
            state,
            restart,
            speed: playback_speed(state, context.horizontal_speed),
        }
    }
}

/// 移动动画按实际速度缩放，避免“滑步”。
fn playback_speed(state: PlayerAnimationState, horizontal_speed: f32) -> f32 {
    match state {
        PlayerAnimationState::Walk => (horizontal_speed / 2.9).clamp(0.7, 1.4),
        PlayerAnimationState::Run => (horizontal_speed / 5.2).clamp(0.8, 1.5),
        PlayerAnimationState::Mining => 1.15,
        _ => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> AnimationContext {
        AnimationContext::default()
    }

    #[test]
    fn still_player_idles() {
        let mut controller = PlayerAnimationController::new();
        let update = controller.update(&context(), 0.016);
        assert_eq!(update.state, PlayerAnimationState::Idle);
        assert!(!update.restart);
    }

    #[test]
    fn movement_state_follows_actual_speed_not_input() {
        let mut controller = PlayerAnimationController::new();
        let mut ctx = context();
        ctx.horizontal_speed = 2.0;
        assert_eq!(
            controller.update(&ctx, 0.016).state,
            PlayerAnimationState::Walk
        );

        // 冲刺键按住但被墙挡住：速度接近 0，应回到 Idle。
        ctx.sprinting = true;
        ctx.horizontal_speed = 0.0;
        assert_eq!(
            controller.update(&ctx, 0.016).state,
            PlayerAnimationState::Idle
        );

        ctx.horizontal_speed = 6.4;
        assert_eq!(
            controller.update(&ctx, 0.016).state,
            PlayerAnimationState::Run
        );
    }

    #[test]
    fn airborne_states_use_vertical_velocity() {
        let mut controller = PlayerAnimationController::new();
        let mut ctx = context();
        ctx.grounded = false;
        ctx.vertical_speed = 3.0;
        assert_eq!(
            controller.update(&ctx, 0.016).state,
            PlayerAnimationState::Jump
        );
        ctx.vertical_speed = -5.0;
        assert_eq!(
            controller.update(&ctx, 0.016).state,
            PlayerAnimationState::Fall
        );
    }

    #[test]
    fn landing_window_plays_once_then_returns_to_locomotion() {
        let mut controller = PlayerAnimationController::new();
        let mut ctx = context();
        ctx.landed = true;
        let update = controller.update(&ctx, 0.016);
        assert_eq!(update.state, PlayerAnimationState::Land);
        assert!(update.restart);

        ctx.landed = false;
        ctx.horizontal_speed = 2.0;
        assert_eq!(
            controller.update(&ctx, 0.016).state,
            PlayerAnimationState::Land
        );
        assert_eq!(
            controller.update(&ctx, LAND_DURATION).state,
            PlayerAnimationState::Walk
        );
    }

    #[test]
    fn combat_and_damage_take_priority() {
        let mut controller = PlayerAnimationController::new();
        let mut ctx = context();
        ctx.mining = true;
        assert_eq!(
            controller.update(&ctx, 0.016).state,
            PlayerAnimationState::Mining
        );

        ctx.attacking = true;
        let update = controller.update(&ctx, 0.016);
        assert_eq!(update.state, PlayerAnimationState::Attack);
        assert!(update.restart, "新一次攻击应从头播放");

        let update = controller.update(&ctx, 0.016);
        assert!(!update.restart, "同一次攻击不重复重播");

        ctx.damaged = true;
        assert_eq!(
            controller.update(&ctx, 0.016).state,
            PlayerAnimationState::Hurt
        );

        ctx.dead = true;
        assert_eq!(
            controller.update(&ctx, 0.016).state,
            PlayerAnimationState::Death
        );

        ctx.dead = false;
        ctx.damaged = false;
        assert_eq!(
            controller.update(&ctx, HURT_DURATION * 2.0).state,
            PlayerAnimationState::Attack
        );
    }

    #[test]
    fn eating_is_lower_priority_than_mining() {
        let mut controller = PlayerAnimationController::new();
        let mut ctx = context();
        ctx.eating = true;
        assert_eq!(
            controller.update(&ctx, 0.016).state,
            PlayerAnimationState::Eat
        );
        ctx.mining = true;
        assert_eq!(
            controller.update(&ctx, 0.016).state,
            PlayerAnimationState::Mining
        );
    }

    #[test]
    fn playback_speed_scales_with_velocity() {
        assert!(playback_speed(PlayerAnimationState::Walk, 2.9) > 0.9);
        assert!(playback_speed(PlayerAnimationState::Walk, 6.0) <= 1.4);
        assert!(playback_speed(PlayerAnimationState::Run, 6.5) > 1.0);
        assert_eq!(playback_speed(PlayerAnimationState::Idle, 0.0), 1.0);
    }

    #[test]
    fn reset_returns_to_idle() {
        let mut controller = PlayerAnimationController::new();
        let mut ctx = context();
        ctx.dead = true;
        controller.update(&ctx, 0.016);
        controller.reset();
        assert_eq!(controller.state, PlayerAnimationState::Idle);
    }
}
