//! 方块放置规则：根据射线命中面推导相邻空格，并集中校验所有放置条件。
//!
//! 放置位置只来自 [`RayHit`] 的命中面，不使用玩家朝向估算；失败时不会修改世界。

use glam::IVec3;

use super::raycast::{placement_target, RayHit};
use crate::player::Player;
use crate::world::{Block, ChunkPos, World};

/// 放置失败的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlacementError {
    /// 当前方块类型不允许放置（例如空气）。
    NotPlaceable,
    /// 目标位置超出世界垂直高度。
    OutOfBounds,
    /// 目标位置所在区块尚未加载。
    ChunkNotLoaded,
    /// 目标位置不是空气。
    NotAir,
    /// 目标位置与玩家碰撞盒相交。
    IntersectsPlayer,
}

/// 校验并返回放置位置的世界坐标。
pub fn placement_cell(
    world: &World,
    hit: &RayHit,
    player: &Player,
    block: Block,
) -> Result<IVec3, PlacementError> {
    if !block.is_placeable() {
        return Err(PlacementError::NotPlaceable);
    }

    let target = placement_target(hit);
    if !world.contains(target.x, target.y, target.z) {
        return Err(PlacementError::OutOfBounds);
    }

    let chunk = ChunkPos::from_world(target.x, target.y, target.z);
    if !world.contains_chunk(chunk) {
        return Err(PlacementError::ChunkNotLoaded);
    }

    if world.block_at(target.x, target.y, target.z) != Block::Air {
        return Err(PlacementError::NotAir);
    }

    if player.intersects_block(target) {
        return Err(PlacementError::IntersectsPlayer);
    }

    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::PhysicsConfig;
    use crate::world::{Chunk, Face, DEFAULT_SEED};
    use glam::Vec3;

    fn empty_world(positions: &[ChunkPos]) -> World {
        let mut world = World::new(DEFAULT_SEED);
        for &pos in positions {
            assert!(world.insert_chunk(Chunk::new(pos)));
        }
        world
    }

    fn player_at(position: Vec3) -> Player {
        Player::new(position, PhysicsConfig::default().dimensions)
    }

    fn hit(block: IVec3, face: Face) -> RayHit {
        RayHit { block, face }
    }

    #[test]
    fn a_valid_air_target_is_accepted() {
        let world = empty_world(&[ChunkPos::new(0, 0, 0), ChunkPos::new(0, 1, 0)]);
        let player = player_at(Vec3::new(100.0, 10.0, 100.0));

        let target = placement_cell(
            &world,
            &hit(IVec3::new(4, 5, 5), Face::PosX),
            &player,
            Block::Stone,
        );

        assert_eq!(target, Ok(IVec3::new(5, 5, 5)));
    }

    #[test]
    fn top_and_bottom_faces_place_vertically() {
        let world = empty_world(&[ChunkPos::new(0, 0, 0), ChunkPos::new(0, 1, 0)]);
        let player = player_at(Vec3::new(100.0, 10.0, 100.0));

        assert_eq!(
            placement_cell(
                &world,
                &hit(IVec3::new(4, 5, 4), Face::PosY),
                &player,
                Block::Dirt
            ),
            Ok(IVec3::new(4, 6, 4))
        );
        assert_eq!(
            placement_cell(
                &world,
                &hit(IVec3::new(4, 5, 4), Face::NegY),
                &player,
                Block::Dirt
            ),
            Ok(IVec3::new(4, 4, 4))
        );
    }

    #[test]
    fn air_cannot_be_placed() {
        let world = empty_world(&[ChunkPos::new(0, 0, 0), ChunkPos::new(0, 1, 0)]);
        let player = player_at(Vec3::new(100.0, 10.0, 100.0));

        assert_eq!(
            placement_cell(
                &world,
                &hit(IVec3::new(4, 5, 5), Face::PosX),
                &player,
                Block::Air
            ),
            Err(PlacementError::NotPlaceable)
        );
    }

    #[test]
    fn occupied_targets_are_rejected() {
        let mut world = empty_world(&[ChunkPos::new(0, 0, 0), ChunkPos::new(0, 1, 0)]);
        assert_eq!(
            world.set_block(5, 5, 5, Block::Stone),
            vec![ChunkPos::new(0, 0, 0)]
        );
        let player = player_at(Vec3::new(100.0, 10.0, 100.0));

        assert_eq!(
            placement_cell(
                &world,
                &hit(IVec3::new(4, 5, 5), Face::PosX),
                &player,
                Block::Stone
            ),
            Err(PlacementError::NotAir)
        );
    }

    #[test]
    fn targets_above_the_world_are_rejected() {
        let world = empty_world(&[ChunkPos::new(0, 0, 0), ChunkPos::new(0, 1, 0)]);
        let player = player_at(Vec3::new(100.0, 10.0, 100.0));

        assert_eq!(
            placement_cell(
                &world,
                &hit(IVec3::new(4, 31, 4), Face::PosY),
                &player,
                Block::Stone
            ),
            Err(PlacementError::OutOfBounds)
        );
    }

    #[test]
    fn unloaded_neighbor_chunks_are_rejected() {
        let world = empty_world(&[ChunkPos::new(0, 0, 0), ChunkPos::new(0, 1, 0)]);
        let player = player_at(Vec3::new(100.0, 10.0, 100.0));

        assert_eq!(
            placement_cell(
                &world,
                &hit(IVec3::new(15, 5, 5), Face::PosX),
                &player,
                Block::Stone
            ),
            Err(PlacementError::ChunkNotLoaded)
        );
    }

    #[test]
    fn blocks_inside_the_player_are_rejected() {
        let world = empty_world(&[ChunkPos::new(0, 0, 0), ChunkPos::new(0, 1, 0)]);
        let player = player_at(Vec3::new(5.0, 6.0, 5.0));

        assert_eq!(
            placement_cell(
                &world,
                &hit(IVec3::new(4, 6, 5), Face::PosX),
                &player,
                Block::Dirt
            ),
            Err(PlacementError::IntersectsPlayer)
        );
    }

    #[test]
    fn negative_and_cross_chunk_targets_are_resolved() {
        let world = empty_world(&[
            ChunkPos::new(-1, 0, 0),
            ChunkPos::new(0, 0, 0),
            ChunkPos::new(0, 1, 0),
        ]);
        let player = player_at(Vec3::new(100.0, 10.0, 100.0));

        // x = -1 属于负坐标区块，放置到 -1 格应命中已加载的 (-1, 0, 0) 区块。
        assert_eq!(
            placement_cell(
                &world,
                &hit(IVec3::new(0, 5, 5), Face::NegX),
                &player,
                Block::Grass
            ),
            Ok(IVec3::new(-1, 5, 5))
        );
    }
}
