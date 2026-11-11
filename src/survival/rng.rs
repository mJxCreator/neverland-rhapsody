//! 可复现的游戏随机数。
//!
//! 生存系统中的概率事件（例如腐肉导致饥饿状态）必须使用固定种子的随机数，
//! 且调用顺序稳定，才能保证同一存档在相同操作序列下得到一致结果。渲染层与
//! 界面层不得自行获取随机数。

/// 基于 xorshift64* 的确定性随机数生成器。
#[derive(Clone, Copy, Debug)]
pub struct GameRng {
    state: u64,
}

impl GameRng {
    /// 以种子创建；种子为 0 时替换为非零常量，避免退化为全零序列。
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    /// 生成下一个 64 位随机数。
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// 生成 `[0, 1)` 之间的随机浮点数。
    pub fn next_f32(&mut self) -> f32 {
        let bits = (self.next_u64() >> 40) as u32;
        bits as f32 / (1u32 << 24) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::GameRng;

    #[test]
    fn same_seed_produces_same_sequence() {
        let mut a = GameRng::new(42);
        let mut b = GameRng::new(42);
        for _ in 0..16 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn zero_seed_still_advances() {
        let mut rng = GameRng::new(0);
        let first = rng.next_u64();
        let second = rng.next_u64();
        assert_ne!(first, 0);
        assert_ne!(first, second);
    }

    #[test]
    fn float_output_stays_in_unit_range() {
        let mut rng = GameRng::new(7);
        for _ in 0..1000 {
            let value = rng.next_f32();
            assert!((0.0..1.0).contains(&value), "越界随机值: {value}");
        }
    }
}
