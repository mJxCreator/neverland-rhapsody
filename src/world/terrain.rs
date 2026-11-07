//! 地形生成：用噪声生成高度图，并按高度分层填充区块。

use noise::{Fbm, MultiFractal, NoiseFn, OpenSimplex};

use super::block::Block;
use super::chunk::{Chunk, ChunkPos, CHUNK_SIZE};

/// 世界竖直方向覆盖的高度，对应两个 16 高区块层。
pub const WORLD_HEIGHT: i32 = 32;
/// 地表基准高度，噪声在此基础上上下起伏。
const BASE_HEIGHT: f64 = 5.0;
/// 顶层露出石头的高度阈值，让高山呈现岩石山顶。
const ROCK_LINE: i32 = 17;
/// 表层草方块下方的泥土层数。
const DIRT_DEPTH: i32 = 4;
/// 山地相对基准的最大抬升量。
const MOUNTAIN_AMPLITUDE: f64 = 20.0;
/// v0.14 木质资源：草地方柱上生成的最短树干高度。
const TREE_TRUNK_HEIGHT: i32 = 3;
/// 树干生成概率的稀疏槽位数：约 1/128 的草地方柱会长出树干。
const TREE_SLOT_MODULUS: u32 = 128;

/// 程序化地形生成器：低频噪声决定整体地势，山脊噪声抬升出山峰。
pub struct TerrainGenerator {
    continent: Fbm<OpenSimplex>,
    mountains: Fbm<OpenSimplex>,
}

impl TerrainGenerator {
    /// 用给定种子构造生成器，相同种子产生完全相同的地形。
    pub fn new(seed: u32) -> Self {
        let continent = Fbm::<OpenSimplex>::new(seed)
            .set_octaves(4)
            .set_frequency(0.015)
            .set_persistence(0.5);
        let mountains = Fbm::<OpenSimplex>::new(seed.wrapping_add(1))
            .set_octaves(4)
            .set_frequency(0.05)
            .set_persistence(0.45);
        Self {
            continent,
            mountains,
        }
    }

    /// 某一世界方柱的地表高度（最顶层方块的世界 y）。
    pub fn height_at(&self, world_x: i32, world_z: i32) -> i32 {
        let x = world_x as f64;
        let z = world_z as f64;

        // 低频噪声决定整体海拔，范围约 ±1。
        let continent = self.continent.get([x, z]);
        // 只有地势较高的区域才抬升出山地，避免整张地图长满尖峰。
        let mask = (continent * 1.8 + 0.2).clamp(0.0, 1.0);
        // 绝对值把噪声折成山脊线，三次方锐化后形成尖锐山体。
        let ridge = (1.0 - self.mountains.get([x, z]).abs()).clamp(0.0, 1.0);

        let height = BASE_HEIGHT + continent * 4.0 + mask * ridge.powi(3) * MOUNTAIN_AMPLITUDE;
        height.clamp(1.0, (WORLD_HEIGHT - 1) as f64) as i32
    }

    /// 生成指定区块：按高度图自下而上填充石头、泥土与草方块，并在少量草地方柱上
    /// 放置可徒手采集的树干，保证新世界能够从零开始获取木材。
    pub fn generate_chunk(&self, pos: ChunkPos) -> Chunk {
        let mut chunk = Chunk::new(pos);
        let origin_x = pos.x * CHUNK_SIZE;
        let origin_y = pos.y * CHUNK_SIZE;
        let origin_z = pos.z * CHUNK_SIZE;

        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let world_x = origin_x + x;
                let world_z = origin_z + z;
                let height = self.height_at(world_x, world_z);
                if height > origin_y {
                    let local_top = (height - origin_y).min(CHUNK_SIZE);
                    for y in 0..local_top {
                        let world_y = origin_y + y;
                        chunk.set_block(x, y, z, self.block_at_height(world_y, height));
                    }
                }

                if !self.has_tree(world_x, world_z) || height >= ROCK_LINE {
                    continue;
                }
                for step in 0..TREE_TRUNK_HEIGHT {
                    let world_y = height + step;
                    if world_y < origin_y
                        || world_y >= origin_y + CHUNK_SIZE
                        || world_y >= WORLD_HEIGHT
                    {
                        continue;
                    }
                    chunk.set_block(x, world_y - origin_y, z, Block::Wood);
                }
            }
        }

        chunk
    }

    /// 该方柱是否生成树干；固定种子的确定性哈希保证区块边界与重复加载一致。
    fn has_tree(&self, world_x: i32, world_z: i32) -> bool {
        let hash = world_x.wrapping_mul(37) ^ world_z.wrapping_mul(17);
        hash.unsigned_abs().is_multiple_of(TREE_SLOT_MODULUS)
    }

    /// 地表高度以下某个 y 处应使用的方块类型。
    fn block_at_height(&self, world_y: i32, height: i32) -> Block {
        if world_y == height - 1 {
            if height >= ROCK_LINE {
                Block::Stone
            } else {
                Block::Grass
            }
        } else if world_y >= height - DIRT_DEPTH {
            Block::Dirt
        } else {
            Block::Stone
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: u32 = 0x5EED;

    #[test]
    fn same_seed_generates_same_heights() {
        let a = TerrainGenerator::new(SEED);
        let b = TerrainGenerator::new(SEED);
        for z in (0..64).step_by(7) {
            for x in (0..64).step_by(7) {
                assert_eq!(a.height_at(x, z), b.height_at(x, z));
            }
        }
    }

    #[test]
    fn different_seeds_differ_somewhere() {
        let a = TerrainGenerator::new(SEED);
        let b = TerrainGenerator::new(SEED + 1);
        let differs = (0..64)
            .flat_map(|z| (0..64).map(move |x| (x, z)))
            .any(|(x, z)| a.height_at(x, z) != b.height_at(x, z));
        assert!(differs);
    }

    #[test]
    fn heights_stay_inside_world_bounds() {
        let terrain = TerrainGenerator::new(SEED);
        let mut max = 0;
        let mut min = WORLD_HEIGHT;
        for z in 0..96 {
            for x in 0..96 {
                let height = terrain.height_at(x, z);
                assert!((1..WORLD_HEIGHT).contains(&height), "高度越界: {height}");
                max = max.max(height);
                min = min.min(height);
            }
        }
        // 既要有低地，也要有超过单层区块的明显山地，同时不应顶到世界高度上限形成平顶。
        assert!(min < CHUNK_SIZE, "缺少低地，最低高度 {min}");
        assert!(max > CHUNK_SIZE, "缺少山地，最高高度 {max}");
        assert!(
            max < WORLD_HEIGHT - 1,
            "地形顶到上限，出现平顶，最高高度 {max}"
        );
    }

    #[test]
    fn chunk_layers_follow_the_heightmap() {
        let terrain = TerrainGenerator::new(SEED);
        let height = terrain.height_at(5, 3);
        let chunk = terrain.generate_chunk(ChunkPos::new(0, 0, 0));

        assert!(
            matches!(chunk.block_at(5, height, 3), Block::Air | Block::Wood),
            "地表上方只能是空气或树干"
        );
        assert!(chunk.block_at(5, height - 1, 3).is_solid());
        assert_eq!(chunk.block_at(5, 0, 3), Block::Stone);
    }
}
