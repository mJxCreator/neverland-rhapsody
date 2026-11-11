//! 世界生成集中配置：版本、海平面、洞穴、矿脉与植被参数。
//!
//! 所有影响地形的数值都从这里读取，避免生成器内部散落魔法数。

use crate::world::block::Block;

/// v0.20 及更早存档使用的地形版本；旧存档必须继续生成原样地形。
pub const LEGACY_WORLD_GEN_VERSION: u32 = 1;
/// 当前版本（v0.21）的多生物群系地形。
pub const CURRENT_WORLD_GEN_VERSION: u32 = 2;

/// 静态水面基准高度。
pub const SEA_LEVEL: i32 = 7;
/// 出生点所在列；与渲染层的出生位置保持一致。
pub const SPAWN_COLUMN: (i32, i32) = (48, 84);
/// 出生点周围不生成树木的半径（切比雪夫距离）。
pub const SPAWN_CLEAR_RADIUS: i32 = 10;

/// 洞穴生成参数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaveConfig {
    /// 生成频率；数值越大洞穴越密集。
    pub frequency: f64,
    /// 判定阈值，越小洞穴越多。
    pub threshold: f64,
    /// 最低可挖空高度。
    pub min_y: i32,
    /// 距离地表的最小保留层数，防止洞穴直接破开地表。
    pub surface_margin: i32,
}

impl Default for CaveConfig {
    fn default() -> Self {
        Self {
            frequency: 0.055,
            threshold: 0.92,
            min_y: 2,
            surface_margin: 4,
        }
    }
}

/// 单类矿脉的生成参数。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OreVeinConfig {
    /// 矿脉方块。
    pub block: Block,
    /// 每个区块的候选矿脉数量。
    pub attempts: u32,
    /// 矿脉最低高度。
    pub min_y: i32,
    /// 矿脉最高高度。
    pub max_y: i32,
    /// 单条矿脉最少方块数。
    pub min_size: u32,
    /// 单条矿脉最多方块数。
    pub max_size: u32,
}

impl OreVeinConfig {
    /// 在该区块内确定性地生成一条矿脉的方块位置。
    pub fn vein_size(&self, roll: u32) -> u32 {
        self.min_size + roll % (self.max_size - self.min_size + 1)
    }
}

/// 矿石参数集中在配置中，生成器只读取不硬编码。
pub const DEFAULT_ORES: [OreVeinConfig; 4] = [
    OreVeinConfig {
        block: Block::CoalOre,
        attempts: 14,
        min_y: 4,
        max_y: 24,
        min_size: 5,
        max_size: 9,
    },
    OreVeinConfig {
        block: Block::IronOre,
        attempts: 8,
        min_y: 3,
        max_y: 20,
        min_size: 4,
        max_size: 7,
    },
    OreVeinConfig {
        block: Block::GoldOre,
        attempts: 4,
        min_y: 2,
        max_y: 14,
        min_size: 3,
        max_size: 5,
    },
    OreVeinConfig {
        block: Block::DiamondOre,
        attempts: 2,
        min_y: 1,
        max_y: 8,
        min_size: 2,
        max_size: 4,
    },
];

/// 地表植被生成参数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VegetationConfig {
    /// 草方块上每列生成高草的基础概率系数。
    pub grass_scale: f64,
    /// 草方块上每列生成花的基础概率系数。
    pub flower_scale: f64,
    /// 沙地上每列生成仙人掌的概率。
    pub cactus_chance: f64,
}

impl Default for VegetationConfig {
    fn default() -> Self {
        Self {
            grass_scale: 0.32,
            flower_scale: 0.09,
            cactus_chance: 0.012,
        }
    }
}

/// 世界生成总配置。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldGenConfig {
    pub sea_level: i32,
    pub spawn_clear_radius: i32,
    pub cave: CaveConfig,
    pub ores: [OreVeinConfig; 4],
    pub vegetation: VegetationConfig,
}

impl Default for WorldGenConfig {
    fn default() -> Self {
        Self {
            sea_level: SEA_LEVEL,
            spawn_clear_radius: SPAWN_CLEAR_RADIUS,
            cave: CaveConfig::default(),
            ores: DEFAULT_ORES,
            vegetation: VegetationConfig::default(),
        }
    }
}
