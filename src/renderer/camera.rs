//! 摄像机：位置与投影参数生成视图投影矩阵。

use glam::camera::rh::proj::directx::perspective;
use glam::camera::rh::view::look_at_mat4;
use glam::{Mat4, Vec3};

const FOV_Y: f32 = 60.0;
const NEAR: f32 = 0.1;
const FAR: f32 = 200.0;

/// 观察世界原点附近的透视摄像机。
pub struct Camera {
    position: Vec3,
    aspect: f32,
}

impl Camera {
    pub fn new(position: Vec3, aspect: f32) -> Self {
        Self { position, aspect }
    }

    /// 视口尺寸变化时更新宽高比。
    pub fn set_aspect(&mut self, aspect: f32) {
        self.aspect = aspect;
    }

    /// 视图投影矩阵（右上手坐标系，裁剪空间深度 0..1）。
    pub fn view_proj(&self) -> Mat4 {
        let view = look_at_mat4(self.position, Vec3::ZERO, Vec3::Y);
        let projection = perspective(FOV_Y.to_radians(), self.aspect, NEAR, FAR);
        projection * view
    }
}
