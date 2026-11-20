//! 玩家输入控制器：采集键盘与鼠标事件，输出移动意图与视角增量。
//!
//! 控制器不直接修改玩家位置，最终位移统一由物理系统处理。
//! 冲刺由方向键双击触发（Shift 也可作为等效请求），松开全部方向键后结束。

use winit::keyboard::KeyCode;

/// 每像素鼠标位移对应的弧度。
const MOUSE_SENSITIVITY: f32 = 0.0022;
/// 双击方向键触发奔跑的最大间隔（秒）。
const DOUBLE_TAP_WINDOW: f32 = 0.3;

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
    shift_sprint: bool,
    /// 双击方向键后锁定的奔跑状态；全部方向键松开时解除。
    sprint_latched: bool,
    /// 控制器内部时钟，用于判定双击间隔。
    clock: f32,
    /// 每个方向键上一次按下的时刻。
    last_press: [Option<f32>; 4],
    look_delta: (f32, f32),
}

impl PlayerController {
    pub fn new() -> Self {
        Self::default()
    }

    /// 推进控制器时钟；主循环每帧调用，用于双击判定。
    pub fn tick(&mut self, delta_seconds: f32) {
        if delta_seconds.is_finite() && delta_seconds > 0.0 {
            self.clock += delta_seconds;
        }
    }

    /// 记录移动与跳跃键状态，其余按键忽略。
    pub fn handle_key(&mut self, code: KeyCode, pressed: bool) {
        if let Some(index) = direction_index(code) {
            let slot = self.direction_mut(index);
            let was_down = *slot;
            *slot = pressed;
            if pressed && !was_down {
                if let Some(last) = self.last_press[index] {
                    if self.clock - last <= DOUBLE_TAP_WINDOW {
                        self.sprint_latched = true;
                    }
                }
                self.last_press[index] = Some(self.clock);
            }
            if !pressed && !self.any_direction_held() {
                self.sprint_latched = false;
            }
            return;
        }

        match code {
            KeyCode::Space => self.jump = pressed,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => self.shift_sprint = pressed,
            _ => {}
        }
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

    /// 当前是否请求冲刺：双击方向键锁定或按住 Shift。
    pub fn sprint(&self) -> bool {
        self.sprint_latched || self.shift_sprint
    }

    /// 当前按键组合对应的移动意图。
    pub fn intent(&self) -> MovementIntent {
        MovementIntent {
            forward: axis(self.forward, self.backward),
            right: axis(self.right, self.left),
            jump: self.jump,
            sprint: self.sprint(),
        }
    }

    fn direction_mut(&mut self, index: usize) -> &mut bool {
        match index {
            0 => &mut self.forward,
            1 => &mut self.backward,
            2 => &mut self.left,
            _ => &mut self.right,
        }
    }

    fn any_direction_held(&self) -> bool {
        self.forward || self.backward || self.left || self.right
    }
}

/// 方向键在双击记录中的下标；非方向键返回 `None`。
fn direction_index(code: KeyCode) -> Option<usize> {
    match code {
        KeyCode::KeyW => Some(0),
        KeyCode::KeyS => Some(1),
        KeyCode::KeyA => Some(2),
        KeyCode::KeyD => Some(3),
        _ => None,
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
    fn single_tap_walks_and_double_tap_sprints() {
        let mut controller = PlayerController::new();
        controller.handle_key(KeyCode::KeyW, true);
        assert!(!controller.sprint(), "单击方向键只应行走");
        controller.handle_key(KeyCode::KeyW, false);

        controller.tick(0.1);
        controller.handle_key(KeyCode::KeyW, true);
        assert!(controller.sprint(), "窗口内双击应进入奔跑");
        assert!(controller.intent().sprint);

        controller.handle_key(KeyCode::KeyW, false);
        assert!(!controller.sprint(), "松开方向键应结束奔跑");
    }

    #[test]
    fn double_tap_outside_the_window_walks() {
        let mut controller = PlayerController::new();
        controller.handle_key(KeyCode::KeyW, true);
        controller.handle_key(KeyCode::KeyW, false);
        controller.tick(DOUBLE_TAP_WINDOW + 0.05);
        controller.handle_key(KeyCode::KeyW, true);
        assert!(!controller.sprint());
    }

    #[test]
    fn key_repeat_is_not_a_double_tap() {
        let mut controller = PlayerController::new();
        controller.handle_key(KeyCode::KeyW, true);
        controller.tick(0.05);
        controller.handle_key(KeyCode::KeyW, true);
        assert!(!controller.sprint(), "按住产生的重复事件不应触发奔跑");
    }

    #[test]
    fn shift_still_requests_sprint() {
        let mut controller = PlayerController::new();
        controller.handle_key(KeyCode::ShiftLeft, true);
        assert!(controller.sprint());
        controller.handle_key(KeyCode::ShiftLeft, false);
        assert!(!controller.sprint());
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
