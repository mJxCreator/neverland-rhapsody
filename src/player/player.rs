//! 玩家实体：位置、速度、碰撞体、地面状态与观察方向。
//!
//! 位置统一以实体底部中心为基准；摄像机位置由 `eye_position` 派生。

use glam::{IVec3, Vec3};

use super::collision::Aabb;

/// 俯仰角上限（度），避免视线与世界上方向重合。
const MAX_PITCH_DEGREES: f32 = 89.0;

fn max_pitch() -> f32 {
    MAX_PITCH_DEGREES.to_radians()
}

/// 玩家碰撞体尺寸，与物理参数一起集中管理。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerDimensions {
    pub width: f32,
    pub height: f32,
    pub eye_height: f32,
}

/// 由物理检测结果决定的移动状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MovementState {
    Grounded,
    Airborne,
}

/// 独立的玩家实体，不直接处理输入或渲染。
pub struct Player {
    position: Vec3,
    /// 上一物理帧末尾的位置，供摄像机插值。
    previous_position: Vec3,
    velocity: Vec3,
    /// 最近一次物理步的加速度，用于调试与状态读取。
    acceleration: Vec3,
    grounded: bool,
    /// 绕 Y 轴的角度（弧度），增大表示向左转。
    yaw: f32,
    /// 抬头为正、低头为负的角度（弧度）。
    pitch: f32,
    dimensions: PlayerDimensions,
    /// 本次腾空期间到达的最高位置，落地时用于计算真实下落距离。
    fall_peak_y: f32,
}

impl Player {
    pub fn new(position: Vec3, dimensions: PlayerDimensions) -> Self {
        Self {
            position,
            previous_position: position,
            velocity: Vec3::ZERO,
            acceleration: Vec3::ZERO,
            grounded: false,
            yaw: 0.0,
            pitch: 0.0,
            dimensions,
            fall_peak_y: position.y,
        }
    }

    /// 玩家底部中心的世界坐标。
    pub fn position(&self) -> Vec3 {
        self.position
    }

    /// 三轴速度（单位/秒）。
    pub fn velocity(&self) -> Vec3 {
        self.velocity
    }

    /// 最近一次物理步的加速度。
    pub fn acceleration(&self) -> Vec3 {
        self.acceleration
    }

    pub fn yaw(&self) -> f32 {
        self.yaw
    }

    pub fn pitch(&self) -> f32 {
        self.pitch
    }

    /// 叠加视角旋转；yaw 正方向向左转，pitch 正方向抬头。
    pub fn rotate_look(&mut self, yaw_delta: f32, pitch_delta: f32) {
        self.yaw += yaw_delta;
        let limit = max_pitch();
        self.pitch = (self.pitch + pitch_delta).clamp(-limit, limit);
    }

    /// 眼睛所在的世界坐标，摄像机跟随该点。
    pub fn eye_position(&self) -> Vec3 {
        self.position + Vec3::Y * self.dimensions.eye_height
    }

    /// 按物理插值系数在上一帧与当前帧位置之间取眼睛位置，用于平滑渲染。
    pub fn interpolated_eye_position(&self, alpha: f32) -> Vec3 {
        let alpha = alpha.clamp(0.0, 1.0);
        self.previous_position.lerp(self.position, alpha) + Vec3::Y * self.dimensions.eye_height
    }

    /// 水平前方向（忽略俯仰），用于地面移动。
    pub fn horizontal_forward(&self) -> Vec3 {
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        Vec3::new(-sin_yaw, 0.0, -cos_yaw)
    }

    /// 水平右方向（忽略俯仰）。
    pub fn horizontal_right(&self) -> Vec3 {
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        Vec3::new(cos_yaw, 0.0, -sin_yaw)
    }

    /// 当前玩家包围盒。
    pub fn aabb(&self) -> Aabb {
        Aabb::from_feet(self.position, self.dimensions.width, self.dimensions.height)
    }

    /// 指定方块是否与玩家碰撞体相交，用于阻止方块放置在自己体内。
    pub fn intersects_block(&self, block: IVec3) -> bool {
        self.aabb().overlaps_block(block)
    }

    pub fn is_grounded(&self) -> bool {
        self.grounded
    }

    /// 当前移动状态由物理检测结果决定，而非输入。
    pub fn movement_state(&self) -> MovementState {
        if self.grounded {
            MovementState::Grounded
        } else {
            MovementState::Airborne
        }
    }

    /// 位置、速度、加速度与视角是否全部为有限值。
    pub fn is_finite(&self) -> bool {
        self.position.is_finite()
            && self.previous_position.is_finite()
            && self.velocity.is_finite()
            && self.acceleration.is_finite()
            && self.yaw.is_finite()
            && self.pitch.is_finite()
    }

    pub(crate) fn dimensions(&self) -> PlayerDimensions {
        self.dimensions
    }

    pub(crate) fn set_position(&mut self, position: Vec3) {
        self.position = position;
    }

    pub(crate) fn set_velocity(&mut self, velocity: Vec3) {
        self.velocity = velocity;
    }

    pub(crate) fn set_acceleration(&mut self, acceleration: Vec3) {
        self.acceleration = acceleration;
    }

    pub(crate) fn set_grounded(&mut self, grounded: bool) {
        self.grounded = grounded;
    }

    /// 当前腾空段内的最高位置。
    pub(crate) fn fall_peak_y(&self) -> f32 {
        self.fall_peak_y
    }

    /// 记录腾空期间到达过的最高位置。
    pub(crate) fn observe_air_position(&mut self, position_y: f32) {
        if position_y.is_finite() {
            self.fall_peak_y = self.fall_peak_y.max(position_y);
        }
    }

    /// 落地或重新站稳后重置下落距离基准。
    pub(crate) fn reset_fall_peak(&mut self, position_y: f32) {
        self.fall_peak_y = position_y;
    }

    /// 传送到安全位置并清空运动与跌落状态；重生与调试传送共用。
    pub fn teleport(&mut self, position: Vec3) {
        self.position = position;
        self.previous_position = position;
        self.velocity = Vec3::ZERO;
        self.acceleration = Vec3::ZERO;
        self.grounded = true;
        self.fall_peak_y = position.y;
    }

    /// 记录本次物理步开始时的位置，供摄像机插值。
    pub(crate) fn snapshot_previous_position(&mut self) {
        self.previous_position = self.position;
    }

    /// 数值异常时回到安全位置并清空运动状态。
    pub(crate) fn reset_motion(&mut self, position: Vec3) {
        self.position = position;
        self.velocity = Vec3::ZERO;
        self.acceleration = Vec3::ZERO;
        self.grounded = false;
        self.fall_peak_y = position.y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dimensions() -> PlayerDimensions {
        PlayerDimensions {
            width: 0.6,
            height: 1.8,
            eye_height: 1.62,
        }
    }

    #[test]
    fn eye_is_above_the_feet() {
        let player = Player::new(Vec3::new(1.0, 2.0, 3.0), dimensions());
        assert!((player.eye_position() - Vec3::new(1.0, 3.62, 3.0)).length() < 1e-6);
    }

    #[test]
    fn horizontal_directions_ignore_pitch() {
        let mut player = Player::new(Vec3::ZERO, dimensions());
        player.rotate_look(0.0, 0.9);
        assert!((player.horizontal_forward() - Vec3::NEG_Z).length() < 1e-6);
        assert!((player.horizontal_right() - Vec3::X).length() < 1e-6);
        assert_eq!(player.horizontal_forward().y, 0.0);
    }

    #[test]
    fn pitch_is_clamped() {
        let mut player = Player::new(Vec3::ZERO, dimensions());
        player.rotate_look(0.0, 10.0);
        assert!(player.pitch() <= max_pitch() + 1e-6);
        player.rotate_look(0.0, -20.0);
        assert!(player.pitch() >= -max_pitch() - 1e-6);
    }

    #[test]
    fn camera_interpolates_between_previous_and_current_positions() {
        let mut player = Player::new(Vec3::new(1.0, 11.0, 1.0), dimensions());
        player.snapshot_previous_position();
        player.set_position(Vec3::new(2.0, 11.0, 1.0));
        let eye = player.interpolated_eye_position(0.5);
        assert!((eye.x - 1.5).abs() < 1e-6);
        assert!((eye.y - (11.0 + 1.62)).abs() < 1e-6);
    }

    #[test]
    fn body_blocks_are_rejected_but_distant_blocks_are_allowed() {
        let player = Player::new(Vec3::new(10.0, 11.0, 10.0), dimensions());
        assert!(player.intersects_block(IVec3::new(10, 11, 10)));
        assert!(player.intersects_block(IVec3::new(10, 12, 10)));
        assert!(!player.intersects_block(IVec3::new(10, 13, 10)));
        assert!(!player.intersects_block(IVec3::new(11, 11, 10)));
    }

    #[test]
    fn air_state_is_not_grounded() {
        let player = Player::new(Vec3::ZERO, dimensions());
        assert_eq!(player.movement_state(), MovementState::Airborne);
        assert!(!player.is_grounded());
    }
}
