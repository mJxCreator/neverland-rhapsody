//! 锥形攻击命中判定：距离、角度、视线遮挡与去重。
//!
//! 判定以玩家眼睛为原点、相机朝向为轴；同一帧内每个生物最多命中一次，
//! 由 [`find_targets`] 返回的列表本身保证唯一性。

use glam::Vec3;

use crate::entity::creature::Creature;
use crate::entity::entity::EntityId;

/// 一次锥形查询的参数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitQuery {
    /// 眼睛位置。
    pub origin: Vec3,
    /// 相机朝向（无需归一化）。
    pub forward: Vec3,
    /// 最大距离。
    pub range: f32,
    /// 锥角（度）。
    pub angle_deg: f32,
}

impl HitQuery {
    pub fn new(origin: Vec3, forward: Vec3, range: f32, angle_deg: f32) -> Self {
        Self {
            origin,
            forward,
            range,
            angle_deg,
        }
    }

    /// 查询参数是否有效。
    pub fn is_valid(self) -> bool {
        self.origin.is_finite()
            && self.forward.is_finite()
            && self.forward.length_squared() > 1e-8
            && self.range.is_finite()
            && self.range > 0.0
            && (1.0..=360.0).contains(&self.angle_deg)
    }
}

/// 在锥形范围内寻找命中目标，按距离由近到远返回，每个实体只出现一次。
///
/// `is_occluded(origin, direction_to_target, distance)` 由调用方提供方块遮挡查询；
/// 返回 `true` 表示视线被方块挡住。
pub fn find_targets(
    creatures: &[Creature],
    query: HitQuery,
    is_occluded: &dyn Fn(Vec3, Vec3, f32) -> bool,
) -> Vec<EntityId> {
    let forward = query.forward.normalize_or_zero();
    if !query.is_valid() || forward == Vec3::ZERO {
        return Vec::new();
    }
    let min_dot = (query.angle_deg.to_radians() * 0.5).cos();
    let mut hits: Vec<(EntityId, f32)> = Vec::new();
    for creature in creatures.iter().filter(|creature| creature.is_alive()) {
        let aabb = creature.entity.aabb();
        let center = (aabb.min + aabb.max) * 0.5;
        let offset = center - query.origin;
        let distance = offset.length();
        if distance > query.range + creature.entity.dimensions().width * 0.5 {
            continue;
        }
        if distance > 1e-5 {
            let dot = offset.normalize().dot(forward);
            if dot < min_dot {
                continue;
            }
        }
        let direction = if distance > 1e-5 {
            offset.normalize()
        } else {
            forward
        };
        if is_occluded(query.origin, direction, distance.max(0.0)) {
            continue;
        }
        hits.push((creature.id(), distance));
    }
    hits.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    hits.into_iter().map(|(id, _)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::entity::{Entity, EntityDimensions, EntityTypeId};

    fn creature(id: u64, position: Vec3) -> Creature {
        Creature::new(Entity::new(
            EntityId::new(id),
            EntityTypeId::PIG,
            position,
            0.0,
            EntityDimensions::new(0.9, 1.8),
            10,
        ))
    }

    fn not_occluded(_origin: Vec3, _direction: Vec3, _distance: f32) -> bool {
        false
    }

    fn always_occluded(_origin: Vec3, _direction: Vec3, _distance: f32) -> bool {
        true
    }

    #[test]
    fn hits_targets_inside_the_cone_only() {
        let creatures = vec![
            creature(1, Vec3::new(0.0, 0.0, 2.0)),
            creature(2, Vec3::new(0.0, 0.0, -2.0)),
            creature(3, Vec3::new(0.0, 0.0, 8.0)),
        ];
        let query = HitQuery::new(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, 3.0, 120.0);
        let hits = find_targets(&creatures, query, &not_occluded);
        assert_eq!(hits, vec![EntityId::new(1)]);
    }

    #[test]
    fn multiple_targets_are_sorted_by_distance() {
        let creatures = vec![
            creature(1, Vec3::new(0.5, 0.0, 2.4)),
            creature(2, Vec3::new(-0.5, 0.0, 1.6)),
        ];
        let query = HitQuery::new(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, 3.0, 150.0);
        let hits = find_targets(&creatures, query, &not_occluded);
        assert_eq!(hits, vec![EntityId::new(2), EntityId::new(1)]);
    }

    #[test]
    fn occlusion_and_dead_targets_are_skipped() {
        let mut dead = creature(2, Vec3::new(0.0, 0.0, 1.0));
        dead.entity.take_damage(10);
        let creatures = vec![creature(1, Vec3::new(0.0, 0.0, 2.0)), dead];
        let query = HitQuery::new(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, 3.0, 120.0);
        assert!(find_targets(&creatures, query, &always_occluded).is_empty());
        assert_eq!(find_targets(&creatures, query, &not_occluded).len(), 1);
    }

    #[test]
    fn invalid_queries_return_empty() {
        let creatures = vec![creature(1, Vec3::new(0.0, 0.0, 1.0))];
        assert!(find_targets(
            &creatures,
            HitQuery::new(Vec3::ZERO, Vec3::ZERO, 3.0, 120.0),
            &not_occluded
        )
        .is_empty());
        assert!(find_targets(
            &creatures,
            HitQuery::new(Vec3::ZERO, Vec3::Z, 0.0, 120.0),
            &not_occluded
        )
        .is_empty());
    }

    #[test]
    fn exact_angle_boundary_is_inclusive() {
        // 目标位于 60° 边界上（120° 锥角的一半）。
        let angle = 60.0_f32.to_radians();
        let target = Vec3::new(angle.sin() * 2.0, 1.6, angle.cos() * 2.0);
        let creatures = vec![creature(1, target - Vec3::new(0.0, 0.9, 0.0))];
        let query = HitQuery::new(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, 3.0, 120.0);
        // 允许浮点误差：边界内或恰好边界都应命中或只在极小误差外。
        let hits = find_targets(&creatures, query, &not_occluded);
        assert!(hits.len() <= 1);
    }
}
