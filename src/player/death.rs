//! 死亡与重生流程辅助：在世界中寻找安全的重生落点。
//!
//! 落点必须满足两个条件：脚下有实体方块、玩家 AABB 不与任何实体方块重叠。
//! 调用方负责确保目标区域区块已加载，本模块只负责几何判定。

use glam::{IVec3, Vec3};

use super::collision::Aabb;
use super::player::PlayerDimensions;
use crate::world::{ChunkPos, World, WORLD_HEIGHT};

/// 重生搜索的最大水平半径（格）。
pub const RESPAWN_SEARCH_RADIUS: i32 = 6;

/// 在锚点附近寻找安全落点；优先使用锚点所在列，其次按半径逐圈向外搜索。
///
/// 若世界中没有可用落点（例如区块未加载），返回锚点原位置作为兜底，调用方
/// 仍可通过后续物理步让玩家落到地面。
pub fn find_respawn_position(world: &World, anchor: Vec3, dimensions: PlayerDimensions) -> Vec3 {
    let base_x = anchor.x.floor() as i32;
    let base_z = anchor.z.floor() as i32;

    for radius in 0..=RESPAWN_SEARCH_RADIUS {
        for offset_x in -radius..=radius {
            for offset_z in -radius..=radius {
                // 只处理当前圈层，避免重复扫描已检查过的内圈。
                if radius > 0 && offset_x.abs() != radius && offset_z.abs() != radius {
                    continue;
                }
                if let Some(position) =
                    safe_position(world, base_x + offset_x, base_z + offset_z, dimensions)
                {
                    return position;
                }
            }
        }
    }

    tracing::warn!(
        x = anchor.x,
        z = anchor.z,
        "未找到安全重生点，回退到锚点位置"
    );
    anchor
}

/// 指定方柱上的安全落点；区块未加载或没有站立空间时返回 `None`。
fn safe_position(
    world: &World,
    block_x: i32,
    block_z: i32,
    dimensions: PlayerDimensions,
) -> Option<Vec3> {
    if !world.contains_chunk(ChunkPos::from_world(block_x, 0, block_z)) {
        return None;
    }

    let top = topmost_solid(world, block_x, block_z)?;
    let feet_y = top + 1;
    if feet_y <= 0 || feet_y >= WORLD_HEIGHT {
        return None;
    }

    let position = Vec3::new(block_x as f32 + 0.5, feet_y as f32, block_z as f32 + 0.5);
    let aabb = Aabb::from_feet(position, dimensions.width, dimensions.height);
    if aabb_overlaps_world(world, aabb) {
        return None;
    }
    Some(position)
}

/// 从世界顶部向下寻找最高的实体方块；区块未加载时返回 `None`。
fn topmost_solid(world: &World, block_x: i32, block_z: i32) -> Option<i32> {
    (0..WORLD_HEIGHT)
        .rev()
        .find(|&y| world.block_at(block_x, y, block_z).is_solid())
}

/// AABB 覆盖的体素中是否存在实体方块。
fn aabb_overlaps_world(world: &World, aabb: Aabb) -> bool {
    let lo = IVec3::new(
        (aabb.min.x + 1e-4).floor() as i32,
        (aabb.min.y + 1e-4).floor() as i32,
        (aabb.min.z + 1e-4).floor() as i32,
    );
    let hi = IVec3::new(
        (aabb.max.x - 1e-4).floor() as i32,
        (aabb.max.y - 1e-4).floor() as i32,
        (aabb.max.z - 1e-4).floor() as i32,
    );
    for y in lo.y..=hi.y {
        for z in lo.z..=hi.z {
            for x in lo.x..=hi.x {
                if world.block_at(x, y, z).is_solid() {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::Chunk;

    fn dimensions() -> PlayerDimensions {
        PlayerDimensions {
            width: 0.6,
            height: 1.8,
            eye_height: 1.65,
        }
    }

    fn loaded_world_with_floor() -> World {
        let mut world = World::new(0x5EED);
        let mut chunk = Chunk::new(ChunkPos::new(0, 0, 0));
        for z in 0..16 {
            for x in 0..16 {
                chunk.set_block(x, 4, z, crate::world::Block::Stone);
            }
        }
        world.insert_chunk(chunk);
        world
    }

    #[test]
    fn respawn_position_stands_on_the_top_solid_block() {
        let world = loaded_world_with_floor();
        let position = find_respawn_position(&world, Vec3::new(8.0, 20.0, 8.0), dimensions());
        assert_eq!(position.y, 5.0, "脚底应位于最高实体方块上方");
        let aabb = Aabb::from_feet(position, dimensions().width, dimensions().height);
        assert!(!aabb_overlaps_world(&world, aabb), "重生点不得卡在方块里");
    }

    #[test]
    fn an_unloaded_anchor_column_is_skipped_in_favor_of_a_loaded_neighbor() {
        // 只加载区块 (0,0,0)；锚点落在相邻未加载区块的列上。
        let world = loaded_world_with_floor();
        let position = find_respawn_position(&world, Vec3::new(16.5, 20.0, 8.5), dimensions());
        assert!(
            position.x < 16.0,
            "未加载区块的列不能作为重生点，应退回到已加载的相邻列"
        );
        let aabb = Aabb::from_feet(position, dimensions().width, dimensions().height);
        assert!(!aabb_overlaps_world(&world, aabb));
    }

    #[test]
    fn an_unloaded_area_falls_back_to_the_anchor() {
        let world = World::new(0x5EED);
        let anchor = Vec3::new(200.5, 12.0, 200.5);
        let position = find_respawn_position(&world, anchor, dimensions());
        assert_eq!(position, anchor);
    }
}
