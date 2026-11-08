//! 模型资源系统 v0.18：GLB 导入、网格/材质、骨骼蒙皮、动画播放与实例化。
//!
//! 设计要点：
//! - [`types::ModelAsset`] 是可共享的静态资源，[`instance::ModelInstance`] 保存每个实例的动画状态与姿态；
//! - 蒙皮采用 CPU 变形：生物顶点量小，便于单元测试与调试，并且每帧只更新蒙皮实例的顶点；
//! - 静态网格由渲染器按（模型、网格、图元）缓存 GPU 缓冲，多个实例只更新世界变换；
//! - 全部动画数据按秒存储，姿态与时间推进和帧率无关。

// 导入/校验 API 覆盖完整 GLB 规范，部分访问器暂未在游戏逻辑中调用。
#![allow(dead_code)]

pub mod animation;
pub mod builtin;
pub mod glb;
pub mod instance;
pub mod loader;
pub mod material;
pub mod mesh;
pub mod registry;
pub mod render;
pub mod skeleton;
pub mod skin;
pub mod types;

pub use self::instance::ModelInstance;
pub use self::material::AlphaMode;
pub use self::mesh::MeshBounds;
pub use self::registry::ModelRegistry;
pub use self::render::{instance_draws, ModelDraw, ModelGeometry, ModelVertex};
pub use self::types::{ModelAsset, ModelId};

/// 模型加载失败；面向日志与调用的可读错误，不暴露底层解析细节。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelLoadError {
    message: String,
}

impl ModelLoadError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn format(message: impl Into<String>) -> Self {
        Self::new(message)
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(format!("不支持的特性: {}", message.into()))
    }

    pub fn io(path: &std::path::Path, error: std::io::Error) -> Self {
        Self::new(format!("读取 {} 失败: {error}", path.display()))
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for ModelLoadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ModelLoadError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_are_readable_and_comparable() {
        let error = ModelLoadError::unsupported("稀疏 Accessor");
        assert!(error.to_string().contains("不支持"));
        assert_eq!(error, ModelLoadError::new(error.message()));
    }
}
