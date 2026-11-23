//! UI 模块：按“一个视觉/交互模块一个目录”组织。
//!
//! 目录约定：
//! - `core`：跨模块共享的基础设施（布局、绘制、字体、主题、输入、通用控件）。
//! - `shell`：整体界面状态机与输入/绘制路由。
//! - `hud`：游戏内 HUD 组合层，只按顺序调用各子模块，不写具体视觉。
//! - `hotbar` / `state` / `crosshair` / `progress` / `target` / `damage` /
//!   `notifications` / `time`：HUD 子模块，各自持有状态与 `config` 参数。
//! - `inventory` / `crafting` / `cooking` / `death`：全屏界面模块。
//!
//! 程序员调整某个界面的尺寸、间距或配色时，只需打开对应目录下的 `config.rs`
//! 或模块内的 `Config` 结构，不需要在多个文件之间查找散落常数。

pub mod cooking;
pub mod core;
pub mod crafting;
pub mod crosshair;
pub mod damage;
pub mod death;
pub mod hotbar;
pub mod hud;
pub mod inventory;
pub mod menu;
pub mod notifications;
pub mod progress;
pub mod shell;
pub mod state;
pub mod target;
pub mod time;

// 兼容既有调用路径：外部只依赖输入路由与贴图比例契约。
pub use core::{input, texture_ratio};

pub use core::draw::UiVertex;
pub use hud::HudModel;
pub use input::{ClickButton, UiIntent};
pub use menu::{MenuAction, MenuEntry, MenuUi};
pub use shell::{UiManager, UiPointerAction, UiState};
