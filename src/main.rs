//! Neverland Rhapsody —— 基于 Rust 与 wgpu 的体素沙盒引擎。
//!
//! 当前里程碑 v0.22：角色建模、装备可视化与第一/第三人称视角。

mod attachment;
mod combat;
mod core;
mod entity;
mod equipment;
mod equipment_visual;
mod interaction;
mod item;
mod mesh;
mod model;
mod player;
mod player_visual;
mod renderer;
mod save;
mod survival;
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
