//! 物品系统：物品定义与堆叠、工具定义与耐久、背包、方块掉落映射、世界掉落物、
//! 拾取以及 v0.14 的配方与合成执行。

pub mod block_drop;
pub mod crafting;
pub mod dropped_item;
pub mod durability;
pub mod inventory;
// 物品定义文件沿用 item.rs 命名，模块名与父模块相同。
#[allow(clippy::module_inception)]
pub mod item;
pub mod item_pickup;
pub mod item_registry;
pub mod recipe;
pub mod recipe_registry;
pub mod tool;
pub mod tool_registry;

pub use self::block_drop::{drops_for_block, BlockDrop};
pub use self::dropped_item::{DroppedItemConfig, DroppedItemRecord, DroppedItemStore};
pub use self::inventory::{Inventory, SavedInventory, HOTBAR_OFFSET, HOTBAR_SLOTS, TOTAL_SLOTS};
pub use self::item::{ItemId, ItemStack};
pub use self::item_pickup::collect as collect_pickups;
pub use self::item_registry::{block_for_item, icon_color, name as item_name};
pub use self::recipe::{Recipe, RecipeCategory, RecipeId};
pub use self::tool::Wear;
