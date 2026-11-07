//! 实体层：生物实例、注册表、物理、碰撞、AI、自然生成、存档记录与渲染。
//!
//! 实体系统只依赖世界方块查询与通用 AABB 工具，不读取玩家输入、不直接改动玩家状态；
//! 与玩家的互动通过显式的分离/伤害接口完成。

pub mod ai;
pub mod creature;
pub mod creature_ai;
pub mod creature_registry;
#[allow(clippy::module_inception)]
pub mod entity;
pub mod entity_collision;
pub mod entity_manager;
pub mod entity_physics;
pub mod entity_registry;
pub mod entity_render;
pub mod entity_save;
pub mod spawn;

pub use ai::AiConfig;
pub use entity::{DamageResult, EntityId, EntityTypeId};
pub use entity_manager::EntityManager;
pub use entity_physics::{EntityPhysicsConfig, EntityPhysicsSystem};
pub use entity_render::EntityRenderConfig;
pub use entity_save::EntityRecord;
pub use spawn::{SpawnConfig, Spawner};
