//! 击退：把攻击方向转换为实体与玩家的速度冲量。
//!
//! 生物直接叠加水平速度并获得短暂上抛；玩家通过 [`crate::player::Player::apply_impulse`]
//! 注入同样的冲量，之后仍由各自的物理系统做碰撞与摩擦。

use glam::Vec3;

use crate::entity::entity::Entity;

/// 攻击对生物的上抛速度。
pub const CREATURE_LIFT: f32 = 3.4;
/// 生物攻击对玩家的水平击退速度。
pub const PLAYER_KNOCKBACK: f32 = 4.5;
/// 生物攻击对玩家的上抛速度。
pub const PLAYER_LIFT: f32 = 3.0;

/// 从攻击者指向目标的水平单位向量；重合时返回零向量。
pub fn horizontal_direction(from: Vec3, to: Vec3) -> Vec3 {
    let delta = Vec3::new(to.x - from.x, 0.0, to.z - from.z);
    delta.normalize_or_zero()
}

/// 给生物叠加击退速度；已死亡目标忽略。
pub fn apply_to_entity(entity: &mut Entity, attacker: Vec3, strength: f32) {
    if !entity.is_alive() {
        return;
    }
    let direction = horizontal_direction(attacker, entity.position());
    let mut velocity = entity.velocity();
    velocity.x += direction.x * strength;
    velocity.z += direction.z * strength;
    velocity.y = velocity.y.max(CREATURE_LIFT);
    entity.set_velocity(velocity);
}

/// 计算玩家应获得的冲量。
pub fn player_impulse(attacker: Vec3, player_position: Vec3) -> Vec3 {
    let direction = horizontal_direction(attacker, player_position);
    Vec3::new(
        direction.x * PLAYER_KNOCKBACK,
        PLAYER_LIFT,
        direction.z * PLAYER_KNOCKBACK,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::entity::{Entity, EntityDimensions, EntityId, EntityTypeId};

    fn entity() -> Entity {
        Entity::new(
            EntityId::new(1),
            EntityTypeId::PIG,
            Vec3::new(2.0, 0.0, 0.0),
            0.0,
            EntityDimensions::new(0.9, 0.9),
            10,
        )
    }

    #[test]
    fn knockback_points_away_from_attacker() {
        let mut entity = entity();
        apply_to_entity(&mut entity, Vec3::ZERO, PLAYER_KNOCKBACK);
        assert!(entity.velocity().x > 0.0);
        assert!(entity.velocity().y > 0.0);
    }

    #[test]
    fn coincident_positions_do_not_launch_horizontally() {
        let mut entity = entity();
        entity.set_position(Vec3::ZERO);
        apply_to_entity(&mut entity, Vec3::ZERO, PLAYER_KNOCKBACK);
        assert!((entity.velocity().x).abs() < 1e-6);
        assert!((entity.velocity().z).abs() < 1e-6);
    }

    #[test]
    fn player_impulse_is_away_and_upward() {
        let impulse = player_impulse(Vec3::new(1.0, 0.0, 0.0), Vec3::new(3.0, 0.0, 0.0));
        assert!(impulse.x > 0.0);
        assert!(impulse.y > 0.0);
    }

    #[test]
    fn dead_entities_are_not_knocked_back() {
        let mut entity = entity();
        entity.take_damage(10);
        apply_to_entity(&mut entity, Vec3::ZERO, PLAYER_KNOCKBACK);
        assert_eq!(entity.velocity(), Vec3::ZERO);
    }
}
