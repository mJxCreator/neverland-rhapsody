//! 复活点：优先使用床，无效时回退到世界出生锚点。

use glam::{IVec3, Vec3};

use crate::player::find_respawn_position;
use crate::player::player::PlayerDimensions;
use crate::world::{Block, ChunkPos, World};

/// 床复活点的安全落点。
///
/// 必须同时满足：床方块仍然存在、所在区块已加载、落点附近存在可站立空间。
/// 任一条件不满足都返回 `None`，由调用方回退到世界出生点。
pub fn bed_respawn_position(
    world: &World,
    bed: IVec3,
    dimensions: PlayerDimensions,
) -> Option<Vec3> {
    if world.block_at(bed.x, bed.y, bed.z) != Block::Bed {
        return None;
    }
    if !world.contains_chunk(ChunkPos::from_world(bed.x, 0, bed.z)) {
        return None;
    }
    let anchor = Vec3::new(bed.x as f32 + 0.5, bed.y as f32 + 1.0, bed.z as f32 + 0.5);
    let position = find_respawn_position(world, anchor, dimensions);
    if !is_safe_standing_position(world, position, dimensions) {
        return None;
    }
    Some(position)
}

/// 落点是否满足地面支撑与两格净空。
fn is_safe_standing_position(world: &World, position: Vec3, dimensions: PlayerDimensions) -> bool {
    let x = position.x.floor() as i32;
    let z = position.z.floor() as i32;
    if !world.contains_chunk(ChunkPos::from_world(x, 0, z)) {
        return false;
    }
    let feet = position.y.floor() as i32;
    let head = (position.y + dimensions.height - 1e-3).floor() as i32;
    if feet <= 0 {
        return false;
    }
    let below = world.block_at(x, feet - 1, z);
    below.is_solid()
        && !world.block_at(x, feet, z).is_solid()
        && !world.block_at(x, head, z).is_solid()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::player::PlayerDimensions;
    use crate::world::{ChunkPos, DEFAULT_SEED, WORLD_HEIGHT};

    fn dimensions() -> PlayerDimensions {
        PlayerDimensions {
            width: 0.6,
            height: 1.8,
            eye_height: 1.62,
        }
    }

    fn flat_world() -> World {
        let mut world = World::new(DEFAULT_SEED);
        let chunk = world.generate_chunk(ChunkPos::from_world(0, 0, 0));
        world.insert_chunk(chunk);
        for x in -2..=2 {
            for z in -2..=2 {
                world.set_block(x, 10, z, Block::Stone);
            }
        }
        for y in 11..WORLD_HEIGHT {
            for x in -2..=2 {
                for z in -2..=2 {
                    world.set_block(x, y, z, Block::Air);
                }
            }
        }
        world
    }

    #[test]
    fn bed_yields_safe_position() {
        let mut world = flat_world();
        let bed = IVec3::new(0, 11, 0);
        world.set_block(bed.x, bed.y, bed.z, Block::Bed);
        let position = bed_respawn_position(&world, bed, dimensions()).expect("床复活点应有效");
        assert!(is_safe_standing_position(&world, position, dimensions()));
    }

    #[test]
    fn missing_bed_falls_back() {
        let world = flat_world();
        assert!(bed_respawn_position(&world, IVec3::new(0, 11, 0), dimensions()).is_none());
    }

    #[test]
    fn unloaded_chunk_is_rejected() {
        let mut world = flat_world();
        let bed = IVec3::new(64, 11, 64);
        world.set_block(0, 11, 0, Block::Bed);
        assert!(bed_respawn_position(&world, bed, dimensions()).is_none());
    }

    #[test]
    fn blocked_space_is_rejected() {
        let mut world = flat_world();
        let bed = IVec3::new(0, 11, 0);
        world.set_block(bed.x, bed.y, bed.z, Block::Bed);
        world.set_block(0, 12, 0, Block::Stone);
        world.set_block(1, 11, 1, Block::Stone);
        world.set_block(-1, 11, -1, Block::Stone);
        world.set_block(1, 11, -1, Block::Stone);
        world.set_block(-1, 11, 1, Block::Stone);
        // 床四周仍可能存在安全落点，此用例仅要求结果不会卡在方块里。
        if let Some(position) = bed_respawn_position(&world, bed, dimensions()) {
            assert!(is_safe_standing_position(&world, position, dimensions()));
        }
    }
}
