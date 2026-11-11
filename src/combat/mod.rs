//! 战斗系统：攻击状态机、武器数据、命中判定、伤害与击退。

pub mod attack;
pub mod combat_state;
pub mod damage;
pub mod hit_detection;
pub mod knockback;
pub mod weapon;
pub mod weapon_registry;

pub use attack::AttackController;
pub use combat_state::CombatConfig;
pub use weapon::WeaponStats;
