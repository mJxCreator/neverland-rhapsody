//! 生物群系静态注册表。

use crate::world::block::Block;

use super::biome::{BiomeDefinition, BiomeId, TreeKind};

/// 平原：缓坡草地，稀疏矮树与较多花草。
pub const PLAINS: BiomeDefinition = BiomeDefinition {
    id: BiomeId::Plains,
    name: "Plains",
    surface: Block::Grass,
    subsurface: Block::Dirt,
    stone: Block::Stone,
    base_height: 7.0,
    height_amplitude: 3.0,
    tree_density: 0.025,
    vegetation_density: 0.34,
    water_probability: 0.45,
    tree: TreeKind::Oak,
};

/// 森林：起伏略大、高树密集。
pub const FOREST: BiomeDefinition = BiomeDefinition {
    id: BiomeId::Forest,
    name: "Forest",
    surface: Block::Grass,
    subsurface: Block::Dirt,
    stone: Block::Stone,
    base_height: 8.5,
    height_amplitude: 4.0,
    tree_density: 0.11,
    vegetation_density: 0.46,
    water_probability: 0.3,
    tree: TreeKind::TallOak,
};

/// 沙漠：沙面与砂岩层，几乎无树，偶尔生成仙人掌。
pub const DESERT: BiomeDefinition = BiomeDefinition {
    id: BiomeId::Desert,
    name: "Desert",
    surface: Block::Sand,
    subsurface: Block::Sandstone,
    stone: Block::Stone,
    base_height: 9.0,
    height_amplitude: 2.5,
    tree_density: 0.0,
    vegetation_density: 0.05,
    water_probability: 0.02,
    tree: TreeKind::None,
};

/// 山地：石质地表与更高的山脊。
pub const MOUNTAINS: BiomeDefinition = BiomeDefinition {
    id: BiomeId::Mountains,
    name: "Mountains",
    surface: Block::Stone,
    subsurface: Block::Stone,
    stone: Block::Stone,
    base_height: 13.0,
    height_amplitude: 5.0,
    tree_density: 0.008,
    vegetation_density: 0.04,
    water_probability: 0.0,
    tree: TreeKind::Oak,
};

const DEFINITIONS: [BiomeDefinition; 4] = [PLAINS, FOREST, DESERT, MOUNTAINS];

/// 按标识查询群系定义。
pub fn definition(id: BiomeId) -> &'static BiomeDefinition {
    &DEFINITIONS[id.index()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_biome_has_a_matching_definition() {
        for biome in BiomeId::ALL {
            let definition = definition(biome);
            assert_eq!(definition.id, biome);
            assert_eq!(biome.name(), definition.name);
            assert!(definition.vegetation_density > 0.0);
            assert!(definition.base_height > 0.0);
        }
    }

    #[test]
    fn biome_ids_follow_array_order() {
        for (index, biome) in BiomeId::ALL.iter().enumerate() {
            assert_eq!(biome.index(), index);
        }
    }
}
