//! 方块类型与各朝向面的基础颜色。

use glam::Vec3;

use super::block_registry::{self, BlockProperties};

/// 方块面的朝向。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    PosX,
    NegX,
    PosY,
    NegY,
    PosZ,
    NegZ,
}

impl Face {
    /// 六个面，供网格提取时固定顺序遍历。
    pub const ALL: [Face; 6] = [
        Face::PosX,
        Face::NegX,
        Face::PosY,
        Face::NegY,
        Face::PosZ,
        Face::NegZ,
    ];

    /// 面朝向的单位法向量。
    pub fn normal(self) -> Vec3 {
        match self {
            Face::PosX => Vec3::X,
            Face::NegX => Vec3::NEG_X,
            Face::PosY => Vec3::Y,
            Face::NegY => Vec3::NEG_Y,
            Face::PosZ => Vec3::Z,
            Face::NegZ => Vec3::NEG_Z,
        }
    }

    /// 该面朝向的模拟光照系数，顶面最亮、底面最暗。
    pub fn shade(self) -> f32 {
        match self {
            Face::PosY => 1.0,
            Face::NegY => 0.5,
            Face::PosX | Face::NegX => 0.8,
            Face::PosZ | Face::NegZ => 0.68,
        }
    }
}

/// 方块类型；`Air` 表示空位。
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum Block {
    Air = 0,
    Stone,
    Dirt,
    Grass,
    Wood,
    Campfire,
    Bed,
    Sand,
    Sandstone,
    #[serde(rename = "coal_ore")]
    CoalOre,
    #[serde(rename = "iron_ore")]
    IronOre,
    #[serde(rename = "gold_ore")]
    GoldOre,
    #[serde(rename = "diamond_ore")]
    DiamondOre,
    Water,
    Leaves,
    #[serde(rename = "tall_grass")]
    TallGrass,
    #[serde(rename = "flower_red")]
    FlowerRed,
    #[serde(rename = "flower_yellow")]
    FlowerYellow,
    Cactus,
}

impl Block {
    /// 全部方块，顺序与稳定 ID 一致；遍历与注册表校验共用。
    pub const ALL: [Block; 19] = [
        Block::Air,
        Block::Stone,
        Block::Dirt,
        Block::Grass,
        Block::Wood,
        Block::Campfire,
        Block::Bed,
        Block::Sand,
        Block::Sandstone,
        Block::CoalOre,
        Block::IronOre,
        Block::GoldOre,
        Block::DiamondOre,
        Block::Water,
        Block::Leaves,
        Block::TallGrass,
        Block::FlowerRed,
        Block::FlowerYellow,
        Block::Cactus,
    ];

    /// 稳定的方块 ID，与存档数据和属性注册表共用。
    pub fn id(self) -> u8 {
        self as u8
    }

    /// 渲染方式；网格提取按该字段选择实体面、水面或交叉面。
    pub fn render_type(self) -> super::block_registry::RenderType {
        self.properties().render_type
    }

    /// 统一的方块属性；所有模块都应通过该接口查询，而不是各自维护条件判断。
    pub fn properties(self) -> &'static BlockProperties {
        block_registry::properties(self)
    }

    /// 是否为具有实体碰撞的方块。
    pub fn is_solid(self) -> bool {
        self.properties().solid
    }

    /// 是否允许被破坏（挖掘）。
    pub fn is_breakable(self) -> bool {
        self.properties().breakable
    }

    /// 是否允许作为放置方块。
    pub fn is_placeable(self) -> bool {
        self.properties().placeable
    }

    /// 是否允许被准星射线选中。
    pub fn is_selectable(self) -> bool {
        self.properties().selectable
    }

    /// 该方块某个面的基础颜色（未叠加朝向明暗）。
    pub fn face_color(self, face: Face) -> Vec3 {
        self.properties().face_color(face)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn air_is_not_solid() {
        assert!(!Block::Air.is_solid());
        assert!(Block::Stone.is_solid());
    }

    #[test]
    fn grass_top_and_bottom_use_different_colors() {
        let top = Block::Grass.face_color(Face::PosY);
        let bottom = Block::Grass.face_color(Face::NegY);
        assert_ne!(top, bottom);
    }

    #[test]
    fn top_face_is_the_brightest() {
        assert_eq!(Face::PosY.shade(), 1.0);
        assert!(Face::NegY.shade() < Face::PosZ.shade());
    }
}
