//! 存档层：把玩家对世界的方块改动写入磁盘，并在启动时重新应用。

pub mod world_save;

pub use self::world_save::{BlockEdit, WorldSave};
