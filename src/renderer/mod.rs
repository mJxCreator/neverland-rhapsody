//! 渲染层：GPU 设备、渲染管线、摄像机、选中线框、HUD 与破坏粒子。

pub mod camera;
pub mod hud;
pub mod particles;
pub mod pipeline;
// 需求文档约定渲染主体位于 renderer/renderer.rs，保留同名子模块。
#[allow(clippy::module_inception)]
pub mod renderer;

pub use self::renderer::{spawn_pose, RenderOutcome, Renderer};
