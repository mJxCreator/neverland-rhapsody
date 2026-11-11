//! 静态水体：低洼地形按低频湖泊噪声填充到统一水面高度。
//!
//! 水没有流动模拟；判定只依赖列位置、群系水概率与地形高度，因此区块边界一致。

use super::biome::BiomeId;
use super::config::WorldGenConfig;
use super::noise::NoiseField;

/// 水体生成器。
pub struct WaterGenerator {
    lakes: NoiseField,
    config: WorldGenConfig,
}

impl WaterGenerator {
    pub fn new(seed: u32, config: WorldGenConfig) -> Self {
        Self {
            lakes: NoiseField::new(seed ^ 0x0A7E_0001, 3, 0.015, 0.55),
            config,
        }
    }

    /// 该列的水面高度；`None` 表示没有水体。
    ///
    /// 群系水概率越高，触发湖泊所需的噪声阈值越低；沙漠与山地几乎不积水。
    pub fn level_at(&self, x: i32, z: i32, biome: BiomeId, terrain_height: i32) -> Option<i32> {
        let probability = biome.definition().water_probability;
        if probability <= 0.0 {
            return None;
        }
        let lake = self.lakes.get2(x as f64, z as f64);
        let threshold = 0.15 - probability * 0.1;
        if lake <= threshold {
            return None;
        }
        let rise = ((lake - threshold) * 6.0).round() as i32;
        let level =
            (self.config.sea_level + rise).clamp(1, super::super::terrain::WORLD_HEIGHT - 2);
        (level > terrain_height).then_some(level)
    }
}
