//! 世界层：方块类型与属性、区块数据、程序化地形、动态世界容器与区块生命周期管理。

pub mod block;
pub mod block_registry;
pub mod chunk;
pub mod chunk_manager;
pub mod terrain;
// 需求文档约定世界主体位于 world/world.rs，保留同名子模块。
#[allow(clippy::module_inception)]
pub mod world;

pub use self::block::{Block, Face};
pub use self::block_registry::{DropRule, MiningCategory};
pub use self::chunk::{Chunk, ChunkPos, CHUNK_SIZE};
pub use self::chunk_manager::{ChunkConfig, ChunkManager};
pub use self::terrain::WORLD_HEIGHT;
pub use self::world::{World, DEFAULT_SEED};
