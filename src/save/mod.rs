//! 存档层：按区块记录玩家对程序化地形的改动，并在区块加载时重新应用。

pub mod world_save;

pub use self::world_save::{BlockEdit, WorldSave};
