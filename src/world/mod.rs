//! 世界层：方块类型、区块数据、程序化地形与世界容器。

pub mod block;
pub mod chunk;
pub mod terrain;
// 需求文档约定世界主体位于 world/world.rs，保留同名子模块。
#[allow(clippy::module_inception)]
pub mod world;

pub use self::block::{Block, Face};
pub use self::chunk::{Chunk, CHUNK_SIZE};
pub use self::world::World;
