//! 实体基础结构：稳定 ID、类型 ID、生命周期、碰撞尺寸与单个实体实例。
//!
//! 实体只保存自身的状态（位置、速度、生命、所属区块等），不持有世界或玩家引用；
//! 物理、AI、渲染等能力分别由同目录下的独立模块实现。

use glam::Vec3;

use crate::player::collision::Aabb;
use crate::world::ChunkPos;

/// 实体的稳定唯一标识；ID 在管理器内单调递增，存档恢复时保留原值。
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct EntityId(u64);

impl EntityId {
    /// 用原始数值构造。
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// 原始数值，用于存档与调试输出。
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// 实体类型标识；与具体生物/实体定义一一对应。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EntityTypeId(u16);

impl EntityTypeId {
    /// 猪。
    pub const PIG: Self = Self(1);
    /// 牛。
    pub const COW: Self = Self(2);

    /// 由原始数值构造。
    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    /// 原始数值，用于存档。
    pub const fn raw(self) -> u16 {
        self.0
    }
}

/// 生命周期：死亡后仍会短暂保留以完成掉落与清理，随后由管理器销毁。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityLifecycle {
    /// 正常活动。
    Active,
    /// 生命归零，等待清理。
    Dead,
}

/// 实体占用的碰撞箱尺寸（水平宽度与高度）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityDimensions {
    /// 水平宽度，碰撞箱为正方形底面。
    pub width: f32,
    /// 高度。
    pub height: f32,
}

impl EntityDimensions {
    /// 构造碰撞尺寸。
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// 一次伤害结算的结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageResult {
    /// 无效伤害（目标已死亡或伤害为 0）。
    Ignored,
    /// 命中但未致死，返回剩余生命。
    Applied { remaining: u32 },
    /// 本次伤害导致目标死亡。
    Killed,
}

/// 一个实体实例的状态。
#[derive(Clone, Debug)]
pub struct Entity {
    id: EntityId,
    kind: EntityTypeId,
    position: Vec3,
    previous_position: Vec3,
    velocity: Vec3,
    dimensions: EntityDimensions,
    health: u32,
    max_health: u32,
    yaw: f32,
    lifecycle: EntityLifecycle,
    grounded: bool,
    /// 当前所属区块，随位置移动自动更新。
    chunk: ChunkPos,
    /// 死亡后经过的时间，用于延迟销毁。
    death_elapsed: f32,
}

impl Entity {
    /// 创建一个满血的活动实体。
    pub fn new(
        id: EntityId,
        kind: EntityTypeId,
        position: Vec3,
        yaw: f32,
        dimensions: EntityDimensions,
        max_health: u32,
    ) -> Self {
        Self {
            id,
            kind,
            position,
            previous_position: position,
            velocity: Vec3::ZERO,
            dimensions,
            health: max_health.max(1),
            max_health: max_health.max(1),
            yaw,
            lifecycle: EntityLifecycle::Active,
            grounded: false,
            chunk: chunk_of(position),
            death_elapsed: 0.0,
        }
    }

    /// 稳定 ID。
    pub fn id(&self) -> EntityId {
        self.id
    }

    /// 实体类型。
    pub fn kind(&self) -> EntityTypeId {
        self.kind
    }

    /// 当前位置（脚底中心）。
    pub fn position(&self) -> Vec3 {
        self.position
    }

    /// 上一物理步的位置，用于渲染插值。
    pub fn previous_position(&self) -> Vec3 {
        self.previous_position
    }

    /// 当前速度。
    pub fn velocity(&self) -> Vec3 {
        self.velocity
    }

    /// 碰撞尺寸。
    pub fn dimensions(&self) -> EntityDimensions {
        self.dimensions
    }

    /// 当前生命。
    pub fn health(&self) -> u32 {
        self.health
    }

    /// 朝向角（弧度），0 表示面向 +Z。
    pub fn yaw(&self) -> f32 {
        self.yaw
    }

    /// 生命周期。
    pub fn lifecycle(&self) -> EntityLifecycle {
        self.lifecycle
    }

    /// 是否存活。
    pub fn is_alive(&self) -> bool {
        self.lifecycle == EntityLifecycle::Active
    }

    /// 是否已死亡。
    pub fn is_dead(&self) -> bool {
        self.lifecycle == EntityLifecycle::Dead
    }

    /// 是否站在地面。
    pub fn is_grounded(&self) -> bool {
        self.grounded
    }

    /// 所属区块。
    pub fn chunk(&self) -> ChunkPos {
        self.chunk
    }

    /// 记录当前位置，供下一物理步做渲染插值。
    pub fn snapshot_position(&mut self) {
        self.previous_position = self.position;
    }

    /// 设置位置并同步所属区块。
    pub fn set_position(&mut self, position: Vec3) {
        self.position = position;
        self.chunk = chunk_of(position);
    }

    /// 设置速度。
    pub fn set_velocity(&mut self, velocity: Vec3) {
        self.velocity = velocity;
    }

    /// 设置朝向角。
    pub fn set_yaw(&mut self, yaw: f32) {
        self.yaw = yaw;
    }

    /// 设置落地状态。
    pub fn set_grounded(&mut self, grounded: bool) {
        self.grounded = grounded;
    }

    /// 设置生命值，自动夹在 1..=上限之间。
    pub fn set_health(&mut self, health: u32) {
        self.health = health.clamp(1, self.max_health);
    }

    /// 结算伤害；已死亡目标返回 [`DamageResult::Ignored`]，因此重复调用不会重复触发掉落。
    pub fn take_damage(&mut self, amount: u32) -> DamageResult {
        if !self.is_alive() || amount == 0 {
            return DamageResult::Ignored;
        }
        let remaining = self.health.saturating_sub(amount);
        self.health = remaining;
        if remaining == 0 {
            self.lifecycle = EntityLifecycle::Dead;
            self.death_elapsed = 0.0;
            DamageResult::Killed
        } else {
            DamageResult::Applied { remaining }
        }
    }

    /// 推进死亡计时；返回 `true` 表示已超过 `linger` 秒，可以销毁。
    pub fn tick_death(&mut self, delta: f32, linger: f32) -> bool {
        if !self.is_dead() {
            return false;
        }
        self.death_elapsed += delta.max(0.0);
        self.death_elapsed >= linger
    }

    /// 实体的世界碰撞箱。
    pub fn aabb(&self) -> Aabb {
        Aabb::from_feet(self.position, self.dimensions.width, self.dimensions.height)
    }
}

/// 位置所属的区块坐标；使用 `floor` 保证负坐标也落在正确区块内。
pub fn chunk_of(position: Vec3) -> ChunkPos {
    ChunkPos::from_world(
        position.x.floor() as i32,
        position.y.floor() as i32,
        position.z.floor() as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::CHUNK_SIZE;

    fn probe() -> Entity {
        Entity::new(
            EntityId::new(7),
            EntityTypeId::PIG,
            Vec3::new(1.5, 10.0, 2.5),
            0.0,
            EntityDimensions::new(0.9, 0.9),
            10,
        )
    }

    #[test]
    fn chunk_uses_floor_for_negative_coordinates() {
        let pos = chunk_of(Vec3::new(-0.1, -0.1, -16.0));
        assert_eq!(pos, ChunkPos::new(-1, -1, -1));
        assert_eq!(chunk_of(Vec3::new(0.0, 0.0, 0.0)), ChunkPos::new(0, 0, 0));
    }

    #[test]
    fn moving_updates_owning_chunk() {
        let mut entity = probe();
        let target = Vec3::new(CHUNK_SIZE as f32 + 1.0, 10.0, 2.5);
        entity.set_position(target);
        assert_eq!(entity.chunk(), ChunkPos::new(1, 0, 0));
    }

    #[test]
    fn repeated_damage_on_dead_entity_is_ignored() {
        let mut entity = probe();
        assert_eq!(
            entity.take_damage(4),
            DamageResult::Applied { remaining: 6 }
        );
        assert_eq!(entity.take_damage(20), DamageResult::Killed);
        assert_eq!(entity.take_damage(3), DamageResult::Ignored);
        assert!(entity.is_dead());
    }

    #[test]
    fn death_linger_expires_once() {
        let mut entity = probe();
        entity.take_damage(10);
        assert!(!entity.tick_death(0.2, 0.6));
        assert!(entity.tick_death(0.5, 0.6));
    }

    #[test]
    fn collision_box_sits_on_feet() {
        let entity = probe();
        let aabb = entity.aabb();
        assert!((aabb.min.y - 10.0).abs() < f32::EPSILON);
        assert!((aabb.max.y - 10.9).abs() < 1e-5);
    }
}
