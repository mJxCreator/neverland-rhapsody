//! 世界：按固定范围生成地形区块，并提供世界坐标下的方块查询。

use super::block::Block;
use super::chunk::{Chunk, ChunkPos, CHUNK_SIZE};
use super::terrain::{TerrainGenerator, WORLD_HEIGHT};

/// 水平方向每轴的区块数，世界因此覆盖 96×96 方块。
pub const WORLD_CHUNKS_X: i32 = 6;
pub const WORLD_CHUNKS_Z: i32 = 6;
/// 竖直方向的区块层数。
pub const WORLD_CHUNKS_Y: i32 = WORLD_HEIGHT / CHUNK_SIZE;
/// 世界包含的区块总数。
pub const WORLD_CHUNK_COUNT: usize = (WORLD_CHUNKS_X * WORLD_CHUNKS_Y * WORLD_CHUNKS_Z) as usize;

/// 默认世界种子。
pub const DEFAULT_SEED: u32 = 0x5EED;

/// 世界：持有地形生成器与已生成的区块集合。
pub struct World {
    terrain: TerrainGenerator,
    chunks: Vec<Chunk>,
}

impl World {
    /// 用给定种子生成完整地形；区块按 y → z → x 顺序连续存放，便于按下标查询。
    pub fn new(seed: u32) -> Self {
        let terrain = TerrainGenerator::new(seed);
        let mut chunks = Vec::with_capacity(WORLD_CHUNK_COUNT);
        for chunk_y in 0..WORLD_CHUNKS_Y {
            for chunk_z in 0..WORLD_CHUNKS_Z {
                for chunk_x in 0..WORLD_CHUNKS_X {
                    let pos = ChunkPos::new(chunk_x, chunk_y, chunk_z);
                    chunks.push(terrain.generate_chunk(pos));
                }
            }
        }
        Self { terrain, chunks }
    }

    /// 当前已生成的全部区块。
    pub fn chunks(&self) -> &[Chunk] {
        &self.chunks
    }

    /// 某一世界方柱的地表高度。
    pub fn height_at(&self, world_x: i32, world_z: i32) -> i32 {
        self.terrain.height_at(world_x, world_z)
    }

    /// 读取世界坐标下的方块；世界外返回空气，世界底面以下视为石头。
    pub fn block_at(&self, x: i32, y: i32, z: i32) -> Block {
        if y < 0 {
            return Block::Stone;
        }
        let pos = ChunkPos::from_world(x, y, z);
        let Some(index) = Self::chunk_index(pos) else {
            return Block::Air;
        };
        self.chunks[index].block_at(
            x - pos.x * CHUNK_SIZE,
            y - pos.y * CHUNK_SIZE,
            z - pos.z * CHUNK_SIZE,
        )
    }

    /// 区块坐标到集合下标；超出世界范围的区块返回 `None`。
    fn chunk_index(pos: ChunkPos) -> Option<usize> {
        if !(0..WORLD_CHUNKS_X).contains(&pos.x)
            || !(0..WORLD_CHUNKS_Y).contains(&pos.y)
            || !(0..WORLD_CHUNKS_Z).contains(&pos.z)
        {
            return None;
        }
        Some(((pos.y * WORLD_CHUNKS_Z + pos.z) * WORLD_CHUNKS_X + pos.x) as usize)
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new(DEFAULT_SEED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_generates_all_chunk_layers() {
        let world = World::new(DEFAULT_SEED);
        assert_eq!(world.chunks().len(), WORLD_CHUNK_COUNT);
    }

    #[test]
    fn blocks_follow_the_heightmap() {
        let world = World::new(DEFAULT_SEED);
        let height = world.height_at(20, 40);
        assert!(world.block_at(20, height - 1, 40).is_solid());
        assert_eq!(world.block_at(20, height, 40), Block::Air);
    }

    #[test]
    fn blocks_cross_chunk_borders() {
        let world = World::new(DEFAULT_SEED);
        // x = 15 与 x = 16 分别落在第 0、1 号区块，世界查询应等价于对应区块的局部查询。
        assert_eq!(
            world.block_at(16, 0, 8),
            world.chunks()[1].block_at(0, 0, 8)
        );
        assert_eq!(
            world.block_at(15, 0, 8),
            world.chunks()[0].block_at(15, 0, 8)
        );
    }

    #[test]
    fn outside_the_world_reads_air_and_below_reads_stone() {
        let world = World::new(DEFAULT_SEED);
        assert_eq!(world.block_at(200, 10, 10), Block::Air);
        assert_eq!(world.block_at(-1, 10, 10), Block::Air);
        assert_eq!(world.block_at(10, -1, 10), Block::Stone);
        assert_eq!(world.block_at(10, WORLD_HEIGHT + 5, 10), Block::Air);
    }

    #[test]
    fn same_seed_builds_the_same_world() {
        let a = World::new(DEFAULT_SEED);
        let b = World::new(DEFAULT_SEED);
        for x in (0..96).step_by(11) {
            for z in (0..96).step_by(11) {
                let height = a.height_at(x, z);
                assert_eq!(a.block_at(x, height - 1, z), b.block_at(x, height - 1, z));
            }
        }
    }
}
