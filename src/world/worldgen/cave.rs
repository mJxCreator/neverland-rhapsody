//! 3D 噪声洞穴。
//!
//! 洞穴直接在区块内的方块上挖空，噪声按世界坐标采样，因此跨区块天然连续。
//! 判定会保留地表以下若干层，避免洞穴破坏地表与湖床。

use super::config::CaveConfig;
use super::noise::NoiseField;

/// 洞穴生成器。
pub struct CaveGenerator {
    primary: NoiseField,
    secondary: NoiseField,
    config: CaveConfig,
}

impl CaveGenerator {
    pub fn new(seed: u32, config: CaveConfig) -> Self {
        Self {
            primary: NoiseField::new(seed ^ 0xCA0E_0001, 3, config.frequency, 0.5),
            secondary: NoiseField::new(seed ^ 0xCA0E_0002, 2, config.frequency, 0.5),
            config,
        }
    }

    /// 该世界坐标是否应被挖空。
    pub fn is_cave(&self, x: i32, y: i32, z: i32) -> bool {
        if y < self.config.min_y {
            return false;
        }
        let p = self.primary.get3(x as f64, y as f64 * 1.4, z as f64);
        let s = self.secondary.get3(x as f64, y as f64 * 1.4, z as f64);
        let tunnel = (1.0 - p.abs()).powi(6) + (1.0 - s.abs()).powi(6);
        tunnel > self.config.threshold
    }

    /// 距离地表至少保留的层数。
    pub fn surface_margin(&self) -> i32 {
        self.config.surface_margin
    }
}
