//! 存档层：按区块记录玩家对程序化地形的改动，并在区块加载时重新应用。
//!
//! 角色快照（背包/生命/装备/饥饿体力）与世界存档（地形改动/掉落物/生物/时间/营火）
//! 分开存放：一个角色可以进入任意世界，退出时两者各自冻结。

pub mod character_save;
pub mod library;
pub mod world_save;

pub use self::character_save::{CharacterSave, CHARACTER_SAVE_VERSION};
pub use self::library::{SaveEntry, SaveLibrary};
pub use self::world_save::{BlockEdit, WorldSave};
