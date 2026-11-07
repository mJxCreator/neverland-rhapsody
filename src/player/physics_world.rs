//! 物理世界查询：为碰撞系统提供方块实体判定。
//!
//! 物理只依赖 World 的方块数据，不依赖渲染器的 GPU 网格；
//! 未加载区块视为实体屏障，避免玩家进入尚未生成地形的区域。

use crate::world::{ChunkPos, World, WORLD_HEIGHT};

/// 物理系统专用的方块查询视图。
pub struct PhysicsWorld<'a> {
    world: &'a World,
}

impl<'a> PhysicsWorld<'a> {
    pub fn new(world: &'a World) -> Self {
        Self { world }
    }

    /// 指定世界坐标是否为实体方块。
    ///
    /// 世界底部以下视为实心地面；顶部以上视为空气；
    /// 水平方向未加载的区块视为实心，防止玩家穿过尚未准备好的地形。
    pub fn is_solid(&self, x: i32, y: i32, z: i32) -> bool {
        if y < 0 {
            return true;
        }
        if y >= WORLD_HEIGHT {
            return false;
        }
        if !self.world.contains_chunk(ChunkPos::from_world(x, y, z)) {
            return true;
        }
        self.world.block_at(x, y, z).is_solid()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Chunk, DEFAULT_SEED};

    fn world_with_air_chunk() -> World {
        let mut world = World::new(DEFAULT_SEED);
        assert!(world.insert_chunk(Chunk::new(ChunkPos::new(0, 0, 0))));
        world
    }

    #[test]
    fn loaded_air_is_passable() {
        let world = world_with_air_chunk();
        let physics = PhysicsWorld::new(&world);
        assert!(!physics.is_solid(8, 11, 8));
    }

    #[test]
    fn unloaded_chunks_block_movement() {
        let world = world_with_air_chunk();
        let physics = PhysicsWorld::new(&world);
        assert!(physics.is_solid(20, 11, 8));
        assert!(physics.is_solid(-1, 11, 8));
    }

    #[test]
    fn vertical_bounds_are_consistent() {
        let world = world_with_air_chunk();
        let physics = PhysicsWorld::new(&world);
        assert!(physics.is_solid(8, -1, 8));
        assert!(!physics.is_solid(8, WORLD_HEIGHT, 8));
    }
}
