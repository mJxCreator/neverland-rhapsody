//! Neverland Rhapsody —— 基于 Rust 与 wgpu 的体素沙盒引擎。
//!
//! 当前里程碑 v0.17：独立 PNG 纹理、自动图集、GPU 纹理化渲染与物品图标绘制。

mod core;
mod entity;
mod interaction;
mod item;
mod mesh;
mod player;
mod renderer;
mod save;
mod texture;
mod ui;
mod world;

use anyhow::Result;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    init_tracing();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "启动 Neverland Rhapsody"
    );

    core::app::App::run(core::config::AppConfig::default())
}

/// 初始化日志：默认输出 `info` 级别，可通过 `RUST_LOG` 环境变量覆盖。
fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
}
