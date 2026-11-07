//! 实体碰撞：实体与玩家之间的水平分离、实体射线拾取，以及通用的 AABB 查询。
//!
//! 实体与方块的碰撞由 [`super::entity_physics`] 复用玩家侧的 `move_box`；
//! 本模块只处理“实体 vs 玩家”这一层，保证两者不会互相穿透。

use glam::{IVec3, Vec3};

use super::creature::Creature;
use super::entity::EntityId;
use crate::player::collision::{Aabb, AABB_EPSILON};

/// 两个 AABB 是否真实重叠（贴合不算）。
pub fn overlaps(a: Aabb, b: Aabb) -> bool {
    a.min.x < b.max.x - AABB_EPSILON
        && a.max.x > b.min.x + AABB_EPSILON
        && a.min.y < b.max.y - AABB_EPSILON
        && a.max.y > b.min.y + AABB_EPSILON
        && a.min.z < b.max.z - AABB_EPSILON
        && a.max.z > b.min.z + AABB_EPSILON
}

/// AABB 是否与任一实体方块重叠。
pub fn intersects_solid(aabb: Aabb, is_solid: &dyn Fn(i32, i32, i32) -> bool) -> bool {
    let lo = Vec3::new(
        (aabb.min.x + AABB_EPSILON).floor(),
        (aabb.min.y + AABB_EPSILON).floor(),
        (aabb.min.z + AABB_EPSILON).floor(),
    );
    let hi = Vec3::new(
        (aabb.max.x - AABB_EPSILON).floor(),
        (aabb.max.y - AABB_EPSILON).floor(),
        (aabb.max.z - AABB_EPSILON).floor(),
    );
    for x in lo.x as i32..=hi.x as i32 {
        for y in lo.y as i32..=hi.y as i32 {
            for z in lo.z as i32..=hi.z as i32 {
                if is_solid(x, y, z) {
                    return true;
                }
            }
        }
    }
    false
}

/// 射线与 AABB 的最近交点距离；起点在盒内时返回 0。
pub fn ray_aabb(origin: Vec3, direction: Vec3, max_distance: f32, aabb: Aabb) -> Option<f32> {
    let direction = direction.normalize_or_zero();
    if direction == Vec3::ZERO || max_distance <= 0.0 {
        return None;
    }
    let mut near = 0.0f32;
    let mut far = max_distance;
    for axis in 0..3 {
        let o = origin[axis];
        let d = direction[axis];
        if d.abs() < 1e-8 {
            if o < aabb.min[axis] || o > aabb.max[axis] {
                return None;
            }
            continue;
        }
        let t1 = (aabb.min[axis] - o) / d;
        let t2 = (aabb.max[axis] - o) / d;
        let (axis_near, axis_far) = if t1 <= t2 { (t1, t2) } else { (t2, t1) };
        near = near.max(axis_near);
        far = far.min(axis_far);
        if near > far {
            return None;
        }
    }
    if near <= max_distance && far >= 0.0 {
        Some(near.max(0.0))
    } else {
        None
    }
}

/// 沿视线拾取最近的生物。
pub fn raycast_entities(
    creatures: &[Creature],
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
) -> Option<(EntityId, f32)> {
    let direction = direction.normalize_or_zero();
    if direction == Vec3::ZERO || max_distance <= 0.0 {
        return None;
    }
    let mut best: Option<(EntityId, f32)> = None;
    for creature in creatures.iter().filter(|creature| creature.is_alive()) {
        if let Some(distance) = ray_aabb(origin, direction, max_distance, creature.entity.aabb()) {
            let closer = match best {
                None => true,
                Some((_, current)) => distance < current,
            };
            if closer {
                best = Some((creature.id(), distance));
            }
        }
    }
    best
}

/// 把玩家从所有重叠生物中水平推出；返回发生调整的次数。
pub fn resolve_player_overlaps(
    position: &mut Vec3,
    width: f32,
    height: f32,
    creatures: &[Creature],
    is_solid: &dyn Fn(i32, i32, i32) -> bool,
) -> usize {
    let mut adjusted = 0;
    for creature in creatures.iter().filter(|creature| creature.is_alive()) {
        let entity_box = creature.entity.aabb();
        let mut player_box = Aabb::from_feet(*position, width, height);
        if !overlaps(player_box, entity_box) {
            continue;
        }
        let candidates = push_candidates(player_box, entity_box);
        for delta in candidates {
            let candidate_position = *position + delta;
            let candidate_box = Aabb::from_feet(candidate_position, width, height);
            if !overlaps(candidate_box, entity_box) && !intersects_solid(candidate_box, is_solid) {
                *position = candidate_position;
                player_box = candidate_box;
                adjusted += 1;
                break;
            }
        }
    }
    adjusted
}

/// 把生物从玩家碰撞箱中水平推开；成功返回 `true`。
pub fn push_creature_out(
    entity: &mut super::entity::Entity,
    player_box: Aabb,
    is_solid: &dyn Fn(i32, i32, i32) -> bool,
) -> bool {
    if !entity.is_alive() {
        return false;
    }
    let entity_box = entity.aabb();
    if !overlaps(entity_box, player_box) {
        return false;
    }
    // 与玩家推出方向相反：把生物推离玩家。
    let candidates = push_candidates(player_box, entity_box)
        .into_iter()
        .map(|delta| -delta)
        .collect::<Vec<_>>();
    for delta in candidates {
        let candidate_position = entity.position() + delta;
        let candidate_box = Aabb::from_feet(
            candidate_position,
            entity.dimensions().width,
            entity.dimensions().height,
        );
        if !overlaps(candidate_box, player_box) && !intersects_solid(candidate_box, is_solid) {
            entity.set_position(candidate_position);
            return true;
        }
    }
    false
}

/// 生成两个水平推出候选（沿 X 或沿 Z），优先位移较小的轴。
fn push_candidates(player_box: Aabb, entity_box: Aabb) -> Vec<Vec3> {
    let player_center = (player_box.min + player_box.max) * 0.5;
    let entity_center = (entity_box.min + entity_box.max) * 0.5;

    let x_delta = if entity_center.x >= player_center.x {
        -(player_box.max.x - entity_box.min.x + AABB_EPSILON)
    } else {
        entity_box.max.x - player_box.min.x + AABB_EPSILON
    };
    let z_delta = if entity_center.z >= player_center.z {
        -(player_box.max.z - entity_box.min.z + AABB_EPSILON)
    } else {
        entity_box.max.z - player_box.min.z + AABB_EPSILON
    };

    let x_axis = Vec3::new(x_delta, 0.0, 0.0);
    let z_axis = Vec3::new(0.0, 0.0, z_delta);
    if x_delta.abs() <= z_delta.abs() {
        vec![x_axis, z_axis]
    } else {
        vec![z_axis, x_axis]
    }
}

/// 射线到指定方块的最小距离，用于比较生物与方块谁更近。
pub fn ray_block_distance(
    origin: Vec3,
    direction: Vec3,
    block: IVec3,
    max_distance: f32,
) -> Option<f32> {
    let aabb = Aabb {
        min: block.as_vec3(),
        max: block.as_vec3() + Vec3::ONE,
    };
    ray_aabb(origin, direction, max_distance, aabb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::creature::Creature;
    use crate::entity::entity::{Entity, EntityDimensions, EntityId, EntityTypeId};

    fn creature(id: u64, position: Vec3) -> Creature {
        Creature::new(Entity::new(
            EntityId::new(id),
            EntityTypeId::PIG,
            position,
            0.0,
            EntityDimensions::new(0.9, 0.9),
            10,
        ))
    }

    fn solid_none(_x: i32, _y: i32, _z: i32) -> bool {
        false
    }

    #[test]
    fn overlap_detects_penetration_only() {
        let a = Aabb::from_feet(Vec3::ZERO, 1.0, 1.0);
        let b = Aabb::from_feet(Vec3::new(0.5, 0.0, 0.0), 1.0, 1.0);
        let touching = Aabb::from_feet(Vec3::new(1.0, 0.0, 0.0), 1.0, 1.0);
        assert!(overlaps(a, b));
        assert!(!overlaps(a, touching));
    }

    #[test]
    fn ray_hits_near_face_of_box() {
        let aabb = Aabb::from_feet(Vec3::new(0.0, 0.0, 3.0), 2.0, 2.0);
        let distance = ray_aabb(Vec3::ZERO, Vec3::Z, 10.0, aabb).unwrap();
        assert!((distance - 2.0).abs() < 1e-4);
    }

    #[test]
    fn ray_misses_lateral_box() {
        let aabb = Aabb::from_feet(Vec3::new(3.0, 0.0, 3.0), 1.0, 1.0);
        assert!(ray_aabb(Vec3::ZERO, Vec3::Z, 10.0, aabb).is_none());
        assert!(ray_aabb(
            Vec3::ZERO,
            Vec3::Z,
            1.0,
            Aabb::from_feet(Vec3::new(0.0, 0.0, 3.0), 2.0, 2.0)
        )
        .is_none());
    }

    #[test]
    fn raycast_picks_nearest_creature() {
        let creatures = vec![
            creature(1, Vec3::new(0.0, 0.0, 6.0)),
            creature(2, Vec3::new(0.0, 0.0, 3.0)),
        ];
        let hit = raycast_entities(&creatures, Vec3::ZERO, Vec3::Z, 8.0).unwrap();
        assert_eq!(hit.0, EntityId::new(2));
        assert!(hit.1 < 3.0);
    }

    #[test]
    fn dead_creature_not_pickable() {
        let mut creatures = vec![creature(1, Vec3::new(0.0, 0.0, 1.0))];
        creatures[0].entity.take_damage(10);
        assert!(raycast_entities(&creatures, Vec3::ZERO, Vec3::Z, 4.0).is_none());
    }

    #[test]
    fn player_is_pushed_out_of_creature() {
        let creatures = vec![creature(1, Vec3::ZERO)];
        let mut position = Vec3::new(0.2, 0.0, 0.0);
        let adjusted = resolve_player_overlaps(&mut position, 0.6, 1.8, &creatures, &solid_none);
        assert_eq!(adjusted, 1);
        assert!((position.x - 0.2).abs() > 0.0);
        let player_box = Aabb::from_feet(position, 0.6, 1.8);
        assert!(!overlaps(player_box, creatures[0].entity.aabb()));
    }

    #[test]
    fn creature_is_pushed_out_of_player() {
        let mut creature = creature(1, Vec3::ZERO);
        let player_box = Aabb::from_feet(Vec3::new(0.2, 0.0, 0.0), 0.6, 1.8);
        assert!(push_creature_out(
            &mut creature.entity,
            player_box,
            &solid_none
        ));
        assert!(!overlaps(creature.entity.aabb(), player_box));
    }

    #[test]
    fn separated_positions_are_not_adjusted() {
        let creatures = vec![creature(1, Vec3::new(5.0, 0.0, 0.0))];
        let mut position = Vec3::ZERO;
        assert_eq!(
            resolve_player_overlaps(&mut position, 0.6, 1.8, &creatures, &solid_none),
            0
        );
    }
}
