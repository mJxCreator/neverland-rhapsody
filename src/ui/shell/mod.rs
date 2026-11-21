//! 界面外壳：整体界面状态机、互斥规则与输入/绘制路由。

pub mod manager;

pub use manager::{UiManager, UiPointerAction, UiState};
