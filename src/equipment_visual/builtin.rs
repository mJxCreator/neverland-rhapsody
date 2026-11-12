//! 内置工具、武器、食物与护甲模型。
//!
//! 这些模型是资源缺失时的回退外观，注册名与
//! [`super::visual_registry`] 中的模型名一一对应；同名 GLB 存在时磁盘资源优先。

use glam::Vec3;

use super::visual_registry::{ARMOR_MATERIALS, TIERS, TOOL_IDS, TOOL_KINDS};
use crate::model::material::{AlphaMode, ModelMaterial};
use crate::model::mesh::{MeshBounds, ModelMesh, ModelPrimitive};
use crate::model::types::ModelAsset;
use crate::model::ModelRegistry;

/// 木柄颜色。
const HANDLE_COLOR: [f32; 4] = [0.40, 0.28, 0.16, 1.0];
/// 剑格颜色。
const GUARD_COLOR: [f32; 4] = [0.35, 0.35, 0.38, 1.0];
/// 钻石剑刃颜色。
const DIAMOND_COLOR: [f32; 4] = [0.34, 0.88, 0.84, 1.0];
/// 苹果颜色。
const APPLE_COLOR: [f32; 4] = [0.78, 0.15, 0.16, 1.0];
/// 面包颜色。
const BREAD_COLOR: [f32; 4] = [0.75, 0.55, 0.28, 1.0];
/// 生肉颜色。
const RAW_MEAT_COLOR: [f32; 4] = [0.82, 0.45, 0.45, 1.0];
/// 熟肉颜色。
const COOKED_MEAT_COLOR: [f32; 4] = [0.62, 0.36, 0.20, 1.0];

struct ColoredBox {
    center: Vec3,
    half: Vec3,
    color: [f32; 4],
}

/// 注册全部内置装备模型；已由磁盘 GLB 注册的同名模型不会被覆盖。
pub fn register(models: &mut ModelRegistry) {
    for (tier_index, (tier, color)) in TIERS.iter().enumerate() {
        for (kind_index, kind) in TOOL_KINDS.iter().enumerate() {
            let name = format!("{tier}_{kind}");
            let item = TOOL_IDS[tier_index][kind_index];
            debug_assert_eq!(
                super::visual_registry::definition(item)
                    .and_then(|entry| entry.parts.first())
                    .map(|part| part.model.as_str()),
                Some(name.as_str())
            );
            models.register_builtin(&name, tool(&name, kind, *color));
        }
    }

    let sword_tiers: [(&str, [f32; 4]); 4] =
        [TIERS[0], TIERS[1], TIERS[2], ("diamond", DIAMOND_COLOR)];
    for (tier, color) in sword_tiers {
        let name = format!("{tier}_sword");
        models.register_builtin(&name, sword(&name, color));
    }

    models.register_builtin("apple", food("apple", APPLE_COLOR, 0.055));
    models.register_builtin("bread", food("bread", BREAD_COLOR, 0.0));
    models.register_builtin("raw_porkchop", food("raw_porkchop", RAW_MEAT_COLOR, 0.0));
    models.register_builtin("raw_beef", food("raw_beef", RAW_MEAT_COLOR, 0.0));
    models.register_builtin(
        "cooked_porkchop",
        food("cooked_porkchop", COOKED_MEAT_COLOR, 0.0),
    );
    models.register_builtin("cooked_beef", food("cooked_beef", COOKED_MEAT_COLOR, 0.0));

    for (material, color) in ARMOR_MATERIALS {
        models.register_builtin(&format!("{material}_helmet"), armor_helmet(material, color));
        models.register_builtin(
            &format!("{material}_chestplate"),
            armor_chestplate(material, color),
        );
        models.register_builtin(
            &format!("{material}_leggings_hip"),
            armor_leggings_hip(material, color),
        );
        models.register_builtin(
            &format!("{material}_leggings_left"),
            armor_legging(material, color, "left"),
        );
        models.register_builtin(
            &format!("{material}_leggings_right"),
            armor_legging(material, color, "right"),
        );
        models.register_builtin(
            &format!("{material}_boot_left"),
            armor_boot(material, color, "left"),
        );
        models.register_builtin(
            &format!("{material}_boot_right"),
            armor_boot(material, color, "right"),
        );
    }
}

fn tool(name: &str, kind: &str, head_color: [f32; 4]) -> ModelAsset {
    let mut boxes = vec![ColoredBox {
        center: Vec3::new(0.0, -0.05, 0.0),
        half: Vec3::new(0.028, 0.30, 0.028),
        color: HANDLE_COLOR,
    }];
    let head = match kind {
        "axe" => ColoredBox {
            center: Vec3::new(0.10, 0.25, 0.0),
            half: Vec3::new(0.11, 0.09, 0.035),
            color: head_color,
        },
        "shovel" => ColoredBox {
            center: Vec3::new(0.0, 0.30, 0.0),
            half: Vec3::new(0.075, 0.11, 0.03),
            color: head_color,
        },
        _ => ColoredBox {
            center: Vec3::new(0.0, 0.26, 0.0),
            half: Vec3::new(0.20, 0.045, 0.05),
            color: head_color,
        },
    };
    boxes.push(head);
    build(name, &boxes)
}

fn sword(name: &str, blade_color: [f32; 4]) -> ModelAsset {
    build(
        name,
        &[
            ColoredBox {
                center: Vec3::new(0.0, 0.02, 0.0),
                half: Vec3::new(0.025, 0.11, 0.025),
                color: HANDLE_COLOR,
            },
            ColoredBox {
                center: Vec3::new(0.0, 0.15, 0.0),
                half: Vec3::new(0.10, 0.02, 0.03),
                color: GUARD_COLOR,
            },
            ColoredBox {
                center: Vec3::new(0.0, 0.40, 0.0),
                half: Vec3::new(0.035, 0.24, 0.02),
                color: blade_color,
            },
        ],
    )
}

fn food(name: &str, color: [f32; 4], apple_radius: f32) -> ModelAsset {
    if apple_radius > 0.0 {
        return build(
            name,
            &[
                ColoredBox {
                    center: Vec3::ZERO,
                    half: Vec3::splat(apple_radius),
                    color,
                },
                ColoredBox {
                    center: Vec3::new(0.0, apple_radius + 0.02, 0.0),
                    half: Vec3::new(0.012, 0.025, 0.012),
                    color: HANDLE_COLOR,
                },
            ],
        );
    }
    let half = if name.starts_with("bread") {
        Vec3::new(0.10, 0.045, 0.055)
    } else {
        Vec3::new(0.09, 0.04, 0.06)
    };
    build(
        name,
        &[ColoredBox {
            center: Vec3::ZERO,
            half,
            color,
        }],
    )
}

fn armor_helmet(material: &str, color: [f32; 4]) -> ModelAsset {
    build(
        &format!("{material}_helmet"),
        &[
            ColoredBox {
                center: Vec3::ZERO,
                half: Vec3::new(0.148, 0.105, 0.148),
                color,
            },
            ColoredBox {
                center: Vec3::new(0.0, -0.02, 0.145),
                half: Vec3::new(0.13, 0.045, 0.015),
                color: shade(color, 0.7),
            },
        ],
    )
}

fn armor_chestplate(material: &str, color: [f32; 4]) -> ModelAsset {
    build(
        &format!("{material}_chestplate"),
        &[
            ColoredBox {
                center: Vec3::ZERO,
                half: Vec3::new(0.212, 0.165, 0.128),
                color,
            },
            ColoredBox {
                center: Vec3::new(0.0, -0.145, 0.0),
                half: Vec3::new(0.20, 0.022, 0.132),
                color: shade(color, 0.7),
            },
        ],
    )
}

fn armor_leggings_hip(material: &str, color: [f32; 4]) -> ModelAsset {
    build(
        &format!("{material}_leggings_hip"),
        &[ColoredBox {
            center: Vec3::ZERO,
            half: Vec3::new(0.172, 0.075, 0.108),
            color,
        }],
    )
}

fn armor_legging(material: &str, color: [f32; 4], side: &str) -> ModelAsset {
    build(
        &format!("{material}_leggings_{side}"),
        &[ColoredBox {
            center: Vec3::ZERO,
            half: Vec3::new(0.075, 0.175, 0.085),
            color,
        }],
    )
}

fn armor_boot(material: &str, color: [f32; 4], side: &str) -> ModelAsset {
    build(
        &format!("{material}_boot_{side}"),
        &[
            ColoredBox {
                center: Vec3::ZERO,
                half: Vec3::new(0.078, 0.05, 0.095),
                color,
            },
            ColoredBox {
                center: Vec3::new(0.0, -0.035, 0.035),
                half: Vec3::new(0.08, 0.018, 0.125),
                color: shade(color, 0.65),
            },
        ],
    )
}

fn shade(color: [f32; 4], factor: f32) -> [f32; 4] {
    [
        color[0] * factor,
        color[1] * factor,
        color[2] * factor,
        color[3],
    ]
}

fn build(name: &str, boxes: &[ColoredBox]) -> ModelAsset {
    let mut primitives = Vec::with_capacity(boxes.len());
    let mut materials = Vec::with_capacity(boxes.len());
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for colored in boxes {
        let material_index = materials.len();
        materials.push(ModelMaterial {
            name: format!("{name}_{material_index}"),
            base_color: colored.color,
            base_color_texture: None,
            metallic: 0.0,
            roughness: 0.85,
            alpha_mode: AlphaMode::Opaque,
            double_sided: false,
            placeholder: false,
        });
        let (primitive, bounds) = box_primitive(colored.center, colored.half, material_index);
        min = min.min(bounds.min);
        max = max.max(bounds.max);
        primitives.push(primitive);
    }
    ModelAsset {
        name: name.to_string(),
        meshes: vec![ModelMesh {
            name: name.to_string(),
            primitives,
            bounds: MeshBounds::new(min, max),
        }],
        materials,
        textures: Vec::new(),
        skeleton: None,
        animations: Vec::new(),
    }
}

fn box_primitive(center: Vec3, half: Vec3, material: usize) -> (ModelPrimitive, MeshBounds) {
    let min = center - half;
    let max = center + half;
    let faces = [
        (
            [
                [max.x, min.y, max.z],
                [max.x, min.y, min.z],
                [max.x, max.y, min.z],
                [max.x, max.y, max.z],
            ],
            [1.0, 0.0, 0.0],
        ),
        (
            [
                [min.x, min.y, min.z],
                [min.x, min.y, max.z],
                [min.x, max.y, max.z],
                [min.x, max.y, min.z],
            ],
            [-1.0, 0.0, 0.0],
        ),
        (
            [
                [min.x, max.y, max.z],
                [max.x, max.y, max.z],
                [max.x, max.y, min.z],
                [min.x, max.y, min.z],
            ],
            [0.0, 1.0, 0.0],
        ),
        (
            [
                [min.x, min.y, min.z],
                [max.x, min.y, min.z],
                [max.x, min.y, max.z],
                [min.x, min.y, max.z],
            ],
            [0.0, -1.0, 0.0],
        ),
        (
            [
                [min.x, min.y, max.z],
                [max.x, min.y, max.z],
                [max.x, max.y, max.z],
                [min.x, max.y, max.z],
            ],
            [0.0, 0.0, 1.0],
        ),
        (
            [
                [max.x, min.y, min.z],
                [min.x, min.y, min.z],
                [min.x, max.y, min.z],
                [max.x, max.y, min.z],
            ],
            [0.0, 0.0, -1.0],
        ),
    ];
    let mut primitive = ModelPrimitive {
        positions: Vec::new(),
        normals: Vec::new(),
        uvs: Vec::new(),
        tangents: None,
        joints: None,
        weights: None,
        indices: Vec::new(),
        material,
    };
    for (corners, normal) in faces {
        let base = primitive.positions.len() as u32;
        for corner in corners {
            primitive.positions.push(corner);
            primitive.normals.push(normal);
            primitive.uvs.push([0.0, 0.0]);
        }
        primitive
            .indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (primitive, MeshBounds::new(min, max))
}

/// 供测试与调试使用：注册全部内置装备并返回注册表。
#[cfg(test)]
fn registered() -> ModelRegistry {
    let mut models = ModelRegistry::new();
    register(&mut models);
    models
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::equipment_visual::visual_registry;
    use crate::model::types::ModelId;

    fn assert_valid(asset: &ModelAsset) {
        assert!(!asset.meshes.is_empty());
        for mesh in &asset.meshes {
            assert!(!mesh.primitives.is_empty());
            for primitive in &mesh.primitives {
                assert!(primitive.validate().is_ok(), "{} 图元非法", asset.name);
                assert!(!primitive.is_skinned());
            }
        }
    }

    #[test]
    fn every_visual_model_is_registered() {
        let models = registered();
        for definition in visual_registry::definitions() {
            for part in &definition.parts {
                let id = models
                    .by_name(&part.model)
                    .unwrap_or_else(|| panic!("缺少模型 {}", part.model));
                let asset = models.get(id).expect("模型资源存在");
                assert_valid(&asset);
                assert_eq!(asset.name, part.model);
            }
        }
    }

    #[test]
    fn tool_models_have_distinct_material_colors() {
        let models = registered();
        let wooden = models
            .get(models.by_name("wooden_pickaxe").unwrap())
            .unwrap();
        let iron = models.get(models.by_name("iron_pickaxe").unwrap()).unwrap();
        let wooden_colors: Vec<[f32; 4]> = wooden.materials.iter().map(|m| m.base_color).collect();
        let iron_colors: Vec<[f32; 4]> = iron.materials.iter().map(|m| m.base_color).collect();
        assert!(wooden_colors.iter().zip(&iron_colors).any(|(a, b)| a != b));
    }

    #[test]
    fn every_armor_part_has_geometry_around_its_origin() {
        let models = registered();
        for (material, _) in ARMOR_MATERIALS {
            for piece in [
                "helmet",
                "chestplate",
                "leggings_hip",
                "leggings_left",
                "leggings_right",
                "boot_left",
                "boot_right",
            ] {
                let name = format!("{material}_{piece}");
                let asset = models.get(models.by_name(&name).unwrap()).unwrap();
                let bounds = asset.bounds().expect("护甲包围盒");
                assert!(bounds.size().length() > 0.05, "{name} 几何过小");
                assert!(
                    bounds.min.y < 0.0 && bounds.max.y > 0.0,
                    "{name} 未围绕骨骼原点"
                );
            }
        }
    }

    #[test]
    fn model_ids_are_stable_across_registration() {
        let first = registered();
        let second = registered();
        let first_ids: Vec<ModelId> = first.assets().map(|(id, _)| id).collect();
        let second_ids: Vec<ModelId> = second.assets().map(|(id, _)| id).collect();
        assert_eq!(first_ids, second_ids);
    }
}
