//! 跨界面共享的 UI 基础设施：布局、绘制、字体、主题、贴图比例与通用控件。
//!
//! 业务模块（`hotbar`、`state`、`inventory` 等）只依赖本层，不互相引用内部实现。

pub mod animation;
pub mod draw;
pub mod font;
pub mod input;
pub mod layout;
pub mod texture_ratio;
pub mod theme;
pub mod widget;
