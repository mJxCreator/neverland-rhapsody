//! UI 模块：统一的界面状态、布局、控件、主题与绘制顶点生成。
//!
//! 分层结构：
//! - `manager`：界面状态机与输入优先级
//! - `input`：键盘/鼠标事件到界面意图的纯函数路由
//! - `theme` / `layout` / `draw` / `widget` / `font` / `animation`：共享视觉与绘制基础
//! - `hud` / `inventory` / `crafting` / `cooking` / `death`：各界面内容
//!
//! 该模块只生成 CPU 侧顶点数据，不创建任何 GPU 资源；渲染器把顶点写入共享缓冲后一次提交。

pub mod animation;
pub mod cooking;
pub mod crafting;
pub mod death;
pub mod draw;
pub mod font;
pub mod hud;
pub mod input;
pub mod inventory;
pub mod layout;
pub mod manager;
pub mod theme;
pub mod widget;

pub use draw::UiVertex;
pub use hud::HudModel;
pub use input::{ClickButton, UiIntent};
pub use manager::{UiManager, UiPointerAction, UiState};
