//! 生存系统：饥饿、体力、自然恢复、食物、世界时间、睡眠、复活点与营火烹饪。
//!
//! 本模块只承载可测试的纯逻辑，不直接依赖窗口、渲染或输入；主循环负责把
//! 游戏时间、玩家状态与界面状态接入这些纯逻辑。

pub mod cooking;
pub mod food;
pub mod food_registry;
pub mod hunger;
pub mod regeneration;
pub mod respawn;
pub mod rng;
pub mod sleep;
pub mod stamina;
// 与模块同名，保留便于按生存子系统目录查找。
#[allow(clippy::module_inception)]
pub mod survival;
pub mod world_time;

pub use survival::{SavedVitals, Vitals};
pub use world_time::{SavedWorldTime, WorldTime};
