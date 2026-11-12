//! 第三人称相机：位于玩家后方，遇到墙体时沿视线收缩距离。
//!
//! 相机只改变观察位置，不修改玩家位置、朝向、物理或装备状态。

use glam::Vec3;

/// 第三人称相机参数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThirdPersonConfig {
    /// 无遮挡时的跟随距离（格）。
    pub distance: f32,
    /// 与墙面之间保留的安全距离（格）。
    pub margin: f32,
    /// 遮挡检测采样步长（格）。
    pub step: f32,
}

impl Default for ThirdPersonConfig {
    fn default() -> Self {
        Self {
            distance: 3.6,
            margin: 0.25,
            step: 0.06,
        }
    }
}

/// 由偏航/俯仰计算视线方向；与第一人称相机完全一致。
pub fn view_forward(yaw: f32, pitch: f32) -> Vec3 {
    let (sin_yaw, cos_yaw) = yaw.sin_cos();
    let (sin_pitch, cos_pitch) = pitch.sin_cos();
    Vec3::new(-sin_yaw * cos_pitch, sin_pitch, -cos_yaw * cos_pitch)
}

/// 求解第三人称相机位置。
///
/// 从眼睛沿视线反方向外推，遇到实心方块时停在障碍前；
/// `is_solid` 由调用方提供世界方块查询，便于单元测试。
pub fn camera_position(
    eye: Vec3,
    yaw: f32,
    pitch: f32,
    config: ThirdPersonConfig,
    is_solid: impl Fn(i32, i32, i32) -> bool,
) -> Vec3 {
    let forward = view_forward(yaw, pitch);
    let desired = eye - forward * config.distance.max(0.0);
    let offset = desired - eye;
    let total = offset.length();
    if total <= 1e-4 {
        return desired;
    }
    let step = config.step.max(0.01);
    let direction = offset / total;
    let mut travelled = 0.0;
    let mut last_safe = 0.0;
    while travelled <= total {
        let sample = eye + direction * travelled;
        if is_solid(
            sample.x.floor() as i32,
            sample.y.floor() as i32,
            sample.z.floor() as i32,
        ) {
            break;
        }
        last_safe = travelled;
        travelled += step;
    }
    if last_safe >= total - step {
        return desired;
    }
    let safe_distance = (last_safe - config.margin).max(0.1).min(total);
    eye + direction * safe_distance
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_matches_first_person_camera() {
        let forward = view_forward(0.0, 0.0);
        assert!((forward - Vec3::NEG_Z).length() < 1e-6);
        let forward = view_forward(std::f32::consts::FRAC_PI_2, 0.0);
        assert!((forward - Vec3::NEG_X).length() < 1e-6);
    }

    #[test]
    fn unobstructed_camera_keeps_full_distance() {
        let eye = Vec3::new(0.0, 1.6, 0.0);
        let config = ThirdPersonConfig::default();
        let position = camera_position(eye, 0.0, 0.0, config, |_, _, _| false);
        assert!((position - (eye + Vec3::Z * config.distance)).length() < 1e-4);
    }

    #[test]
    fn wall_behind_player_shortens_the_camera() {
        let eye = Vec3::new(0.0, 1.6, 0.0);
        let config = ThirdPersonConfig::default();
        // z = 2 处有一面墙；相机默认想去 z = 3.6。
        let position = camera_position(eye, 0.0, 0.0, config, |_, _, z| z >= 2);
        assert!(position.z > 0.0 && position.z < 2.0, "相机应停在墙前");
        assert!((position.z - (2.0 - config.margin)).abs() < 0.15);
        assert!((position.x).abs() < 1e-5);
    }

    #[test]
    fn ceiling_does_not_invert_the_camera() {
        let eye = Vec3::new(0.0, 1.6, 0.0);
        let config = ThirdPersonConfig::default();
        // 向上看时相机位于脚下方向；地面 y < 0 视为实心。
        let position = camera_position(eye, 0.0, 1.2, config, |_, y, _| y < 0);
        assert!(position.y >= 0.0, "相机不应穿入地面");
    }
}
