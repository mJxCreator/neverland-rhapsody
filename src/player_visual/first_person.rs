//! 第一人称表现：独立的相机空间手臂与手持物品。
//!
//! 不把第三人称人物模型放在相机前，而是用专门的 `player_fp_arms` 模型，
//! 位置固定在视线右下方，避免遮挡主要视野。

use glam::{Mat4, Quat, Vec3};

use super::animation_state::PlayerAnimationState;
use crate::model::types::Transform;

/// 手臂根节点在相机空间的默认位置（右、下、前）。
pub const ARM_ORIGIN: Vec3 = Vec3::new(0.30, -0.27, -0.42);
/// 攻击动作的参考时长（秒），与内置 attack 片段一致。
const ATTACK_DURATION: f32 = 0.65;

/// 相机世界变换；本地 `-Z` 为视线前方。
pub fn camera_transform(position: Vec3, yaw: f32, pitch: f32) -> Mat4 {
    let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);
    Mat4::from_rotation_translation(rotation, position)
}

/// 手臂根节点的相机空间局部变换。
pub fn arm_local(
    state: PlayerAnimationState,
    horizontal_speed: f32,
    time: f32,
    time_in_state: f32,
) -> Transform {
    let mut offset = ARM_ORIGIN;
    let mut rotation = Quat::IDENTITY;
    match state {
        PlayerAnimationState::Walk => {
            let phase = time * 6.0;
            offset.y += phase.sin() * 0.012;
            offset.x += (phase * 0.5).cos() * 0.006;
            rotation *= Quat::from_rotation_x(phase.sin() * 0.04);
        }
        PlayerAnimationState::Run => {
            let phase = time * 9.0;
            offset.y += phase.sin() * 0.022;
            offset.x += (phase * 0.5).cos() * 0.010;
            rotation *= Quat::from_rotation_x(phase.sin() * 0.08);
        }
        PlayerAnimationState::Mining => {
            let phase = time * 11.0;
            offset.y += phase.sin() * 0.012;
            rotation *= Quat::from_rotation_x(phase.sin() * 0.45);
        }
        PlayerAnimationState::Attack => {
            let progress = (time_in_state / ATTACK_DURATION).clamp(0.0, 1.0);
            let swing = (progress * std::f32::consts::PI).sin();
            offset.z -= 0.06 * swing;
            rotation *= Quat::from_rotation_x(1.15 * swing);
        }
        PlayerAnimationState::Eat => {
            let phase = time_in_state * 6.0;
            offset = Vec3::new(0.10, -0.10 + phase.sin() * 0.008, -0.26);
            rotation *= Quat::from_rotation_x(0.25);
        }
        PlayerAnimationState::Jump | PlayerAnimationState::Fall => {
            rotation *= Quat::from_rotation_x(-0.12);
        }
        PlayerAnimationState::Hurt => {
            offset.x += 0.02;
            rotation *= Quat::from_rotation_z(0.12);
        }
        PlayerAnimationState::Idle | PlayerAnimationState::Land | PlayerAnimationState::Death => {
            let phase = time * 1.6;
            offset.y += phase.sin() * 0.004;
        }
    }
    // 速度只影响微小摆动幅度，避免高速时手臂抖动过大。
    let speed_scale = (horizontal_speed / 6.0).clamp(0.0, 1.0);
    offset.x += speed_scale * 0.004;
    Transform::new(offset, rotation, Vec3::ONE)
}

/// 手臂根节点的世界变换。
pub fn arm_transform(
    camera_position: Vec3,
    yaw: f32,
    pitch: f32,
    state: PlayerAnimationState,
    horizontal_speed: f32,
    time: f32,
    time_in_state: f32,
) -> Mat4 {
    camera_transform(camera_position, yaw, pitch)
        * arm_local(state, horizontal_speed, time, time_in_state).to_matrix()
}

/// 手持物品的世界变换：手臂根节点 × 物品的第一人称局部变换。
#[allow(clippy::too_many_arguments)]
pub fn held_transform(
    camera_position: Vec3,
    yaw: f32,
    pitch: f32,
    state: PlayerAnimationState,
    horizontal_speed: f32,
    time: f32,
    time_in_state: f32,
    item_local: Transform,
) -> Mat4 {
    arm_transform(
        camera_position,
        yaw,
        pitch,
        state,
        horizontal_speed,
        time,
        time_in_state,
    ) * item_local.to_matrix()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arm_stays_in_front_below_and_to_the_right() {
        let matrix = arm_transform(
            Vec3::new(10.0, 2.0, 5.0),
            0.0,
            0.0,
            PlayerAnimationState::Idle,
            0.0,
            0.0,
            0.0,
        );
        let hand = matrix.transform_point3(Vec3::ZERO);
        let relative = hand - Vec3::new(10.0, 2.0, 5.0);
        assert!(relative.x > 0.0, "手臂应在视线右侧");
        assert!(relative.y < 0.0, "手臂应在视线下方");
        assert!(relative.z < -0.2, "手臂应在相机前方");
    }

    #[test]
    fn attack_raises_the_arm_during_the_swing() {
        let hand = Vec3::new(0.0, 0.0, -0.27);
        let idle = arm_transform(
            Vec3::ZERO,
            0.0,
            0.0,
            PlayerAnimationState::Idle,
            0.0,
            0.0,
            0.0,
        )
        .transform_point3(hand);
        let attack = arm_transform(
            Vec3::ZERO,
            0.0,
            0.0,
            PlayerAnimationState::Attack,
            0.0,
            0.0,
            ATTACK_DURATION * 0.5,
        )
        .transform_point3(hand);
        assert!(attack.y > idle.y, "挥击中途手臂应抬起");
    }

    #[test]
    fn held_item_follows_the_arm_hand() {
        let local = Transform::new(Vec3::new(0.0, 0.0, -0.27), Quat::IDENTITY, Vec3::ONE);
        let arm = arm_transform(
            Vec3::ZERO,
            0.0,
            0.0,
            PlayerAnimationState::Idle,
            0.0,
            0.0,
            0.0,
        );
        let held = held_transform(
            Vec3::ZERO,
            0.0,
            0.0,
            PlayerAnimationState::Idle,
            0.0,
            0.0,
            0.0,
            local,
        );
        let expected = arm.transform_point3(Vec3::new(0.0, 0.0, -0.27));
        let actual = held.transform_point3(Vec3::ZERO);
        assert!((actual - expected).length() < 1e-5);
    }
}
