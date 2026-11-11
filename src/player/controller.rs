//! 玩家输入控制器：采集键盘与鼠标事件，输出移动意图与视角增量。
//!
//! 控制器不直接修改玩家位置，最终位移统一由物理系统处理。

use winit::keyboard::KeyCode;

/// 每像素鼠标位移对应的弧度。
const MOUSE_SENSITIVITY: f32 = 0.0022;

/// 一次输入解析得到的移动意图。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MovementIntent {
    /// 前后意图，`1.0` 表示向前。
    pub forward: f32,
    /// 左右意图，`1.0` 表示向右。
    pub right: f32,
    /// 是否请求跳跃。
    pub jump: bool,
    /// 是否请求冲刺；体力不足时由主循环清除。
    pub sprint: bool,
}

/// 保存当前按键与待应用的鼠标位移。
#[derive(Debug, Default)]
pub struct PlayerController {
    forward: bool,
    backward: bool,
    left: bool,
    right: bool,
    jump: bool,
    sprint: bool,
    look_delta: (f32, f32),
}

impl PlayerController {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录移动与跳跃键状态，其余按键忽略。
    pub fn handle_key(&mut self, code: KeyCode, pressed: bool) {
        let slot = match code {
            KeyCode::KeyW => &mut self.forward,
            KeyCode::KeyS => &mut self.backward,
            KeyCode::KeyA => &mut self.left,
            KeyCode::KeyD => &mut self.right,
            KeyCode::Space => &mut self.jump,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => &mut self.sprint,
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

    /// 取出并清空累计的视角旋转增量（yaw, pitch），单位为弧度。
    pub fn take_look(&mut self) -> (f32, f32) {
        // 鼠标向右/向下移动时，视角向右/向下转。
        let look = (
            -self.look_delta.0 * MOUSE_SENSITIVITY,
            -self.look_delta.1 * MOUSE_SENSITIVITY,
        );
        self.look_delta = (0.0, 0.0);
        look
    }

    /// 当前是否按住 Shift 请求冲刺。
    pub fn sprint(&self) -> bool {
        self.sprint
    }

    /// 当前按键组合对应的移动意图。
    pub fn intent(&self) -> MovementIntent {
        MovementIntent {
            forward: axis(self.forward, self.backward),
            right: axis(self.right, self.left),
            jump: self.jump,
            sprint: self.sprint,
        }
    }
}

/// 正负两个按键合成一个 -1..1 的轴值。
fn axis(positive: bool, negative: bool) -> f32 {
    f32::from(u8::from(positive)) - f32::from(u8::from(negative))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movement_keys_produce_an_intent() {
        let mut controller = PlayerController::new();
        controller.handle_key(KeyCode::KeyW, true);
        controller.handle_key(KeyCode::KeyD, true);

        let intent = controller.intent();
        assert_eq!(intent.forward, 1.0);
        assert_eq!(intent.right, 1.0);
        assert!(!intent.jump);
    }

    #[test]
    fn opposite_keys_cancel_out() {
        let mut controller = PlayerController::new();
        controller.handle_key(KeyCode::KeyW, true);
        controller.handle_key(KeyCode::KeyS, true);
        assert_eq!(controller.intent().forward, 0.0);
    }

    #[test]
    fn space_requests_a_jump() {
        let mut controller = PlayerController::new();
        controller.handle_key(KeyCode::Space, true);
        assert!(controller.intent().jump);
        controller.handle_key(KeyCode::Space, false);
        assert!(!controller.intent().jump);
    }

    #[test]
    fn mouse_look_is_accumulated_then_consumed() {
        let mut controller = PlayerController::new();
        controller.add_look(100.0, -50.0);
        let look = controller.take_look();

        assert!(look.0 < 0.0, "鼠标右移应向右转");
        assert!(look.1 > 0.0, "鼠标上移应抬头");
        assert_eq!(controller.take_look(), (0.0, 0.0));
    }

    #[test]
    fn release_keys_stop_movement() {
        let mut controller = PlayerController::new();
        controller.handle_key(KeyCode::KeyW, true);
        controller.handle_key(KeyCode::KeyW, false);
        assert_eq!(controller.intent(), MovementIntent::default());
    }

    #[test]
    fn reset_clears_every_input() {
        let mut controller = PlayerController::new();
        controller.handle_key(KeyCode::KeyA, true);
        controller.add_look(10.0, 10.0);
        controller.reset();
        assert_eq!(controller.intent(), MovementIntent::default());
        assert_eq!(controller.take_look(), (0.0, 0.0));
    }
}
