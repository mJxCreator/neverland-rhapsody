//! 世界生成系统：现代多生物群系地形与旧版地形共存。
//!
//! 新存档使用 [`OverworldGenerator`]，旧存档按 [`LEGACY_WORLD_GEN_VERSION`]
//! 继续走原有高度图生成器，保证既有世界外观不变。

pub mod biome;
pub mod biome_registry;
pub mod cave;
pub mod config;
pub mod noise;
pub mod ore;
pub mod stats;
pub mod terrain;
pub mod vegetation;
pub mod water;

pub use config::{
    WorldGenConfig, CURRENT_WORLD_GEN_VERSION, LEGACY_WORLD_GEN_VERSION, SPAWN_COLUMN,
};
pub use stats::WorldGenStatsSnapshot;
pub use terrain::OverworldGenerator;

use crate::world::chunk::{Chunk, ChunkPos};
use crate::world::terrain::TerrainGenerator;

/// 世界生成器；按存档版本选择旧版或现代地形。
pub enum WorldGenerator {
    /// v0.20 及更早的高度图地形。
    Legacy(TerrainGenerator),
    /// v0.21 起的多生物群系地形。
    Modern(Box<OverworldGenerator>),
}

impl WorldGenerator {
    /// 按版本创建生成器；未知版本回退到现代地形。
    pub fn new(seed: u32, version: u32) -> Self {
        match version {
            LEGACY_WORLD_GEN_VERSION => Self::Legacy(TerrainGenerator::new(seed)),
            _ => Self::Modern(Box::new(OverworldGenerator::new(
                seed,
                WorldGenConfig::default(),
            ))),
        }
    }

    /// 该列地表高度。
    pub fn height_at(&self, x: i32, z: i32) -> i32 {
        match self {
            Self::Legacy(generator) => generator.height_at(x, z),
            Self::Modern(generator) => generator.height_at(x, z),
        }
    }

    /// 该列生物群系；旧版地形只有草地，按平原处理。
    pub fn biome_at(&self, x: i32, z: i32) -> biome::BiomeId {
        match self {
            Self::Legacy(_) => biome::BiomeId::Plains,
            Self::Modern(generator) => generator.biome_at(x, z),
        }
    }

    /// 生成区块。
    pub fn generate_chunk(&self, pos: ChunkPos) -> Chunk {
        match self {
            Self::Legacy(generator) => generator.generate_chunk(pos),
            Self::Modern(generator) => generator.generate_chunk(pos),
        }
    }

    /// 生成统计；旧版地形不统计。
    pub fn stats(&self) -> WorldGenStatsSnapshot {
        match self {
            Self::Legacy(_) => WorldGenStatsSnapshot::default(),
            Self::Modern(generator) => generator.stats(),
        }
    }
}
