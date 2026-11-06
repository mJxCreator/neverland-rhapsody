//! 世界：持有区块集合并提供方块读写入口。

use super::block::Block;
use super::chunk::{Chunk, ChunkPos, CHUNK_SIZE};

/// 示例平地覆盖的区块数量（每个轴向 2 个，共 32×32 方块）。
pub const DEMO_CHUNK_COUNT: i32 = 2;

/// 区块集合；v0.4 阶段只承载一块静态平地，地形生成在后续里程碑接入。
pub struct World {
    chunks: Vec<Chunk>,
}

impl World {
    /// 生成用于观察渲染的示例平地：底层石头、中层泥土、表层草方块。
    pub fn flat_demo() -> Self {
        let mut chunks = Vec::with_capacity((DEMO_CHUNK_COUNT * DEMO_CHUNK_COUNT) as usize);
        for chunk_z in 0..DEMO_CHUNK_COUNT {
            for chunk_x in 0..DEMO_CHUNK_COUNT {
                let mut chunk = Chunk::new(ChunkPos::new(chunk_x, 0, chunk_z));
                for y in 0..3 {
                    let block = match y {
                        2 => Block::Grass,
                        1 => Block::Dirt,
                        _ => Block::Stone,
                    };
                    for z in 0..CHUNK_SIZE {
                        for x in 0..CHUNK_SIZE {
                            chunk.set_block(x, y, z, block);
                        }
                    }
                }
                chunks.push(chunk);
            }
        }
        Self { chunks }
    }

    /// 当前已加载的全部区块。
    pub fn chunks(&self) -> &[Chunk] {
        &self.chunks
    }
}

impl Default for World {
    fn default() -> Self {
        Self::flat_demo()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_demo_has_two_by_two_chunks() {
        let world = World::flat_demo();
        assert_eq!(
            world.chunks().len(),
            (DEMO_CHUNK_COUNT * DEMO_CHUNK_COUNT) as usize
        );
    }

    #[test]
    fn ground_layers_are_stone_dirt_grass() {
        let world = World::flat_demo();
        let chunk = &world.chunks()[0];
        assert_eq!(chunk.block_at(0, 0, 0), Block::Stone);
        assert_eq!(chunk.block_at(0, 1, 0), Block::Dirt);
        assert_eq!(chunk.block_at(0, 2, 0), Block::Grass);
        assert_eq!(chunk.block_at(0, 3, 0), Block::Air);
    }
}
