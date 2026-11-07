//! 玩家层：实体状态、输入意图、物理世界查询、AABB 碰撞与固定步长物理。

pub mod collision;
pub mod controller;
pub mod physics;
pub mod physics_world;
// 玩家实体位于 player/player.rs，保留与模块同名的子模块以贴近组件命名。
#[allow(clippy::module_inception)]
pub mod player;

pub use self::controller::PlayerController;
pub use self::physics::{PhysicsConfig, PhysicsSystem};
pub use self::physics_world::PhysicsWorld;
pub use self::player::Player;
