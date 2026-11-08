//! UI 输入路由：把键盘与鼠标事件翻译为界面意图，保证一次输入只触发一个操作。
//!
//! 路由保持纯函数特征：不读取全局状态、不修改界面，由 [`UiManager`] 执行结果。
//! 这样死亡界面最高优先级、背包与合成互斥等规则可以被单元测试覆盖。

use winit::keyboard::KeyCode;

use super::manager::UiState;

/// 统一的鼠标按键。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickButton {
    Left,
    Right,
}

/// 键盘路由出的界面意图；`None` 表示该按键不属于 UI 层，继续交给游戏层。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiIntent {
    OpenInventory,
    CloseInventory,
    OpenCrafting,
    CloseCrafting,
    ReleaseCursor,
    Quit,
}

/// 根据当前界面状态解析按键。
///
/// 规则：
/// - 死亡界面吞掉所有 UI 快捷键，仅允许鼠标操作。
/// - `E` 只在游戏内打开背包、在背包内关闭；合成中不响应。
/// - `C` 只在游戏内打开合成、在合成内关闭；背包中不响应。
/// - `Esc` 关闭当前全屏界面；无界面时先释放鼠标，再退出游戏。
/// - 只在按下事件产生意图，避免按键释放重复触发。
pub fn route_key(
    state: UiState,
    code: KeyCode,
    pressed: bool,
    cursor_grabbed: bool,
) -> Option<UiIntent> {
    if !pressed {
        return None;
    }
    match state {
        UiState::Death => None,
        UiState::Crafting => match code {
            KeyCode::Escape => Some(UiIntent::CloseCrafting),
            KeyCode::KeyC => Some(UiIntent::CloseCrafting),
            _ => None,
        },
        UiState::Inventory => match code {
            KeyCode::Escape => Some(UiIntent::CloseInventory),
            KeyCode::KeyE => Some(UiIntent::CloseInventory),
            KeyCode::KeyC => Some(UiIntent::CloseInventory),
            _ => None,
        },
        UiState::Gameplay => match code {
            KeyCode::Escape => Some(if cursor_grabbed {
                UiIntent::ReleaseCursor
            } else {
                UiIntent::Quit
            }),
            KeyCode::KeyE => Some(UiIntent::OpenInventory),
            KeyCode::KeyC => Some(UiIntent::OpenCrafting),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_closes_only_the_active_screen() {
        assert_eq!(
            route_key(UiState::Inventory, KeyCode::Escape, true, false),
            Some(UiIntent::CloseInventory)
        );
        assert_eq!(
            route_key(UiState::Crafting, KeyCode::Escape, true, false),
            Some(UiIntent::CloseCrafting)
        );
        assert_eq!(
            route_key(UiState::Gameplay, KeyCode::Escape, true, true),
            Some(UiIntent::ReleaseCursor)
        );
        assert_eq!(
            route_key(UiState::Gameplay, KeyCode::Escape, true, false),
            Some(UiIntent::Quit)
        );
    }

    #[test]
    fn inventory_and_crafting_do_not_chain() {
        assert_eq!(
            route_key(UiState::Inventory, KeyCode::KeyC, true, false),
            Some(UiIntent::CloseInventory)
        );
        assert_eq!(
            route_key(UiState::Crafting, KeyCode::KeyE, true, false),
            None
        );
        assert_eq!(
            route_key(UiState::Gameplay, KeyCode::KeyE, true, true),
            Some(UiIntent::OpenInventory)
        );
        assert_eq!(
            route_key(UiState::Gameplay, KeyCode::KeyC, true, true),
            Some(UiIntent::OpenCrafting)
        );
    }

    #[test]
    fn death_blocks_every_ui_shortcut() {
        for code in [KeyCode::Escape, KeyCode::KeyE, KeyCode::KeyC] {
            assert_eq!(route_key(UiState::Death, code, true, true), None);
            assert_eq!(route_key(UiState::Death, code, true, false), None);
        }
    }

    #[test]
    fn key_release_never_triggers_ui() {
        assert_eq!(
            route_key(UiState::Gameplay, KeyCode::KeyE, false, true),
            None
        );
        assert_eq!(
            route_key(UiState::Inventory, KeyCode::Escape, false, false),
            None
        );
    }
}
