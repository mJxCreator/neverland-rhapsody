//! 物品注册表：集中定义物品名称、堆叠上限、图标与关联方块。
//!
//! 注册表按稳定物品 ID 索引；新增物品只能追加到末尾，不得调整已有 ID，
//! 否则旧存档中的物品数据会错位。

use glam::Vec3;

use super::item::{ItemDefinition, ItemIcon, ItemId, ItemType};
use crate::world::Block;

/// 基础方块物品的统一堆叠上限。
pub const DEFAULT_MAX_STACK_SIZE: u32 = 64;

const INVALID: ItemDefinition = ItemDefinition {
    id: ItemId::INVALID,
    name: "Air",
    max_stack_size: 0,
    icon: ItemIcon::NONE,
    associated_block: None,
    item_type: ItemType::Material,
};

const STONE: ItemDefinition = ItemDefinition {
    id: ItemId::STONE,
    name: "Stone",
    max_stack_size: DEFAULT_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.55, 0.55, 0.58),
    },
    associated_block: Some(Block::Stone),
    item_type: ItemType::Block,
};

const DIRT: ItemDefinition = ItemDefinition {
    id: ItemId::DIRT,
    name: "Dirt",
    max_stack_size: DEFAULT_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.45, 0.33, 0.22),
    },
    associated_block: Some(Block::Dirt),
    item_type: ItemType::Block,
};

const GRASS: ItemDefinition = ItemDefinition {
    id: ItemId::GRASS,
    name: "Grass",
    max_stack_size: DEFAULT_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.35, 0.62, 0.32),
    },
    associated_block: Some(Block::Grass),
    item_type: ItemType::Block,
};

/// 工具的统一堆叠上限：带独立耐久的工具不能堆叠。
pub const TOOL_MAX_STACK_SIZE: u32 = 1;

const WOODEN_PICKAXE: ItemDefinition = ItemDefinition {
    id: ItemId::WOODEN_PICKAXE,
    name: "Wooden Pickaxe",
    max_stack_size: TOOL_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.62, 0.43, 0.24),
    },
    associated_block: None,
    item_type: ItemType::Tool,
};

const STONE_PICKAXE: ItemDefinition = ItemDefinition {
    id: ItemId::STONE_PICKAXE,
    name: "Stone Pickaxe",
    max_stack_size: TOOL_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.58, 0.58, 0.62),
    },
    associated_block: None,
    item_type: ItemType::Tool,
};

const IRON_PICKAXE: ItemDefinition = ItemDefinition {
    id: ItemId::IRON_PICKAXE,
    name: "Iron Pickaxe",
    max_stack_size: TOOL_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.80, 0.82, 0.88),
    },
    associated_block: None,
    item_type: ItemType::Tool,
};

const WOODEN_AXE: ItemDefinition = ItemDefinition {
    id: ItemId::WOODEN_AXE,
    name: "Wooden Axe",
    max_stack_size: TOOL_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.68, 0.42, 0.20),
    },
    associated_block: None,
    item_type: ItemType::Tool,
};

const STONE_AXE: ItemDefinition = ItemDefinition {
    id: ItemId::STONE_AXE,
    name: "Stone Axe",
    max_stack_size: TOOL_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.60, 0.57, 0.50),
    },
    associated_block: None,
    item_type: ItemType::Tool,
};

const IRON_AXE: ItemDefinition = ItemDefinition {
    id: ItemId::IRON_AXE,
    name: "Iron Axe",
    max_stack_size: TOOL_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.85, 0.82, 0.74),
    },
    associated_block: None,
    item_type: ItemType::Tool,
};

const WOODEN_SHOVEL: ItemDefinition = ItemDefinition {
    id: ItemId::WOODEN_SHOVEL,
    name: "Wooden Shovel",
    max_stack_size: TOOL_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.55, 0.55, 0.35),
    },
    associated_block: None,
    item_type: ItemType::Tool,
};

const STONE_SHOVEL: ItemDefinition = ItemDefinition {
    id: ItemId::STONE_SHOVEL,
    name: "Stone Shovel",
    max_stack_size: TOOL_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.52, 0.58, 0.58),
    },
    associated_block: None,
    item_type: ItemType::Tool,
};

const IRON_SHOVEL: ItemDefinition = ItemDefinition {
    id: ItemId::IRON_SHOVEL,
    name: "Iron Shovel",
    max_stack_size: TOOL_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.78, 0.86, 0.88),
    },
    associated_block: None,
    item_type: ItemType::Tool,
};

const WOOD: ItemDefinition = ItemDefinition {
    id: ItemId::WOOD,
    name: "Wood",
    max_stack_size: DEFAULT_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.48, 0.32, 0.18),
    },
    associated_block: Some(Block::Wood),
    item_type: ItemType::Block,
};

const STICK: ItemDefinition = ItemDefinition {
    id: ItemId::STICK,
    name: "Stick",
    max_stack_size: DEFAULT_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.66, 0.50, 0.28),
    },
    associated_block: None,
    item_type: ItemType::Material,
};

const IRON_INGOT: ItemDefinition = ItemDefinition {
    id: ItemId::IRON_INGOT,
    name: "Iron Ingot",
    max_stack_size: DEFAULT_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.86, 0.87, 0.90),
    },
    associated_block: None,
    item_type: ItemType::Material,
};

const RAW_PORKCHOP: ItemDefinition = ItemDefinition {
    id: ItemId::RAW_PORKCHOP,
    name: "Raw Porkchop",
    max_stack_size: DEFAULT_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.92, 0.50, 0.53),
    },
    associated_block: None,
    item_type: ItemType::Material,
};

const RAW_BEEF: ItemDefinition = ItemDefinition {
    id: ItemId::RAW_BEEF,
    name: "Raw Beef",
    max_stack_size: DEFAULT_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.70, 0.22, 0.22),
    },
    associated_block: None,
    item_type: ItemType::Material,
};

const LEATHER: ItemDefinition = ItemDefinition {
    id: ItemId::LEATHER,
    name: "Leather",
    max_stack_size: DEFAULT_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.55, 0.38, 0.22),
    },
    associated_block: None,
    item_type: ItemType::Material,
};

const DIAMOND: ItemDefinition = ItemDefinition {
    id: ItemId::DIAMOND,
    name: "Diamond",
    max_stack_size: DEFAULT_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.35, 0.85, 0.86),
    },
    associated_block: None,
    item_type: ItemType::Material,
};

const WOODEN_SWORD: ItemDefinition = ItemDefinition {
    id: ItemId::WOODEN_SWORD,
    name: "Wooden Sword",
    max_stack_size: TOOL_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.66, 0.46, 0.25),
    },
    associated_block: None,
    item_type: ItemType::Weapon,
};

const STONE_SWORD: ItemDefinition = ItemDefinition {
    id: ItemId::STONE_SWORD,
    name: "Stone Sword",
    max_stack_size: TOOL_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.60, 0.60, 0.64),
    },
    associated_block: None,
    item_type: ItemType::Weapon,
};

const IRON_SWORD: ItemDefinition = ItemDefinition {
    id: ItemId::IRON_SWORD,
    name: "Iron Sword",
    max_stack_size: TOOL_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.84, 0.86, 0.92),
    },
    associated_block: None,
    item_type: ItemType::Weapon,
};

const DIAMOND_SWORD: ItemDefinition = ItemDefinition {
    id: ItemId::DIAMOND_SWORD,
    name: "Diamond Sword",
    max_stack_size: TOOL_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.40, 0.92, 0.92),
    },
    associated_block: None,
    item_type: ItemType::Weapon,
};

macro_rules! armor_definition {
    ($name:ident, $id:expr, $label:expr, $color:expr) => {
        const $name: ItemDefinition = ItemDefinition {
            id: $id,
            name: $label,
            max_stack_size: TOOL_MAX_STACK_SIZE,
            icon: ItemIcon { color: $color },
            associated_block: None,
            item_type: ItemType::Armor,
        };
    };
}

armor_definition!(
    LEATHER_HELMET,
    ItemId::LEATHER_HELMET,
    "Leather Helmet",
    Vec3::new(0.58, 0.39, 0.23)
);
armor_definition!(
    LEATHER_CHESTPLATE,
    ItemId::LEATHER_CHESTPLATE,
    "Leather Chestplate",
    Vec3::new(0.60, 0.40, 0.24)
);
armor_definition!(
    LEATHER_LEGGINGS,
    ItemId::LEATHER_LEGGINGS,
    "Leather Leggings",
    Vec3::new(0.56, 0.37, 0.22)
);
armor_definition!(
    LEATHER_BOOTS,
    ItemId::LEATHER_BOOTS,
    "Leather Boots",
    Vec3::new(0.54, 0.36, 0.21)
);
armor_definition!(
    IRON_HELMET,
    ItemId::IRON_HELMET,
    "Iron Helmet",
    Vec3::new(0.82, 0.84, 0.88)
);
armor_definition!(
    IRON_CHESTPLATE,
    ItemId::IRON_CHESTPLATE,
    "Iron Chestplate",
    Vec3::new(0.84, 0.86, 0.90)
);
armor_definition!(
    IRON_LEGGINGS,
    ItemId::IRON_LEGGINGS,
    "Iron Leggings",
    Vec3::new(0.80, 0.82, 0.86)
);
armor_definition!(
    IRON_BOOTS,
    ItemId::IRON_BOOTS,
    "Iron Boots",
    Vec3::new(0.78, 0.80, 0.84)
);
armor_definition!(
    DIAMOND_HELMET,
    ItemId::DIAMOND_HELMET,
    "Diamond Helmet",
    Vec3::new(0.38, 0.90, 0.90)
);
armor_definition!(
    DIAMOND_CHESTPLATE,
    ItemId::DIAMOND_CHESTPLATE,
    "Diamond Chestplate",
    Vec3::new(0.42, 0.94, 0.94)
);
armor_definition!(
    DIAMOND_LEGGINGS,
    ItemId::DIAMOND_LEGGINGS,
    "Diamond Leggings",
    Vec3::new(0.36, 0.88, 0.88)
);
armor_definition!(
    DIAMOND_BOOTS,
    ItemId::DIAMOND_BOOTS,
    "Diamond Boots",
    Vec3::new(0.34, 0.86, 0.86)
);

const ROTTEN_FLESH: ItemDefinition = ItemDefinition {
    id: ItemId::ROTTEN_FLESH,
    name: "Rotten Flesh",
    max_stack_size: DEFAULT_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.45, 0.28, 0.26),
    },
    associated_block: None,
    item_type: ItemType::Material,
};

const COOKED_PORKCHOP: ItemDefinition = ItemDefinition {
    id: ItemId::COOKED_PORKCHOP,
    name: "Cooked Porkchop",
    max_stack_size: DEFAULT_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.74, 0.44, 0.26),
    },
    associated_block: None,
    item_type: ItemType::Consumable,
};

const COOKED_BEEF: ItemDefinition = ItemDefinition {
    id: ItemId::COOKED_BEEF,
    name: "Steak",
    max_stack_size: DEFAULT_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.62, 0.30, 0.20),
    },
    associated_block: None,
    item_type: ItemType::Consumable,
};

const APPLE: ItemDefinition = ItemDefinition {
    id: ItemId::APPLE,
    name: "Apple",
    max_stack_size: DEFAULT_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.82, 0.16, 0.18),
    },
    associated_block: None,
    item_type: ItemType::Consumable,
};

const BREAD: ItemDefinition = ItemDefinition {
    id: ItemId::BREAD,
    name: "Bread",
    max_stack_size: DEFAULT_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.76, 0.58, 0.28),
    },
    associated_block: None,
    item_type: ItemType::Consumable,
};

const CAMPFIRE: ItemDefinition = ItemDefinition {
    id: ItemId::CAMPFIRE,
    name: "Campfire",
    max_stack_size: DEFAULT_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.70, 0.38, 0.16),
    },
    associated_block: Some(Block::Campfire),
    item_type: ItemType::Block,
};

const BED: ItemDefinition = ItemDefinition {
    id: ItemId::BED,
    name: "Bed",
    max_stack_size: DEFAULT_MAX_STACK_SIZE,
    icon: ItemIcon {
        color: Vec3::new(0.72, 0.20, 0.22),
    },
    associated_block: Some(Block::Bed),
    item_type: ItemType::Block,
};

/// 全部物品定义，下标必须等于物品 ID。
const DEFINITIONS: [ItemDefinition; 43] = [
    INVALID,
    STONE,
    DIRT,
    GRASS,
    WOODEN_PICKAXE,
    STONE_PICKAXE,
    IRON_PICKAXE,
    WOODEN_AXE,
    STONE_AXE,
    IRON_AXE,
    WOODEN_SHOVEL,
    STONE_SHOVEL,
    IRON_SHOVEL,
    WOOD,
    STICK,
    IRON_INGOT,
    RAW_PORKCHOP,
    RAW_BEEF,
    LEATHER,
    DIAMOND,
    WOODEN_SWORD,
    STONE_SWORD,
    IRON_SWORD,
    DIAMOND_SWORD,
    LEATHER_HELMET,
    LEATHER_CHESTPLATE,
    LEATHER_LEGGINGS,
    LEATHER_BOOTS,
    IRON_HELMET,
    IRON_CHESTPLATE,
    IRON_LEGGINGS,
    IRON_BOOTS,
    DIAMOND_HELMET,
    DIAMOND_CHESTPLATE,
    DIAMOND_LEGGINGS,
    DIAMOND_BOOTS,
    ROTTEN_FLESH,
    COOKED_PORKCHOP,
    COOKED_BEEF,
    APPLE,
    BREAD,
    CAMPFIRE,
    BED,
];

/// 按物品 ID 查询定义；无效 ID 返回 `None`。
pub fn definition(item: ItemId) -> Option<&'static ItemDefinition> {
    DEFINITIONS
        .get(item.raw() as usize)
        .filter(|definition| definition.id != ItemId::INVALID && definition.id == item)
}

/// 某种方块对应的物品；不可掉落的方块返回 `None`。
///
/// 当前由掉落映射与测试使用，作为方块到物品的反向查询保留。
#[allow(dead_code)]
pub fn item_for_block(block: Block) -> Option<ItemId> {
    DEFINITIONS
        .iter()
        .find(|definition| {
            definition.id != ItemId::INVALID && definition.associated_block == Some(block)
        })
        .map(|definition| definition.id)
}

/// 物品关联的可放置方块；非方块物品或无效 ID 返回 `None`。
pub fn block_for_item(item: ItemId) -> Option<Block> {
    let definition = definition(item)?;
    let block = definition.associated_block?;
    definition.is_placeable().then_some(block)
}

/// 物品的最大堆叠数量；无效 ID 返回 `None`。
pub fn max_stack_size(item: ItemId) -> Option<u32> {
    definition(item).map(|definition| definition.max_stack_size)
}

/// 物品展示名称；无效 ID 返回 `"?"`。
pub fn name(item: ItemId) -> &'static str {
    definition(item).map_or("?", |definition| definition.name)
}

/// 物品图标颜色；无效 ID 返回透明黑。
pub fn icon_color(item: ItemId) -> Vec3 {
    definition(item).map_or(Vec3::ZERO, |definition| definition.icon.color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_match_table_order() {
        let definitions: Vec<ItemDefinition> = DEFINITIONS
            .iter()
            .copied()
            .filter(|definition| definition.id != ItemId::INVALID)
            .collect();
        for definition in &definitions {
            assert_eq!(
                DEFINITIONS.get(definition.id.raw() as usize),
                Some(definition)
            );
            if let Some(block) = definition.associated_block {
                assert_eq!(item_for_block(block), Some(definition.id));
            }
        }
        let mut ids: Vec<u16> = definitions
            .iter()
            .map(|definition| definition.id.raw())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), definitions.len(), "物品 ID 必须唯一");
    }

    #[test]
    fn invalid_ids_are_rejected_by_every_lookup() {
        assert!(definition(ItemId::INVALID).is_none());
        assert!(block_for_item(ItemId::INVALID).is_none());
        assert!(max_stack_size(ItemId::INVALID).is_none());
        assert_eq!(name(ItemId::INVALID), "?");
        assert_eq!(icon_color(ItemId::INVALID), Vec3::ZERO);
    }

    #[test]
    fn block_items_map_both_directions() {
        assert_eq!(item_for_block(Block::Stone), Some(ItemId::STONE));
        assert_eq!(block_for_item(ItemId::STONE), Some(Block::Stone));
        assert_eq!(item_for_block(Block::Air), None);
        assert_eq!(block_for_item(ItemId::INVALID), None);
    }

    #[test]
    fn every_block_item_stacks_to_the_shared_limit() {
        for item in [ItemId::STONE, ItemId::DIRT, ItemId::GRASS, ItemId::WOOD] {
            let definition = definition(item).expect("物品应存在");
            assert_eq!(definition.max_stack_size, DEFAULT_MAX_STACK_SIZE);
            assert!(definition.is_placeable());
        }
    }

    #[test]
    fn crafting_materials_are_stackable_and_not_placeable() {
        for item in [ItemId::STICK, ItemId::IRON_INGOT] {
            let definition = definition(item).expect("材料应存在");
            assert_eq!(definition.item_type, ItemType::Material);
            assert_eq!(definition.max_stack_size, DEFAULT_MAX_STACK_SIZE);
            assert!(!definition.is_placeable());
            assert_eq!(item_for_block(Block::Wood), Some(ItemId::WOOD));
        }
    }

    #[test]
    fn creature_drop_items_are_registered_materials() {
        for item in [ItemId::RAW_PORKCHOP, ItemId::RAW_BEEF, ItemId::LEATHER] {
            let definition = definition(item).expect("生物掉落物品应已注册");
            assert_eq!(definition.item_type, ItemType::Material);
            assert_eq!(definition.max_stack_size, DEFAULT_MAX_STACK_SIZE);
            assert_eq!(block_for_item(item), None);
        }
        assert_eq!(ItemId::RAW_PORKCHOP.raw(), 16);
        assert_eq!(ItemId::RAW_BEEF.raw(), 17);
        assert_eq!(ItemId::LEATHER.raw(), 18);
    }

    #[test]
    fn tool_items_cannot_stack_and_are_not_placeable() {
        for tool in crate::item::tool_registry::definitions() {
            let definition = definition(tool.id).expect("工具必须有物品定义");
            assert_eq!(definition.max_stack_size, TOOL_MAX_STACK_SIZE);
            assert_eq!(definition.item_type, ItemType::Tool);
            assert!(!definition.is_placeable());
            assert_eq!(block_for_item(tool.id), None);
        }
    }
}
