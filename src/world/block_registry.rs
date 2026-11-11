//! 方块属性注册表：集中定义每种方块的身份、硬度、交互权限与渲染信息。
//!
//! 方块 ID 与 [`Block`] 的枚举判别值保持一致，新增方块只能追加到末尾，
//! 不得调整已有 ID，否则旧存档的方块数据会错位。

use glam::Vec3;

use super::block::{Block, Face};
use crate::item::tool::ToolType;

/// 方块渲染方式；当前只有不透明实体方块，为后续透明/交叉渲染预留。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderType {
    /// 不渲染，通常对应空气。
    None,
    /// 不透明实体方块。
    Opaque,
}

/// 挖掘类别；决定哪一类工具能对该方块提供效率加成。
///
/// 类别与工具等级相互独立：类别匹配只影响速度，能否获得掉落由采集等级判定。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MiningCategory {
    /// 不属于任何可工具加速的类别。
    None,
    /// 泥土、草方块等，铲子适用。
    Dirt,
    /// 石头类，镐适用。
    Stone,
    /// 木质类，斧适用。
    Wood,
}

/// 方块被破坏后的掉落规则。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropRule {
    /// 任何情况下都按掉落表生成。
    Always,
    /// 只有满足采集条件（工具类别与等级）才生成正常掉落，否则无掉落。
    RequiresCorrectTool,
    /// 永不掉落。
    Never,
}

/// 方块各朝向的基础颜色；当前用顶点色代替纹理。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockColors {
    pub top: Vec3,
    pub bottom: Vec3,
    pub side: Vec3,
}

impl BlockColors {
    const NONE: Self = Self {
        top: Vec3::ZERO,
        bottom: Vec3::ZERO,
        side: Vec3::ZERO,
    };

    const STONE: Self = Self {
        top: Vec3::new(0.55, 0.55, 0.58),
        bottom: Vec3::new(0.55, 0.55, 0.58),
        side: Vec3::new(0.55, 0.55, 0.58),
    };

    const DIRT: Self = Self {
        top: Vec3::new(0.45, 0.33, 0.22),
        bottom: Vec3::new(0.45, 0.33, 0.22),
        side: Vec3::new(0.45, 0.33, 0.22),
    };

    const GRASS: Self = Self {
        top: Vec3::new(0.35, 0.62, 0.32),
        bottom: Vec3::new(0.45, 0.33, 0.22),
        side: Vec3::new(0.42, 0.47, 0.24),
    };

    const WOOD: Self = Self {
        top: Vec3::new(0.58, 0.44, 0.26),
        bottom: Vec3::new(0.58, 0.44, 0.26),
        side: Vec3::new(0.48, 0.32, 0.18),
    };

    const CAMPFIRE: Self = Self {
        top: Vec3::new(0.95, 0.52, 0.16),
        bottom: Vec3::new(0.35, 0.24, 0.14),
        side: Vec3::new(0.46, 0.30, 0.16),
    };

    const BED: Self = Self {
        top: Vec3::new(0.78, 0.20, 0.22),
        bottom: Vec3::new(0.42, 0.28, 0.18),
        side: Vec3::new(0.66, 0.18, 0.20),
    };

    /// 某个朝向面的基础颜色。
    pub fn face_color(self, face: Face) -> Vec3 {
        match face {
            Face::PosY => self.top,
            Face::NegY => self.bottom,
            Face::PosX | Face::NegX | Face::PosZ | Face::NegZ => self.side,
        }
    }
}

/// 一种方块的全部静态属性。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockProperties {
    /// 稳定的方块 ID，与存档中的方块枚举判别值一致。
    pub id: u8,
    /// 展示用名称。
    pub name: &'static str,
    /// 硬度；0 表示不可挖掘。挖掘时间 = 硬度 × 基础系数。
    pub hardness: f32,
    /// 该方块所属的挖掘类别，决定哪类工具能获得效率加成。
    pub mining_category: MiningCategory,
    /// 推荐或要求的工具类别；`None` 表示徒手与任意工具都可正常采集。
    pub required_tool: Option<ToolType>,
    /// 采集等级要求；工具等级低于该值时无法获得正常掉落。
    pub harvest_level: u8,
    /// 破坏后的掉落规则。
    pub drop_rule: DropRule,
    /// 是否具有实体碰撞。
    pub solid: bool,
    /// 是否允许被破坏。
    pub breakable: bool,
    /// 是否允许作为放置方块。
    pub placeable: bool,
    /// 是否允许被准星射线选中。
    pub selectable: bool,
    /// 渲染方式。
    pub render_type: RenderType,
    /// 各朝向基础颜色，替代尚未接入的纹理。
    pub colors: BlockColors,
}

impl BlockProperties {
    /// 该方块某个朝向面的基础颜色。
    pub fn face_color(self, face: Face) -> Vec3 {
        self.colors.face_color(face)
    }
}

pub const AIR: BlockProperties = BlockProperties {
    id: 0,
    name: "Air",
    hardness: 0.0,
    mining_category: MiningCategory::None,
    required_tool: None,
    harvest_level: 0,
    drop_rule: DropRule::Never,
    solid: false,
    breakable: false,
    placeable: false,
    selectable: false,
    render_type: RenderType::None,
    colors: BlockColors::NONE,
};

pub const STONE: BlockProperties = BlockProperties {
    id: 1,
    name: "Stone",
    hardness: 1.5,
    mining_category: MiningCategory::Stone,
    // v0.14 起木镐即可获得石头掉落，保证“木镐采石→石镐”的合成闭环。
    required_tool: Some(ToolType::Pickaxe),
    harvest_level: 0,
    drop_rule: DropRule::RequiresCorrectTool,
    solid: true,
    breakable: true,
    placeable: true,
    selectable: true,
    render_type: RenderType::Opaque,
    colors: BlockColors::STONE,
};

pub const DIRT: BlockProperties = BlockProperties {
    id: 2,
    name: "Dirt",
    hardness: 0.5,
    mining_category: MiningCategory::Dirt,
    // 铲子只是推荐工具：通过挖掘类别带来速度加成，但徒手也能正常采集。
    required_tool: None,
    harvest_level: 0,
    drop_rule: DropRule::Always,
    solid: true,
    breakable: true,
    placeable: true,
    selectable: true,
    render_type: RenderType::Opaque,
    colors: BlockColors::DIRT,
};

pub const GRASS: BlockProperties = BlockProperties {
    id: 3,
    name: "Grass",
    hardness: 0.6,
    mining_category: MiningCategory::Dirt,
    // 铲子只是推荐工具：通过挖掘类别带来速度加成，但徒手也能正常采集。
    required_tool: None,
    harvest_level: 0,
    drop_rule: DropRule::Always,
    solid: true,
    breakable: true,
    placeable: true,
    selectable: true,
    render_type: RenderType::Opaque,
    colors: BlockColors::GRASS,
};

pub const WOOD: BlockProperties = BlockProperties {
    id: 4,
    name: "Wood",
    hardness: 2.0,
    mining_category: MiningCategory::Wood,
    // 斧只是推荐工具：徒手也能采集木材，保证新世界可以从零开始合成。
    required_tool: None,
    harvest_level: 0,
    drop_rule: DropRule::Always,
    solid: true,
    breakable: true,
    placeable: true,
    selectable: true,
    render_type: RenderType::Opaque,
    colors: BlockColors::WOOD,
};

pub const CAMPFIRE: BlockProperties = BlockProperties {
    id: 5,
    name: "Campfire",
    hardness: 0.4,
    mining_category: MiningCategory::Wood,
    required_tool: None,
    harvest_level: 0,
    drop_rule: DropRule::Always,
    solid: true,
    breakable: true,
    placeable: true,
    selectable: true,
    render_type: RenderType::Opaque,
    colors: BlockColors::CAMPFIRE,
};

pub const BED: BlockProperties = BlockProperties {
    id: 6,
    name: "Bed",
    hardness: 0.3,
    mining_category: MiningCategory::Wood,
    required_tool: None,
    harvest_level: 0,
    drop_rule: DropRule::Always,
    solid: true,
    breakable: true,
    placeable: true,
    selectable: true,
    render_type: RenderType::Opaque,
    colors: BlockColors::BED,
};

/// 全部方块属性，下标必须等于方块 ID。
const DEFINITIONS: [BlockProperties; 7] = [AIR, STONE, DIRT, GRASS, WOOD, CAMPFIRE, BED];

/// 按方块类型查询统一属性。
pub fn properties(block: Block) -> &'static BlockProperties {
    &DEFINITIONS[block.id() as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_ids_are_stable_and_unique() {
        let definitions = [AIR, STONE, DIRT, GRASS, WOOD, CAMPFIRE, BED];
        let mut ids = Vec::new();
        for (index, definition) in definitions.iter().enumerate() {
            assert_eq!(definition.id as usize, index, "属性表顺序与 ID 不一致");
            ids.push(definition.id);
        }
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), definitions.len(), "方块 ID 必须唯一");

        assert_eq!(Block::Air.id(), 0);
        assert_eq!(Block::Stone.id(), 1);
        assert_eq!(Block::Dirt.id(), 2);
        assert_eq!(Block::Grass.id(), 3);
        assert_eq!(Block::Wood.id(), 4);
        assert_eq!(Block::Campfire.id(), 5);
        assert_eq!(Block::Bed.id(), 6);
    }

    #[test]
    fn hardness_matches_the_design_table() {
        assert_eq!(properties(Block::Air).hardness, 0.0);
        assert_eq!(properties(Block::Grass).hardness, 0.6);
        assert_eq!(properties(Block::Dirt).hardness, 0.5);
        assert_eq!(properties(Block::Stone).hardness, 1.5);
        assert_eq!(properties(Block::Wood).hardness, 2.0);
    }

    #[test]
    fn mining_properties_separate_category_from_harvest_level() {
        let stone = properties(Block::Stone);
        assert_eq!(stone.mining_category, MiningCategory::Stone);
        assert_eq!(stone.required_tool, Some(ToolType::Pickaxe));
        assert_eq!(
            stone.harvest_level, 0,
            "木镐必须能够采集石头，形成木镐→石镐的合成闭环"
        );
        assert_eq!(stone.drop_rule, DropRule::RequiresCorrectTool);

        let dirt = properties(Block::Dirt);
        assert_eq!(dirt.mining_category, MiningCategory::Dirt);
        assert_eq!(
            dirt.required_tool, None,
            "铲子只是推荐工具，不能成为强制要求"
        );
        assert_eq!(dirt.harvest_level, 0);
        assert_eq!(dirt.drop_rule, DropRule::Always, "泥土徒手也应掉落");
    }

    #[test]
    fn air_never_drops_and_has_no_requirements() {
        let air = properties(Block::Air);
        assert_eq!(air.mining_category, MiningCategory::None);
        assert_eq!(air.required_tool, None);
        assert_eq!(air.drop_rule, DropRule::Never);
    }

    #[test]
    fn air_is_never_interactive() {
        let air = properties(Block::Air);
        assert!(!air.solid);
        assert!(!air.breakable);
        assert!(!air.placeable);
        assert!(!air.selectable);
        assert_eq!(air.render_type, RenderType::None);
        assert!(!Block::Air.is_solid());
        assert!(!Block::Air.is_breakable());
        assert!(!Block::Air.is_placeable());
        assert!(!Block::Air.is_selectable());
    }

    #[test]
    fn solid_blocks_are_breakable_placeable_and_selectable() {
        for block in [
            Block::Stone,
            Block::Dirt,
            Block::Grass,
            Block::Wood,
            Block::Campfire,
            Block::Bed,
        ] {
            let definition = properties(block);
            assert!(definition.solid);
            assert!(definition.breakable);
            assert!(definition.placeable);
            assert!(definition.selectable);
            assert_eq!(definition.render_type, RenderType::Opaque);
        }
    }

    #[test]
    fn grass_keeps_distinct_top_and_bottom_colors() {
        let definition = properties(Block::Grass);
        assert_ne!(
            definition.face_color(Face::PosY),
            definition.face_color(Face::NegY)
        );
        assert_eq!(
            Block::Grass.face_color(Face::PosY),
            definition.face_color(Face::PosY)
        );
    }

    #[test]
    fn block_serde_names_stay_backward_compatible() {
        assert_eq!(
            serde_json::to_string(&Block::Grass).expect("序列化失败"),
            "\"grass\""
        );
        assert_eq!(
            serde_json::from_str::<Block>("\"stone\"").expect("反序列化失败"),
            Block::Stone
        );
    }
}
