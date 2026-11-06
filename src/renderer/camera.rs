//! 摄像机：位置、朝向与投影参数生成视图投影矩阵。

use glam::camera::rh::proj::directx::perspective;
use glam::camera::rh::view::look_at_mat4;
use glam::{Mat4, Vec3};

const FOV_Y: f32 = 60.0;
const NEAR: f32 = 0.1;
const FAR: f32 = 200.0;
/// 俯仰角上限（度），避免视线与世界上方向重合导致视图矩阵退化。
const MAX_PITCH_DEGREES: f32 = 89.0;

/// 第一人称透视摄像机，用偏航与俯仰描述朝向。
pub struct Camera {
    position: Vec3,
    /// 绕 Y 轴的角度（弧度），增大表示向左转。
    yaw: f32,
    /// 抬头为正、低头为负的角度（弧度）。
    pitch: f32,
    aspect: f32,
}

impl Camera {
    pub fn new(position: Vec3, yaw: f32, pitch: f32, aspect: f32) -> Self {
        Self {
            position,
            yaw,
            pitch: clamp_pitch(pitch),
            aspect,
        }
    }

    /// 视口尺寸变化时更新宽高比。
    pub fn set_aspect(&mut self, aspect: f32) {
        self.aspect = aspect;
    }

    /// 叠加视角旋转；yaw 正方向为向左转，pitch 正方向为抬头。
    pub fn rotate(&mut self, yaw_delta: f32, pitch_delta: f32) {
        self.yaw += yaw_delta;
        self.pitch = clamp_pitch(self.pitch + pitch_delta);
    }

    /// 按世界坐标平移摄像机。
    pub fn translate(&mut self, delta: Vec3) {
        self.position += delta;
    }

    /// 视线方向的单位向量（含俯仰）。
    pub fn forward(&self) -> Vec3 {
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        Vec3::new(-sin_yaw * cos_pitch, sin_pitch, -cos_yaw * cos_pitch)
    }

    /// 水平右方向的单位向量（不含俯仰）。
    pub fn right(&self) -> Vec3 {
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        Vec3::new(cos_yaw, 0.0, -sin_yaw)
    }

    /// 视图投影矩阵（右上手坐标系，裁剪空间深度 0..1）。
    pub fn view_proj(&self) -> Mat4 {
        let view = look_at_mat4(self.position, self.position + self.forward(), Vec3::Y);
        let projection = perspective(FOV_Y.to_radians(), self.aspect, NEAR, FAR);
        projection * view
    }
}

fn clamp_pitch(pitch: f32) -> f32 {
    let limit = MAX_PITCH_DEGREES.to_radians();
    pitch.clamp(-limit, limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_camera() -> Camera {
        Camera::new(Vec3::ZERO, 0.0, 0.0, 1.0)
    }

    #[test]
    fn forward_points_to_negative_z_by_default() {
        let camera = test_camera();
        assert!((camera.forward() - Vec3::NEG_Z).length() < 1e-6);
    }

    #[test]
    fn right_is_positive_x_by_default() {
        let camera = test_camera();
        assert!((camera.right() - Vec3::X).length() < 1e-6);
    }

    #[test]
    fn pitch_is_clamped() {
        let mut camera = test_camera();
        camera.rotate(0.0, 10.0);
        assert!(camera.pitch <= MAX_PITCH_DEGREES.to_radians() + 1e-6);
        camera.rotate(0.0, -20.0);
        assert!(camera.pitch >= -MAX_PITCH_DEGREES.to_radians() - 1e-6);
    }
}
