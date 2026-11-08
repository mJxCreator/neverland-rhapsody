//! 纹理资源系统：PNG 加载、纹理注册、图集打包、方块/物品映射与 GPU 资源。
//!
//! 各子模块职责分离：`texture_loader` 只负责解码与规格校验，
//! `texture_registry` 只维护稳定 ID，`texture_atlas` 只负责布局，
//! `texture_assets` 负责把资源装配成渲染层可直接消费的结果。
//!
//! 模块对渲染层与 UI 暴露的查询 API（物品图标、材质、UV 校验等）会有意
//! 保留给 v0.17 的背包/快捷栏绘制，因此允许部分当前未调用的公开接口。
#![allow(dead_code)]

pub mod block_texture;
pub mod gpu_texture;
pub mod item_texture;
pub mod material;
pub mod texture_assets;
pub mod texture_atlas;
pub mod texture_hot_reload;
pub mod texture_loader;
pub mod texture_registry;
pub mod types;
pub mod ui_texture;

pub use self::block_texture::BlockTextureMap;
pub use self::gpu_texture::{
    create_hud_bind_group, create_hud_bind_group_layout, create_texture_bind_group_layout,
    upload_atlas, upload_texture, upload_texture_image, GpuAtlasTexture, GpuTexture,
};
pub use self::item_texture::ItemTextureMap;
pub use self::texture_assets::{TextureAssets, DEFAULT_ASSET_ROOT};
pub use self::texture_hot_reload::TextureWatcher;
pub use self::ui_texture::UiTextureMap;
