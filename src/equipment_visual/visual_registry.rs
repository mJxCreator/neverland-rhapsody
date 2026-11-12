//! 装备与手持物品的视觉注册表。
//!
//! 视觉定义只描述“用哪个模型、挂在哪个挂载点、局部如何摆放”，
//! 装备属性、耐久与是否可装备仍由 `equipment` / `item` 系统决定。

use std::sync::OnceLock;

use glam::{Quat, Vec3};

use crate::attachment::AttachmentId;
use crate::item::ItemId;
use crate::model::types::Transform;

/// 物品在角色身上的展示方式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayMode {
    /// 手持模型：在右手显示，并由第一人称手臂持有。
    Handheld,
    /// 仅在 UI 显示图标，不挂到角色身上。
    IconOnly,
    /// 角色模型上完全不展示。
    Hidden,
}

/// 一个挂载部件。
#[derive(Clone, Debug, PartialEq)]
pub struct VisualPart {
    pub attachment: AttachmentId,
    pub model: String,
    pub local: Transform,
}

/// 一件物品的完整视觉定义。
#[derive(Clone, Debug, PartialEq)]
pub struct ItemVisualDefinition {
    pub item: ItemId,
    pub display: DisplayMode,
    pub parts: Vec<VisualPart>,
    /// 第一人称下相对手臂根节点的局部变换；仅 Handheld 物品使用。
    pub first_person: Transform,
}

/// 工具与武器的材质档位：名称、模型前缀与主色。
pub const TIERS: [(&str, [f32; 4]); 3] = [
    ("wooden", [0.58, 0.42, 0.24, 1.0]),
    ("stone", [0.50, 0.50, 0.52, 1.0]),
    ("iron", [0.80, 0.82, 0.86, 1.0]),
];

/// 工具种类名称后缀。
pub const TOOL_KINDS: [&str; 3] = ["pickaxe", "axe", "shovel"];

/// 护甲材质名称与颜色。
pub const ARMOR_MATERIALS: [(&str, [f32; 4]); 3] = [
    ("leather", [0.55, 0.37, 0.22, 1.0]),
    ("iron", [0.78, 0.80, 0.84, 1.0]),
    ("diamond", [0.34, 0.88, 0.84, 1.0]),
];

pub const TOOL_IDS: [[ItemId; 3]; 3] = [
    [
        ItemId::WOODEN_PICKAXE,
        ItemId::WOODEN_AXE,
        ItemId::WOODEN_SHOVEL,
    ],
    [
        ItemId::STONE_PICKAXE,
        ItemId::STONE_AXE,
        ItemId::STONE_SHOVEL,
    ],
    [ItemId::IRON_PICKAXE, ItemId::IRON_AXE, ItemId::IRON_SHOVEL],
];

const SWORD_IDS: [ItemId; 4] = [
    ItemId::WOODEN_SWORD,
    ItemId::STONE_SWORD,
    ItemId::IRON_SWORD,
    ItemId::DIAMOND_SWORD,
];

const SWORD_TIERS: [&str; 4] = ["wooden", "stone", "iron", "diamond"];

const ARMOR_IDS: [[ItemId; 4]; 3] = [
    [
        ItemId::LEATHER_HELMET,
        ItemId::LEATHER_CHESTPLATE,
        ItemId::LEATHER_LEGGINGS,
        ItemId::LEATHER_BOOTS,
    ],
    [
        ItemId::IRON_HELMET,
        ItemId::IRON_CHESTPLATE,
        ItemId::IRON_LEGGINGS,
        ItemId::IRON_BOOTS,
    ],
    [
        ItemId::DIAMOND_HELMET,
        ItemId::DIAMOND_CHESTPLATE,
        ItemId::DIAMOND_LEGGINGS,
        ItemId::DIAMOND_BOOTS,
    ],
];

const HANDHELD_FOODS: [(ItemId, &str); 6] = [
    (ItemId::APPLE, "apple"),
    (ItemId::BREAD, "bread"),
    (ItemId::RAW_PORKCHOP, "raw_porkchop"),
    (ItemId::RAW_BEEF, "raw_beef"),
    (ItemId::COOKED_PORKCHOP, "cooked_porkchop"),
    (ItemId::COOKED_BEEF, "cooked_beef"),
];

static DEFINITIONS: OnceLock<Vec<ItemVisualDefinition>> = OnceLock::new();

/// 全部有视觉定义的物品。
pub fn definitions() -> &'static [ItemVisualDefinition] {
    DEFINITIONS.get_or_init(build)
}

/// 查询物品的视觉定义。
pub fn definition(item: ItemId) -> Option<&'static ItemVisualDefinition> {
    definitions().iter().find(|entry| entry.item == item)
}

/// 查询展示方式；没有显式定义的物品按 `Hidden` 处理。
pub fn display_mode(item: ItemId) -> DisplayMode {
    definition(item)
        .map(|entry| entry.display)
        .unwrap_or(DisplayMode::Hidden)
}

/// 该物品是否应在手中显示模型。
pub fn is_handheld(item: ItemId) -> bool {
    display_mode(item) == DisplayMode::Handheld
}

fn build() -> Vec<ItemVisualDefinition> {
    let mut definitions = Vec::new();

    for (tier_index, (tier, _color)) in TIERS.iter().enumerate() {
        for (kind_index, kind) in TOOL_KINDS.iter().enumerate() {
            definitions.push(ItemVisualDefinition {
                item: TOOL_IDS[tier_index][kind_index],
                display: DisplayMode::Handheld,
                parts: vec![VisualPart {
                    attachment: AttachmentId::RightHand,
                    model: format!("{tier}_{kind}"),
                    local: tool_grip(),
                }],
                first_person: first_person_tool(),
            });
        }
    }

    for (index, item) in SWORD_IDS.into_iter().enumerate() {
        let tier = SWORD_TIERS[index];
        definitions.push(ItemVisualDefinition {
            item,
            display: DisplayMode::Handheld,
            parts: vec![VisualPart {
                attachment: AttachmentId::RightHand,
                model: format!("{tier}_sword"),
                local: sword_grip(),
            }],
            first_person: first_person_sword(),
        });
    }

    for (item, model) in HANDHELD_FOODS {
        definitions.push(ItemVisualDefinition {
            item,
            display: DisplayMode::Handheld,
            parts: vec![VisualPart {
                attachment: AttachmentId::RightHand,
                model: model.to_string(),
                local: food_grip(),
            }],
            first_person: first_person_food(),
        });
    }

    for (material_index, (material, _color)) in ARMOR_MATERIALS.iter().enumerate() {
        let ids = ARMOR_IDS[material_index];
        definitions.push(ItemVisualDefinition {
            item: ids[0],
            display: DisplayMode::IconOnly,
            parts: vec![VisualPart {
                attachment: AttachmentId::Head,
                model: format!("{material}_helmet"),
                local: offset(Vec3::new(0.0, 0.12, 0.0), Vec3::new(1.08, 1.0, 1.08)),
            }],
            first_person: Transform::IDENTITY,
        });
        definitions.push(ItemVisualDefinition {
            item: ids[1],
            display: DisplayMode::IconOnly,
            parts: vec![VisualPart {
                attachment: AttachmentId::Chest,
                model: format!("{material}_chestplate"),
                local: offset(Vec3::ZERO, Vec3::new(1.06, 1.0, 1.08)),
            }],
            first_person: Transform::IDENTITY,
        });
        definitions.push(ItemVisualDefinition {
            item: ids[2],
            display: DisplayMode::IconOnly,
            parts: vec![
                VisualPart {
                    attachment: AttachmentId::Legs,
                    model: format!("{material}_leggings_hip"),
                    local: offset(Vec3::new(0.0, -0.05, 0.0), Vec3::new(1.06, 1.0, 1.08)),
                },
                VisualPart {
                    attachment: AttachmentId::LeftLeg,
                    model: format!("{material}_leggings_left"),
                    local: offset(Vec3::new(0.0, -0.18, 0.0), Vec3::new(1.06, 1.0, 1.06)),
                },
                VisualPart {
                    attachment: AttachmentId::RightLeg,
                    model: format!("{material}_leggings_right"),
                    local: offset(Vec3::new(0.0, -0.18, 0.0), Vec3::new(1.06, 1.0, 1.06)),
                },
            ],
            first_person: Transform::IDENTITY,
        });
        definitions.push(ItemVisualDefinition {
            item: ids[3],
            display: DisplayMode::IconOnly,
            parts: vec![
                VisualPart {
                    attachment: AttachmentId::LeftFoot,
                    model: format!("{material}_boot_left"),
                    local: offset(Vec3::new(0.0, 0.0, 0.02), Vec3::new(1.1, 1.0, 1.1)),
                },
                VisualPart {
                    attachment: AttachmentId::RightFoot,
                    model: format!("{material}_boot_right"),
                    local: offset(Vec3::new(0.0, 0.0, 0.02), Vec3::new(1.1, 1.0, 1.1)),
                },
            ],
            first_person: Transform::IDENTITY,
        });
    }

    definitions
}

fn offset(translation: Vec3, scale: Vec3) -> Transform {
    Transform::new(translation, Quat::IDENTITY, scale)
}

fn tool_grip() -> Transform {
    Transform::new(
        Vec3::new(0.0, -0.02, 0.0),
        Quat::from_rotation_x(0.55),
        Vec3::ONE,
    )
}

fn sword_grip() -> Transform {
    Transform::new(
        Vec3::new(0.0, -0.02, 0.02),
        Quat::from_rotation_x(0.42),
        Vec3::ONE,
    )
}

fn food_grip() -> Transform {
    Transform::new(
        Vec3::new(0.0, -0.03, 0.03),
        Quat::from_rotation_x(0.2),
        Vec3::ONE,
    )
}

fn first_person_tool() -> Transform {
    Transform::new(
        Vec3::new(0.0, 0.0, -0.27),
        Quat::from_rotation_x(-0.35),
        Vec3::ONE,
    )
}

fn first_person_sword() -> Transform {
    Transform::new(
        Vec3::new(0.0, 0.0, -0.27),
        Quat::from_rotation_x(-0.25),
        Vec3::ONE,
    )
}

fn first_person_food() -> Transform {
    Transform::new(
        Vec3::new(0.0, 0.02, -0.29),
        Quat::from_rotation_x(-0.1),
        Vec3::ONE,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_and_sword_is_handheld_with_a_right_hand_part() {
        for (tier_index, (tier, _)) in TIERS.iter().enumerate() {
            for (kind_index, kind) in TOOL_KINDS.iter().enumerate() {
                let item = TOOL_IDS[tier_index][kind_index];
                let entry = definition(item).expect("工具视觉定义");
                assert_eq!(entry.display, DisplayMode::Handheld);
                assert_eq!(entry.parts.len(), 1);
                assert_eq!(entry.parts[0].attachment, AttachmentId::RightHand);
                assert_eq!(entry.parts[0].model, format!("{tier}_{kind}"));
            }
        }
        for (index, item) in SWORD_IDS.into_iter().enumerate() {
            let entry = definition(item).expect("剑视觉定义");
            assert_eq!(entry.display, DisplayMode::Handheld);
            assert_eq!(entry.parts[0].attachment, AttachmentId::RightHand);
            assert_eq!(
                entry.parts[0].model,
                format!("{}_sword", SWORD_TIERS[index])
            );
        }
        for (item, _) in HANDHELD_FOODS {
            assert!(is_handheld(item));
        }
    }

    #[test]
    fn armor_parts_follow_the_expected_bones() {
        for (index, (material, _)) in ARMOR_MATERIALS.iter().enumerate() {
            let ids = ARMOR_IDS[index];
            let helmet = definition(ids[0]).expect("头盔");
            assert_eq!(helmet.parts[0].attachment, AttachmentId::Head);
            assert_eq!(helmet.parts[0].model, format!("{material}_helmet"));

            let chest = definition(ids[1]).expect("胸甲");
            assert_eq!(chest.parts[0].attachment, AttachmentId::Chest);

            let leggings = definition(ids[2]).expect("护腿");
            let attachments: Vec<_> = leggings.parts.iter().map(|part| part.attachment).collect();
            assert_eq!(
                attachments,
                vec![
                    AttachmentId::Legs,
                    AttachmentId::LeftLeg,
                    AttachmentId::RightLeg
                ]
            );

            let boots = definition(ids[3]).expect("靴子");
            let attachments: Vec<_> = boots.parts.iter().map(|part| part.attachment).collect();
            assert_eq!(
                attachments,
                vec![AttachmentId::LeftFoot, AttachmentId::RightFoot]
            );
        }
    }

    #[test]
    fn unknown_items_are_hidden_and_armor_is_icon_only() {
        assert_eq!(display_mode(ItemId::STONE), DisplayMode::Hidden);
        assert_eq!(display_mode(ItemId::IRON_HELMET), DisplayMode::IconOnly);
        assert!(!is_handheld(ItemId::IRON_HELMET));
    }

    #[test]
    fn definitions_are_unique_per_item() {
        let all = definitions();
        for (index, entry) in all.iter().enumerate() {
            assert!(
                all[..index].iter().all(|other| other.item != entry.item),
                "物品 {:?} 重复定义",
                entry.item
            );
        }
    }
}
