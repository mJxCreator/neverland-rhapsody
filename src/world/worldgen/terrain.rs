//! 多生物群系地形生成：温度/湿度权重混合高度，叠加山脊与湖泊。
//!
//! 生成顺序：地形与分层 → 静态水体 → 洞穴 → 矿脉 → 树木 → 地表植被。
//! 所有采样都基于世界坐标，区块之间无需共享中间状态。

use std::time::Instant;

use crate::world::block::Block;
use crate::world::chunk::{Chunk, ChunkPos, CHUNK_SIZE};
use crate::world::terrain::WORLD_HEIGHT;

use super::biome::{BiomeId, ColumnSurface};
use super::cave::CaveGenerator;
use super::config::WorldGenConfig;
use super::noise::{smoothstep, NoiseField};
use super::ore::OreGenerator;
use super::stats::{WorldGenStats, WorldGenStatsSnapshot};
use super::vegetation::VegetationGenerator;
use super::water::WaterGenerator;

/// 现代（v0.21）地形生成器。
pub struct OverworldGenerator {
    config: WorldGenConfig,
    continent: NoiseField,
    detail: NoiseField,
    ridge: NoiseField,
    temperature: NoiseField,
    humidity: NoiseField,
    water: WaterGenerator,
    caves: CaveGenerator,
    ores: OreGenerator,
    vegetation: VegetationGenerator,
    stats: WorldGenStats,
}

impl OverworldGenerator {
    pub fn new(seed: u32, config: WorldGenConfig) -> Self {
        Self {
            config,
            continent: NoiseField::new(seed ^ 0x0000_1000, 2, 0.004, 0.5),
            detail: NoiseField::new(seed ^ 0x0000_1001, 4, 0.03, 0.5),
            ridge: NoiseField::new(seed ^ 0x0000_1002, 3, 0.012, 0.5),
            temperature: NoiseField::new(seed ^ 0x0000_1003, 2, 0.0045, 0.5),
            humidity: NoiseField::new(seed ^ 0x0000_1004, 2, 0.006, 0.5),
            water: WaterGenerator::new(seed, config),
            caves: CaveGenerator::new(seed, config.cave),
            ores: OreGenerator::new(seed, config.ores),
            vegetation: VegetationGenerator::new(seed, config.vegetation),
            stats: WorldGenStats::default(),
        }
    }

    /// 读取累计生成耗时。
    pub fn stats(&self) -> WorldGenStatsSnapshot {
        self.stats.snapshot()
    }

    /// 该列地表最高实心方块的高度。
    pub fn height_at(&self, x: i32, z: i32) -> i32 {
        self.sample(x, z).height
    }

    /// 该列主导生物群系。
    pub fn biome_at(&self, x: i32, z: i32) -> BiomeId {
        self.sample(x, z).biome
    }

    /// 生成一个区块。
    pub fn generate_chunk(&self, pos: ChunkPos) -> Chunk {
        let started = Instant::now();
        let mut chunk = Chunk::new(pos);
        let local_origin_y = pos.y * CHUNK_SIZE;

        let terrain_started = Instant::now();
        let mut heights = [0i32; (CHUNK_SIZE * CHUNK_SIZE) as usize];
        for lz in 0..CHUNK_SIZE {
            for lx in 0..CHUNK_SIZE {
                let wx = pos.x * CHUNK_SIZE + lx;
                let wz = pos.z * CHUNK_SIZE + lz;
                let surface = self.sample(wx, wz);
                heights[(lz * CHUNK_SIZE + lx) as usize] = surface.height;
                self.fill_column(&mut chunk, pos, surface, lx, lz);
            }
        }
        let terrain_ns = terrain_started.elapsed().as_nanos() as u64;

        let cave_started = Instant::now();
        for lz in 0..CHUNK_SIZE {
            for lx in 0..CHUNK_SIZE {
                let height = heights[(lz * CHUNK_SIZE + lx) as usize];
                let wx = pos.x * CHUNK_SIZE + lx;
                let wz = pos.z * CHUNK_SIZE + lz;
                let top = height - self.caves.surface_margin();
                for wy in self.config.cave.min_y..=top {
                    let ly = wy - local_origin_y;
                    if !(0..CHUNK_SIZE).contains(&ly) {
                        continue;
                    }
                    if self.caves.is_cave(wx, wy, wz) {
                        chunk.set_block(lx, ly, lz, Block::Air);
                    }
                }
            }
        }
        let cave_ns = cave_started.elapsed().as_nanos() as u64;

        let ore_started = Instant::now();
        self.ores.generate(pos, &mut chunk);
        let ore_ns = ore_started.elapsed().as_nanos() as u64;

        let decoration_started = Instant::now();
        let column = |x: i32, z: i32| self.sample(x, z);
        self.vegetation.generate_trees(pos, &mut chunk, column);
        let column = |x: i32, z: i32| self.sample(x, z);
        self.vegetation
            .generate_ground_cover(pos, &mut chunk, column);
        let decoration_ns = decoration_started.elapsed().as_nanos() as u64;

        let total_ns = started.elapsed().as_nanos() as u64;
        self.stats
            .record_chunk(total_ns, terrain_ns, cave_ns, ore_ns, decoration_ns);
        chunk
    }

    /// 单列采样：气候权重、混合高度与水面。
    fn sample(&self, x: i32, z: i32) -> ColumnSurface {
        let temperature = self.temperature.get2(x as f64, z as f64);
        let humidity = self.humidity.get2(x as f64, z as f64);
        let weights = biome_weights(temperature, humidity);
        let detail = self.detail.get2(x as f64, z as f64);
        let continent = self.continent.get2(x as f64, z as f64);
        let ridge = (1.0 - self.ridge.get2(x as f64, z as f64).abs()).powi(3);

        // 低频起伏形成谷地与高地，湖泊才能在低洼处自然积水。
        let mut height = continent * 3.2;
        for (index, biome) in BiomeId::ALL.iter().enumerate() {
            let definition = biome.definition();
            height +=
                weights[index] * (definition.base_height + detail * definition.height_amplitude);
        }
        height += weights[BiomeId::Mountains.index()] * ridge * 7.0;
        let height = (height.round() as i32).clamp(2, WORLD_HEIGHT - 3);

        let biome = dominant_biome(&weights);
        let water_level = self.water.level_at(x, z, biome, height);
        ColumnSurface {
            height,
            biome,
            water_level,
        }
    }

    fn fill_column(
        &self,
        chunk: &mut Chunk,
        pos: ChunkPos,
        surface: ColumnSurface,
        lx: i32,
        lz: i32,
    ) {
        let definition = surface.biome.definition();
        let shore = surface
            .water_level
            .is_some_and(|level| surface.height <= level + 1);
        let surface_block = if shore {
            Block::Sand
        } else {
            definition.surface
        };
        let subsurface_block = if shore {
            Block::Sand
        } else {
            definition.subsurface
        };
        let local_origin_y = pos.y * CHUNK_SIZE;

        for wy in 0..surface.height {
            let ly = wy - local_origin_y;
            if !(0..CHUNK_SIZE).contains(&ly) {
                continue;
            }
            let block = if wy == surface.height - 1 {
                surface_block
            } else if wy >= surface.height - 4 {
                subsurface_block
            } else {
                definition.stone
            };
            chunk.set_block(lx, ly, lz, block);
        }

        if let Some(level) = surface.water_level {
            for wy in surface.height..=level {
                let ly = wy - local_origin_y;
                if (0..CHUNK_SIZE).contains(&ly) {
                    chunk.set_block(lx, ly, lz, Block::Water);
                }
            }
        }
    }
}

/// 由温度与湿度计算四种群系的权重，并归一化。
pub fn biome_weights(temperature: f64, humidity: f64) -> [f64; 4] {
    let mountains = smoothstep(0.18, 0.55, -temperature);
    let desert =
        smoothstep(0.0, 0.4, temperature) * smoothstep(0.15, -0.25, humidity) * (1.0 - mountains);
    let forest = smoothstep(-0.1, 0.45, humidity) * (1.0 - desert) * (1.0 - mountains);
    let plains = (1.0 - mountains - desert - forest).max(0.05);

    let mut weights = [plains, forest, desert, mountains];
    let sum: f64 = weights.iter().sum();
    if sum > 0.0 {
        for weight in &mut weights {
            *weight /= sum;
        }
    }
    weights
}

fn dominant_biome(weights: &[f64; 4]) -> BiomeId {
    let mut best = BiomeId::Plains;
    let mut best_weight = weights[0];
    for biome in BiomeId::ALL.iter().skip(1) {
        if weights[biome.index()] > best_weight {
            best = *biome;
            best_weight = weights[biome.index()];
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::worldgen::config::WorldGenConfig;

    fn generator(seed: u32) -> OverworldGenerator {
        OverworldGenerator::new(seed, WorldGenConfig::default())
    }

    #[test]
    fn heights_stay_inside_the_world() {
        let generator = generator(7);
        for z in (-128..=128).step_by(8) {
            for x in (-128..=128).step_by(8) {
                let height = generator.height_at(x, z);
                assert!(
                    (2..=WORLD_HEIGHT - 3).contains(&height),
                    "高度越界: {height}"
                );
            }
        }
    }

    #[test]
    fn all_four_biomes_appear() {
        let generator = generator(2026);
        let mut seen = [false; 4];
        for z in (-512..=512).step_by(16) {
            for x in (-512..=512).step_by(16) {
                seen[generator.biome_at(x, z).index()] = true;
            }
        }
        assert!(
            seen.iter().all(|value| *value),
            "四种群系都应出现: {seen:?}"
        );
    }

    #[test]
    fn biome_edges_keep_height_continuous() {
        let generator = generator(31);
        for z in (-64..=64).step_by(4) {
            let mut previous = generator.height_at(-64, z);
            for x in -63..=64 {
                let current = generator.height_at(x, z);
                assert!(
                    (current - previous).abs() <= 4,
                    "相邻列高度跳变过大: {previous} -> {current}"
                );
                previous = current;
            }
        }
    }

    #[test]
    fn chunks_are_deterministic_and_can_contain_water() {
        let generator = generator(99);
        let mut water_seen = false;
        for cx in -4..=4 {
            for cz in -4..=4 {
                let pos = ChunkPos::new(cx, 0, cz);
                let first = generator.generate_chunk(pos);
                let second = generator.generate_chunk(pos);
                for y in 0..CHUNK_SIZE {
                    for z in 0..CHUNK_SIZE {
                        for x in 0..CHUNK_SIZE {
                            let block = first.block_at(x, y, z);
                            assert_eq!(block, second.block_at(x, y, z));
                            water_seen |= block == Block::Water;
                        }
                    }
                }
            }
        }
        assert!(water_seen, "生成范围内应出现湖泊");
        assert!(generator.stats().chunks > 0);
        assert!(generator.stats().average_ms() < 250.0);
    }

    #[test]
    fn caves_carve_underground_and_keep_the_surface_intact() {
        let generator = generator(99);
        let mut cave_blocks = 0;
        for cx in -2..=2 {
            for cz in -2..=2 {
                let pos = ChunkPos::new(cx, 0, cz);
                let chunk = generator.generate_chunk(pos);
                for lz in 0..CHUNK_SIZE {
                    for lx in 0..CHUNK_SIZE {
                        let wx = pos.x * CHUNK_SIZE + lx;
                        let wz = pos.z * CHUNK_SIZE + lz;
                        let height = generator.height_at(wx, wz);
                        if height <= CHUNK_SIZE {
                            assert_ne!(
                                chunk.block_at(lx, height - 1, lz),
                                Block::Air,
                                "地表方块不能被洞穴破坏"
                            );
                        }
                        for wy in 2..(height - 4).min(CHUNK_SIZE) {
                            if chunk.block_at(lx, wy, lz) == Block::Air {
                                cave_blocks += 1;
                            }
                        }
                    }
                }
            }
        }
        assert!(cave_blocks > 0, "地下应生成洞穴");
    }

    #[test]
    fn trees_are_rooted_on_the_surface() {
        let generator = generator(99);
        let mut trees = 0;
        for cx in -3..=3 {
            for cz in -3..=3 {
                let pos = ChunkPos::new(cx, 0, cz);
                let chunk = generator.generate_chunk(pos);
                for lz in 0..CHUNK_SIZE {
                    for lx in 0..CHUNK_SIZE {
                        for y in 1..CHUNK_SIZE {
                            if chunk.block_at(lx, y, lz) == Block::Wood
                                && chunk.block_at(lx, y - 1, lz) != Block::Wood
                            {
                                trees += 1;
                                assert!(
                                    matches!(
                                        chunk.block_at(lx, y - 1, lz),
                                        Block::Grass | Block::Dirt | Block::Stone
                                    ),
                                    "树干应直接立在地表方块上"
                                );
                            }
                        }
                    }
                }
            }
        }
        assert!(trees > 0, "生成范围内应出现树木");
    }

    #[test]
    fn spawn_area_has_no_trees() {
        let generator = generator(5);
        let pos = ChunkPos::from_world(
            super::super::config::SPAWN_COLUMN.0,
            0,
            super::super::config::SPAWN_COLUMN.1,
        );
        let chunk = generator.generate_chunk(pos);
        let (sx, _, sz) = pos.local_of(
            super::super::config::SPAWN_COLUMN.0,
            0,
            super::super::config::SPAWN_COLUMN.1,
        );
        for dz in -6..=6 {
            for dx in -6..=6 {
                let x = sx + dx;
                let z = sz + dz;
                if !(0..CHUNK_SIZE).contains(&x) || !(0..CHUNK_SIZE).contains(&z) {
                    continue;
                }
                for y in 0..CHUNK_SIZE {
                    assert_ne!(chunk.block_at(x, y, z), Block::Wood, "出生点附近不应有树干");
                    assert_ne!(
                        chunk.block_at(x, y, z),
                        Block::Leaves,
                        "出生点附近不应有树叶"
                    );
                }
            }
        }
    }
}
