//! 世界生成耗时统计。
//!
//! 生成器以只读方式被调用，因此计数器使用 `Cell` 做内部可变统计。

use std::cell::Cell;

/// 可读取的统计快照。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldGenStatsSnapshot {
    pub chunks: u64,
    pub total_ns: u64,
    pub terrain_ns: u64,
    pub cave_ns: u64,
    pub ore_ns: u64,
    pub decoration_ns: u64,
}

impl WorldGenStatsSnapshot {
    /// 单区块平均耗时（毫秒）。
    pub fn average_ms(&self) -> f64 {
        if self.chunks == 0 {
            0.0
        } else {
            self.total_ns as f64 / self.chunks as f64 / 1_000_000.0
        }
    }

    /// 指定阶段的平均耗时（毫秒）。
    pub fn stage_average_ms(&self, stage_ns: u64) -> f64 {
        if self.chunks == 0 {
            0.0
        } else {
            stage_ns as f64 / self.chunks as f64 / 1_000_000.0
        }
    }
}

/// 生成期间的累计计数器。
#[derive(Debug, Default)]
pub struct WorldGenStats {
    chunks: Cell<u64>,
    total_ns: Cell<u64>,
    terrain_ns: Cell<u64>,
    cave_ns: Cell<u64>,
    ore_ns: Cell<u64>,
    decoration_ns: Cell<u64>,
}

impl WorldGenStats {
    /// 记录一个区块的各阶段耗时。
    pub fn record_chunk(
        &self,
        total_ns: u64,
        terrain_ns: u64,
        cave_ns: u64,
        ore_ns: u64,
        decoration_ns: u64,
    ) {
        self.chunks.set(self.chunks.get() + 1);
        self.total_ns.set(self.total_ns.get() + total_ns);
        self.terrain_ns.set(self.terrain_ns.get() + terrain_ns);
        self.cave_ns.set(self.cave_ns.get() + cave_ns);
        self.ore_ns.set(self.ore_ns.get() + ore_ns);
        self.decoration_ns
            .set(self.decoration_ns.get() + decoration_ns);
    }

    /// 读取当前累计值。
    pub fn snapshot(&self) -> WorldGenStatsSnapshot {
        WorldGenStatsSnapshot {
            chunks: self.chunks.get(),
            total_ns: self.total_ns.get(),
            terrain_ns: self.terrain_ns.get(),
            cave_ns: self.cave_ns.get(),
            ore_ns: self.ore_ns.get(),
            decoration_ns: self.decoration_ns.get(),
        }
    }
}
