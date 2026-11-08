//! 渲染层：GPU 设备、渲染管线、摄像机、选中线框、屏幕空间 UI 顶点提交与破坏粒子。
//!
//! UI 的布局与顶点生成位于 `crate::ui`；本模块只负责把它们写入
//! 共享的动态顶点缓冲并调用 HUD 管线，不重复实现界面逻辑。

pub mod camera;
pub mod model;
pub mod particles;
pub mod pipeline;
// 需求文档约定渲染主体位于 renderer/renderer.rs，保留同名子模块。
#[allow(clippy::module_inception)]
pub mod renderer;

pub use self::renderer::{spawn_pose, RenderOutcome, Renderer};
