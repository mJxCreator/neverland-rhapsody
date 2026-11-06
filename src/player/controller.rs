//! 玩家输入控制器：把键盘与鼠标事件转换为摄像机的移动和视角。

use glam::Vec3;
use winit::keyboard::KeyCode;

use crate::renderer::camera::Camera;

/// 每像素鼠标位移对应的弧度。
const MOUSE_SENSITIVITY: f32 = 0.0022;
/// 摄像机移动速度（单位/秒）。
const MOVE_SPEED: f32 = 8.0;

/// 保存当前按键与待应用的鼠标位移，逐帧驱动摄像机。
#[derive(Debug, Default)]
pub struct PlayerController {
    forward: bool,
    backward: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    look_delta: (f32, f32),
}

impl PlayerController {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录移动键状态，其余按键忽略。
    pub fn handle_key(&mut self, code: KeyCode, pressed: bool) {
        let slot = match code {
            KeyCode::KeyW => &mut self.forward,
            KeyCode::KeyS => &mut self.backward,
            KeyCode::KeyA => &mut self.left,
            KeyCode::KeyD => &mut self.right,
            KeyCode::Space => &mut self.up,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => &mut self.down,
            _ => return,
        };
        *slot = pressed;
    }

    /// 累加本帧的鼠标原始位移。
    pub fn add_look(&mut self, dx: f32, dy: f32) {
        self.look_delta.0 += dx;
        self.look_delta.1 += dy;
    }

    /// 清空全部输入状态，例如窗口失去焦点时。
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// 应用视角旋转与键盘位移；`delta_secs` 为本帧时长（秒）。
    pub fn update(&mut self, camera: &mut Camera, delta_secs: f32) {
        if self.look_delta != (0.0, 0.0) {
            // 鼠标向右/向下移动时，视角向右/向下转。
            camera.rotate(
                -self.look_delta.0 * MOUSE_SENSITIVITY,
                -self.look_delta.1 * MOUSE_SENSITIVITY,
            );
            self.look_delta = (0.0, 0.0);
        }

        let forward = camera.forward();
        let right = camera.right();
        let mut direction = Vec3::ZERO;
        if self.forward {
            direction += forward;
        }
        if self.backward {
            direction -= forward;
        }
        if self.right {
            direction += right;
        }
        if self.left {
            direction -= right;
        }
        if self.up {
            direction += Vec3::Y;
        }
        if self.down {
            direction -= Vec3::Y;
        }

        if direction.length_squared() > 0.0 {
            camera.translate(direction.normalize() * MOVE_SPEED * delta_secs);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera() -> Camera {
        Camera::new(Vec3::ZERO, 0.0, 0.0, 1.0)
    }

    #[test]
    fn movement_keys_change_the_view() {
        let mut camera = camera();
        let before = camera.view_proj();

        let mut controller = PlayerController::new();
        controller.handle_key(KeyCode::KeyW, true);
        controller.update(&mut camera, 1.0);

        assert_ne!(before, camera.view_proj());
    }

    #[test]
    fn mouse_look_changes_the_view() {
        let mut camera = camera();
        let before = camera.view_proj();

        let mut controller = PlayerController::new();
        controller.add_look(100.0, 0.0);
        controller.update(&mut camera, 0.0);

        assert_ne!(before, camera.view_proj());
    }

    #[test]
    fn release_keys_stops_movement() {
        let mut camera = camera();

        let mut controller = PlayerController::new();
        controller.handle_key(KeyCode::KeyW, true);
        controller.update(&mut camera, 1.0);
        let moved = camera.view_proj();

        controller.handle_key(KeyCode::KeyW, false);
        controller.update(&mut camera, 1.0);

        assert_eq!(moved, camera.view_proj());
    }
}
