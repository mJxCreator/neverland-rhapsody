//! 玩家层：实体状态、输入意图、物理世界查询、AABB 碰撞、固定步长物理、
//! 生命值/伤害与死亡重生。

pub mod collision;
pub mod controller;
pub mod damage;
pub mod death;
pub mod physics;
pub mod physics_world;
// 玩家实体位于 player/player.rs，保留与模块同名的子模块以贴近组件命名。
#[allow(clippy::module_inception)]
pub mod player;
pub mod survival;

pub use self::controller::PlayerController;
pub use self::damage::{Damage, DamageSource};
pub use self::death::find_respawn_position;
pub use self::physics::{PhysicsConfig, PhysicsSystem, FIXED_TIMESTEP};
pub use self::physics_world::PhysicsWorld;
pub use self::player::Player;
pub use self::survival::{DamageOutcome, SavedSurvival, Survival};
