//! 可复现噪声与确定性随机数工具。

use noise::{Fbm, MultiFractal, NoiseFn, OpenSimplex};

/// 带固定频率的分形噪声采样器。
pub struct NoiseField {
    noise: Fbm<OpenSimplex>,
}

impl NoiseField {
    /// 创建 2D/3D 通用的分形噪声。
    pub fn new(seed: u32, octaves: usize, frequency: f64, persistence: f64) -> Self {
        Self {
            noise: Fbm::<OpenSimplex>::new(seed)
                .set_octaves(octaves)
                .set_frequency(frequency)
                .set_persistence(persistence),
        }
    }

    /// 2D 采样，返回约 -1..1。
    pub fn get2(&self, x: f64, z: f64) -> f64 {
        self.noise.get([x, z])
    }

    /// 3D 采样，返回约 -1..1。
    pub fn get3(&self, x: f64, y: f64, z: f64) -> f64 {
        self.noise.get([x, y, z])
    }
}

/// SplitMix64：稳定、快速的确定性散列。
pub fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut mixed = value;
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    mixed ^ (mixed >> 31)
}

/// 把世界种子与整数坐标混合成 [0,1) 的确定性随机值。
pub fn hash01(seed: u32, x: i32, z: i32, salt: u64) -> f64 {
    let packed = (x as i64 as u64 & 0xFFFF_FFFF) | ((z as i64 as u64 & 0xFFFF_FFFF) << 32);
    let value = splitmix64(
        (seed as u64) ^ packed.rotate_left(17) ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15),
    );
    (value >> 11) as f64 / (1u64 << 53) as f64
}

/// 小型确定性随机数发生器，用于矿脉走向等连续随机过程。
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self {
            state: splitmix64(seed),
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = splitmix64(self.state);
        self.state
    }

    /// [0, exclusive) 整数，exclusive 必须大于 0。
    pub fn below(&mut self, exclusive: u32) -> u32 {
        (self.next_u64() % exclusive as u64) as u32
    }

    /// -1..=1 的整数步长。
    pub fn step(&mut self) -> i32 {
        self.below(3) as i32 - 1
    }
}

/// 平滑阶梯函数。
pub fn smoothstep(edge0: f64, edge1: f64, value: f64) -> f64 {
    if (edge1 - edge0).abs() < f64::EPSILON {
        return if value >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_reproducible_and_distributed() {
        let a = hash01(42, 3, -7, 11);
        let b = hash01(42, 3, -7, 11);
        assert_eq!(a, b);
        assert!((0.0..1.0).contains(&a));
        assert_ne!(hash01(42, 3, -7, 12), a);
    }

    #[test]
    fn rng_is_deterministic() {
        let mut first = Rng::new(7);
        let mut second = Rng::new(7);
        for _ in 0..8 {
            assert_eq!(first.next_u64(), second.next_u64());
        }
    }
}
