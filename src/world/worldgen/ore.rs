//! 矿石矿脉生成。
//!
//! 每条矿脉由“来源区块 + 矿种 + 序号”确定；生成某区块时会重放周围 3×3 区块的
//! 矿脉并只写入落在本区块内的方块，因此跨区块矿脉不会重复也不会断裂。

use crate::world::block::Block;
use crate::world::chunk::{Chunk, ChunkPos, CHUNK_SIZE};

use super::config::OreVeinConfig;
use super::noise::Rng;

/// 矿脉生成器。
pub struct OreGenerator {
    seed: u32,
    configs: [OreVeinConfig; 4],
}

impl OreGenerator {
    pub fn new(seed: u32, configs: [OreVeinConfig; 4]) -> Self {
        Self { seed, configs }
    }

    /// 把矿脉写入目标区块；只替换石头，避免覆盖洞穴、水或地表。
    pub fn generate(&self, pos: ChunkPos, chunk: &mut Chunk) {
        for dx in -1..=1 {
            for dz in -1..=1 {
                let source = ChunkPos::new(pos.x + dx, 0, pos.z + dz);
                for (index, config) in self.configs.iter().enumerate() {
                    self.write_veins(source, pos, chunk, index, config);
                }
            }
        }
    }

    fn write_veins(
        &self,
        source: ChunkPos,
        target: ChunkPos,
        chunk: &mut Chunk,
        index: usize,
        config: &OreVeinConfig,
    ) {
        let mut rng = Rng::new(vein_seed(self.seed, source, index));
        for _ in 0..config.attempts {
            let origin_x = source.x * CHUNK_SIZE + rng.below(CHUNK_SIZE as u32) as i32;
            let origin_z = source.z * CHUNK_SIZE + rng.below(CHUNK_SIZE as u32) as i32;
            let y_range = (config.max_y - config.min_y + 1).max(1) as u32;
            let origin_y = config.min_y + rng.below(y_range) as i32;
            let size = config.vein_size(rng.below(1024));

            let mut x = origin_x;
            let mut y = origin_y;
            let mut z = origin_z;
            for _ in 0..size {
                if ChunkPos::from_world(x, y, z) == target {
                    let (lx, ly, lz) = target.local_of(x, y, z);
                    if chunk.block_at(lx, ly, lz) == Block::Stone {
                        chunk.set_block(lx, ly, lz, config.block);
                    }
                }
                x += rng.step();
                y = (y + rng.step()).clamp(config.min_y, config.max_y);
                z += rng.step();
            }
        }
    }
}

/// 为“来源区块 + 矿种”生成稳定的随机种子。
fn vein_seed(seed: u32, chunk: ChunkPos, index: usize) -> u64 {
    let packed =
        (chunk.x as i64 as u64 & 0xFFFF_FFFF) | ((chunk.z as i64 as u64 & 0xFFFF_FFFF) << 32);
    (seed as u64) ^ packed.rotate_left(23) ^ (index as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::worldgen::config::DEFAULT_ORES;

    fn count_ores(chunk: &Chunk) -> usize {
        let mut count = 0;
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    if matches!(
                        chunk.block_at(x, y, z),
                        Block::CoalOre | Block::IronOre | Block::GoldOre | Block::DiamondOre
                    ) {
                        count += 1;
                    }
                }
            }
        }
        count
    }

    #[test]
    fn ore_generation_is_deterministic() {
        let generator = OreGenerator::new(99, DEFAULT_ORES);
        let pos = ChunkPos::new(3, 0, -2);
        let mut first = Chunk::new(pos);
        let mut second = Chunk::new(pos);
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    first.set_block(x, y, z, Block::Stone);
                    second.set_block(x, y, z, Block::Stone);
                }
            }
        }
        generator.generate(pos, &mut first);
        generator.generate(pos, &mut second);
        assert_eq!(count_ores(&first), count_ores(&second));
        assert!(count_ores(&first) > 0, "石头区块应生成矿脉");
    }

    #[test]
    fn ores_only_replace_stone() {
        let generator = OreGenerator::new(5, DEFAULT_ORES);
        let pos = ChunkPos::new(0, 0, 0);
        let mut chunk = Chunk::new(pos);
        generator.generate(pos, &mut chunk);
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let block = chunk.block_at(x, y, z);
                    assert!(
                        block == Block::Air || block == Block::Stone,
                        "不应写入非石头位置"
                    );
                }
            }
        }
    }
}
