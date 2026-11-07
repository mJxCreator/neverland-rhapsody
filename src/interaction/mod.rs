//! 交互层：视角射线检测、持续挖掘、方块放置规则与快捷栏选择。

pub mod hotbar;
pub mod mining;
pub mod placement;
pub mod raycast;

pub use self::hotbar::{Hotbar, DEFAULT_SLOTS, HOTBAR_SLOTS};
pub use self::mining::{MiningOutcome, MiningState, MiningTarget};
pub use self::placement::placement_cell;
pub use self::raycast::{raycast, RayHit};
