//! 区块：固定尺寸方块数组与区块/世界坐标换算。

use glam::Vec3;

use super::block::Block;

/// 区块在三个轴向上的边长（方块数）。
pub const CHUNK_SIZE: i32 = 16;
/// 单个区块的方块总数。
pub const CHUNK_VOLUME: usize = (CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE) as usize;

/// 区块在区块坐标系中的位置。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl ChunkPos {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// 区块最小角的世界坐标。
    pub fn origin(self) -> Vec3 {
        Vec3::new(
            (self.x * CHUNK_SIZE) as f32,
            (self.y * CHUNK_SIZE) as f32,
            (self.z * CHUNK_SIZE) as f32,
        )
    }
}

/// 固定 16×16×16 的方块容器。
pub struct Chunk {
    pos: ChunkPos,
    blocks: Box<[Block; CHUNK_VOLUME]>,
}

impl Chunk {
    /// 创建只含空气的空区块。
    pub fn new(pos: ChunkPos) -> Self {
        Self {
            pos,
            blocks: Box::new([Block::Air; CHUNK_VOLUME]),
        }
    }

    pub fn pos(&self) -> ChunkPos {
        self.pos
    }

    /// 读取区块局部坐标下的方块；越界返回 `Air`。
    pub fn block_at(&self, x: i32, y: i32, z: i32) -> Block {
        if !(0..CHUNK_SIZE).contains(&x)
            || !(0..CHUNK_SIZE).contains(&y)
            || !(0..CHUNK_SIZE).contains(&z)
        {
            return Block::Air;
        }
        self.blocks[Self::index(x, y, z)]
    }

    /// 写入区块局部坐标下的方块，坐标必须落在区块内。
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block: Block) {
        debug_assert!(
            (0..CHUNK_SIZE).contains(&x)
                && (0..CHUNK_SIZE).contains(&y)
                && (0..CHUNK_SIZE).contains(&z),
            "局部坐标越界: ({x}, {y}, {z})"
        );
        let index = Self::index(x, y, z);
        self.blocks[index] = block;
    }

    /// 局部坐标到数组下标的映射，按 y → z → x 展开。
    fn index(x: i32, y: i32, z: i32) -> usize {
        let size = CHUNK_SIZE as usize;
        (y as usize * size + z as usize) * size + x as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_are_addressed_independently() {
        let mut chunk = Chunk::new(ChunkPos::new(0, 0, 0));
        chunk.set_block(0, 0, 0, Block::Stone);
        chunk.set_block(1, 0, 0, Block::Dirt);
        assert_eq!(chunk.block_at(0, 0, 0), Block::Stone);
        assert_eq!(chunk.block_at(1, 0, 0), Block::Dirt);
        assert_eq!(chunk.block_at(0, 0, 1), Block::Air);
    }

    #[test]
    fn out_of_bounds_reads_air() {
        let chunk = Chunk::new(ChunkPos::new(0, 0, 0));
        assert_eq!(chunk.block_at(-1, 0, 0), Block::Air);
        assert_eq!(chunk.block_at(CHUNK_SIZE, 0, 0), Block::Air);
    }

    #[test]
    fn chunk_origin_is_in_world_coordinates() {
        assert_eq!(ChunkPos::new(1, 0, 1).origin(), Vec3::new(16.0, 0.0, 16.0));
        assert_eq!(ChunkPos::new(0, 0, 0).origin(), Vec3::ZERO);
    }
}
