//! 世界：动态区块容器，按 ChunkPos 查询与增删区块，并提供世界坐标下的方块读写。

use std::collections::HashMap;

use super::block::Block;
use super::chunk::{Chunk, ChunkPos, CHUNK_SIZE};
use super::terrain::WORLD_HEIGHT;
use super::worldgen::{WorldGenStatsSnapshot, WorldGenerator, CURRENT_WORLD_GEN_VERSION};

/// 竖直方向的区块层数：世界高度固定为 32，对应 2 层 16 高区块。
pub const WORLD_CHUNKS_Y: i32 = WORLD_HEIGHT / CHUNK_SIZE;

/// 默认世界种子。
pub const DEFAULT_SEED: u32 = 0x5EED;

/// 动态世界：只持有当前已加载的区块，水平方向不设边界。
pub struct World {
    seed: u32,
    generation_version: u32,
    generator: WorldGenerator,
    chunks: HashMap<ChunkPos, Chunk>,
}

impl World {
    /// 创建空世界；区块由区块管理器按玩家位置陆续加载。
    pub fn new(seed: u32) -> Self {
        Self::with_generation(seed, CURRENT_WORLD_GEN_VERSION)
    }

    /// 按存档记录的地形版本创建世界；旧版本继续使用原有高度图。
    pub fn with_generation(seed: u32, generation_version: u32) -> Self {
        Self {
            seed,
            generation_version,
            generator: WorldGenerator::new(seed, generation_version),
            chunks: HashMap::new(),
        }
    }

    /// 生成世界所用的种子，存档据此判断改动属于哪张地图。
    pub fn seed(&self) -> u32 {
        self.seed
    }

    /// 世界生成版本；旧存档据此保持原有地形。
    pub fn generation_version(&self) -> u32 {
        self.generation_version
    }

    /// 累计的世界生成耗时统计。
    pub fn generation_stats(&self) -> WorldGenStatsSnapshot {
        self.generator.stats()
    }

    /// 该列的主导生物群系。
    pub fn biome_at(&self, world_x: i32, world_z: i32) -> super::worldgen::biome::BiomeId {
        self.generator.biome_at(world_x, world_z)
    }

    /// 当前已加载的全部区块，顺序不保证。
    pub fn chunks(&self) -> impl Iterator<Item = &Chunk> + '_ {
        self.chunks.values()
    }

    /// 当前已加载区块的坐标。
    pub fn chunk_positions(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.chunks.keys().copied()
    }

    /// 当前已加载的区块数量。
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// 按区块坐标读取区块。
    pub fn chunk_at(&self, pos: ChunkPos) -> Option<&Chunk> {
        self.chunks.get(&pos)
    }

    /// 区块坐标是否已加载。
    pub fn contains_chunk(&self, pos: ChunkPos) -> bool {
        self.chunks.contains_key(&pos)
    }

    /// 生成指定区块的地形数据，但不插入世界。
    pub fn generate_chunk(&self, pos: ChunkPos) -> Chunk {
        self.generator.generate_chunk(pos)
    }

    /// 插入区块；相同 ChunkPos 已存在或垂直层级越界时返回 `false`，避免重复实例。
    pub fn insert_chunk(&mut self, chunk: Chunk) -> bool {
        let pos = chunk.pos();
        if !(0..WORLD_CHUNKS_Y).contains(&pos.y) {
            return false;
        }
        if self.chunks.contains_key(&pos) {
            return false;
        }
        self.chunks.insert(pos, chunk);
        true
    }

    /// 移除区块并返回其数据；区块不存在时返回 `None`。
    pub fn remove_chunk(&mut self, pos: ChunkPos) -> Option<Chunk> {
        self.chunks.remove(&pos)
    }

    /// 某一世界方柱的地表高度。
    pub fn height_at(&self, world_x: i32, world_z: i32) -> i32 {
        self.generator.height_at(world_x, world_z)
    }

    /// 读取世界坐标下的方块。
    ///
    /// 垂直范围固定为 `0..WORLD_HEIGHT`：底面以下视为石头，顶部以上视为空气；
    /// 水平方向未加载的区块一律视为空气，因此区块边界始终显示边界面。
    pub fn block_at(&self, x: i32, y: i32, z: i32) -> Block {
        if y < 0 {
            return Block::Stone;
        }
        if y >= WORLD_HEIGHT {
            return Block::Air;
        }
        let pos = ChunkPos::from_world(x, y, z);
        let Some(chunk) = self.chunks.get(&pos) else {
            return Block::Air;
        };
        let (local_x, local_y, local_z) = pos.local_of(x, y, z);
        chunk.block_at(local_x, local_y, local_z)
    }

    /// 坐标是否落在垂直世界高度内；水平方向不设边界，能否修改取决于区块是否已加载。
    pub fn contains(&self, _x: i32, y: i32, _z: i32) -> bool {
        (0..WORLD_HEIGHT).contains(&y)
    }

    /// 写入世界坐标下的方块，返回需要重建网格的区块：自身以及与该方块相邻的已加载区块。
    ///
    /// 区块未加载、垂直坐标越界或方块没有变化时返回空列表。
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block: Block) -> Vec<ChunkPos> {
        if !self.contains(x, y, z) {
            return Vec::new();
        }
        let pos = ChunkPos::from_world(x, y, z);
        let Some(chunk) = self.chunks.get_mut(&pos) else {
            return Vec::new();
        };

        let (local_x, local_y, local_z) = pos.local_of(x, y, z);
        if chunk.block_at(local_x, local_y, local_z) == block {
            return Vec::new();
        }
        chunk.set_block(local_x, local_y, local_z, block);

        let mut affected = vec![pos];
        let mut neighbors = [None; 6];
        if local_x == 0 {
            neighbors[0] = Some(ChunkPos::new(pos.x - 1, pos.y, pos.z));
        }
        if local_x == CHUNK_SIZE - 1 {
            neighbors[1] = Some(ChunkPos::new(pos.x + 1, pos.y, pos.z));
        }
        if local_y == 0 {
            neighbors[2] = Some(ChunkPos::new(pos.x, pos.y - 1, pos.z));
        }
        if local_y == CHUNK_SIZE - 1 {
            neighbors[3] = Some(ChunkPos::new(pos.x, pos.y + 1, pos.z));
        }
        if local_z == 0 {
            neighbors[4] = Some(ChunkPos::new(pos.x, pos.y, pos.z - 1));
        }
        if local_z == CHUNK_SIZE - 1 {
            neighbors[5] = Some(ChunkPos::new(pos.x, pos.y, pos.z + 1));
        }
        for neighbor in neighbors.into_iter().flatten() {
            if self.chunks.contains_key(&neighbor) {
                affected.push(neighbor);
            }
        }

        affected
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

    /// 创建带指定区块的空世界，供测试直接读写。
    fn loaded_world(positions: &[ChunkPos]) -> World {
        let mut world = World::new(DEFAULT_SEED);
        for &pos in positions {
            let chunk = world.generate_chunk(pos);
            assert!(world.insert_chunk(chunk), "测试区块重复: {pos:?}");
        }
        world
    }

    #[test]
    fn world_starts_empty_and_loads_chunks_on_demand() {
        let mut world = World::new(DEFAULT_SEED);
        assert_eq!(world.chunk_count(), 0);
        assert!(!world.contains_chunk(ChunkPos::new(0, 0, 0)));

        let chunk = world.generate_chunk(ChunkPos::new(0, 0, 0));
        assert!(world.insert_chunk(chunk));
        assert_eq!(world.chunk_count(), 1);
        assert!(world.contains_chunk(ChunkPos::new(0, 0, 0)));
    }

    #[test]
    fn the_same_chunk_position_cannot_be_inserted_twice() {
        let mut world = World::new(DEFAULT_SEED);
        let pos = ChunkPos::new(0, 0, 0);
        assert!(world.insert_chunk(world.generate_chunk(pos)));
        assert!(!world.insert_chunk(world.generate_chunk(pos)));
        assert_eq!(world.chunk_count(), 1);
    }

    #[test]
    fn removing_a_chunk_returns_its_data() {
        let mut world = loaded_world(&[ChunkPos::new(0, 0, 0)]);
        let removed = world.remove_chunk(ChunkPos::new(0, 0, 0));
        assert!(removed.is_some());
        assert_eq!(
            removed.map(|chunk| chunk.pos()),
            Some(ChunkPos::new(0, 0, 0))
        );
        assert!(!world.contains_chunk(ChunkPos::new(0, 0, 0)));
        assert!(world.remove_chunk(ChunkPos::new(0, 0, 0)).is_none());
    }

    #[test]
    fn blocks_follow_the_heightmap_inside_loaded_chunks() {
        let world = loaded_world(&[
            ChunkPos::new(0, 0, 0),
            ChunkPos::new(0, 1, 0),
            ChunkPos::new(1, 0, 0),
            ChunkPos::new(1, 1, 0),
        ]);
        let height = world.height_at(20, 10);
        assert!(world.block_at(20, height - 1, 10).is_solid());
        assert_eq!(world.block_at(20, height, 10), Block::Air);
    }

    #[test]
    fn blocks_cross_chunk_borders() {
        let world = loaded_world(&[
            ChunkPos::new(0, 0, 0),
            ChunkPos::new(1, 0, 0),
            ChunkPos::new(0, 1, 0),
            ChunkPos::new(1, 1, 0),
        ]);
        assert_eq!(
            world.block_at(16, 0, 8),
            world
                .chunk_at(ChunkPos::new(1, 0, 0))
                .unwrap()
                .block_at(0, 0, 8)
        );
        assert_eq!(
            world.block_at(15, 0, 8),
            world
                .chunk_at(ChunkPos::new(0, 0, 0))
                .unwrap()
                .block_at(15, 0, 8)
        );
    }

    #[test]
    fn negative_coordinates_read_the_right_chunk() {
        let pos = ChunkPos::new(-2, 0, -2);
        let mut world = loaded_world(&[pos]);
        // (-1, 0, -17) 属于未加载的区块 (-1, 0, -2)，按空气处理。
        assert_eq!(world.block_at(-1, 0, -17), Block::Air);
        assert!(world.contains(-1, 0, -17));
        assert!(world.block_at(-17, 5, -17).is_solid());

        let affected = world.set_block(-17, 5, -17, Block::Stone);
        assert_eq!(affected, vec![pos]);
        assert_eq!(world.block_at(-17, 5, -17), Block::Stone);
    }

    #[test]
    fn unloaded_chunks_read_air_and_reject_edits() {
        let mut world = World::new(DEFAULT_SEED);
        assert_eq!(world.block_at(200, 10, 10), Block::Air);
        assert_eq!(world.block_at(-1, 10, 10), Block::Air);
        assert!(world.set_block(200, 10, 10, Block::Stone).is_empty());
    }

    #[test]
    fn vertical_out_of_bounds_is_uniform_and_safe() {
        let mut world = loaded_world(&[ChunkPos::new(0, 0, 0), ChunkPos::new(0, 1, 0)]);
        assert_eq!(world.block_at(10, -1, 10), Block::Stone);
        assert_eq!(world.block_at(10, WORLD_HEIGHT, 10), Block::Air);
        assert_eq!(world.block_at(10, WORLD_HEIGHT + 5, 10), Block::Air);
        assert!(!world.contains(10, -1, 10));
        assert!(!world.contains(10, WORLD_HEIGHT, 10));
        assert!(world.set_block(10, -1, 10, Block::Stone).is_empty());
        assert!(world
            .set_block(10, WORLD_HEIGHT, 10, Block::Stone)
            .is_empty());
    }

    #[test]
    fn horizontal_range_is_unbounded() {
        let world = loaded_world(&[ChunkPos::new(100, 0, -100), ChunkPos::new(100, 1, -100)]);
        assert!(world.contains(1600, 10, -1600));
        // 区块已加载时，世界最底层必然有实体方块。
        assert!(world.block_at(1600, 0, -1600).is_solid());
    }

    #[test]
    fn duplicate_vertical_layers_are_rejected() {
        let mut world = World::new(DEFAULT_SEED);
        let mut chunk = Chunk::new(ChunkPos::new(0, 2, 0));
        chunk.set_block(0, 0, 0, Block::Stone);
        assert!(!world.insert_chunk(chunk));
        assert_eq!(world.chunk_count(), 0);
    }

    #[test]
    fn set_block_writes_and_reports_the_chunk() {
        let mut world = loaded_world(&[ChunkPos::new(0, 0, 0), ChunkPos::new(0, 1, 0)]);
        let height = world.height_at(10, 10);
        assert_eq!(world.block_at(10, height, 10), Block::Air);

        let affected = world.set_block(10, height, 10, Block::Stone);

        assert_eq!(world.block_at(10, height, 10), Block::Stone);
        assert_eq!(affected, vec![ChunkPos::new(0, 0, 0)]);
    }

    #[test]
    fn edits_on_a_chunk_border_report_neighbors() {
        let mut world = loaded_world(&[
            ChunkPos::new(0, 0, 0),
            ChunkPos::new(1, 0, 0),
            ChunkPos::new(0, 1, 0),
        ]);
        let affected = world.set_block(15, 0, 8, Block::Air);

        assert_eq!(world.block_at(15, 0, 8), Block::Air);
        assert_eq!(
            affected,
            vec![ChunkPos::new(0, 0, 0), ChunkPos::new(1, 0, 0)]
        );
    }

    #[test]
    fn border_edit_ignores_unloaded_neighbors() {
        let mut world = loaded_world(&[ChunkPos::new(0, 0, 0)]);
        let affected = world.set_block(15, 0, 8, Block::Air);
        assert_eq!(affected, vec![ChunkPos::new(0, 0, 0)]);
    }

    #[test]
    fn unchanged_edits_report_nothing() {
        let mut world = loaded_world(&[ChunkPos::new(0, 0, 0)]);
        assert!(world.set_block(10, 0, 10, Block::Stone).is_empty());
    }

    #[test]
    fn same_seed_builds_the_same_terrain() {
        let a = World::new(DEFAULT_SEED);
        let b = World::new(DEFAULT_SEED);
        for x in (-32..64).step_by(11) {
            for z in (0..96).step_by(11) {
                assert_eq!(a.height_at(x, z), b.height_at(x, z));
            }
        }
    }
}
