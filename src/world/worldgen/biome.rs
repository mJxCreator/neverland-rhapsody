//! 生物群系标识与定义结构。
//!
//! 四类群系覆盖森林、平原、沙漠与山地；生成器按温度/湿度权重混合高度，
//! 保证群系交界处高度连续，表层方块则取权重最大的群系。

use crate::world::block::Block;

/// 生物群系标识；稳定顺序用于权重数组索引。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BiomeId {
    Plains = 0,
    Forest = 1,
    Desert = 2,
    Mountains = 3,
}

impl BiomeId {
    /// 全部群系，顺序与权重数组一致。
    pub const ALL: [BiomeId; 4] = [
        BiomeId::Plains,
        BiomeId::Forest,
        BiomeId::Desert,
        BiomeId::Mountains,
    ];

    /// 权重数组下标。
    pub fn index(self) -> usize {
        self as usize
    }

    /// 展示名称。
    pub fn name(self) -> &'static str {
        self.definition().name
    }

    /// 该群系的完整定义。
    pub fn definition(self) -> &'static BiomeDefinition {
        super::biome_registry::definition(self)
    }
}

/// 单列地表信息，供树木与植被生成复用。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnSurface {
    /// 地表最高实心方块的世界高度。
    pub height: i32,
    /// 主导生物群系。
    pub biome: BiomeId,
    /// 水面高度；`None` 表示该列没有水体。
    pub water_level: Option<i32>,
}

/// 树木种类。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeKind {
    /// 不生成树木。
    None,
    /// 矮树：4–5 格树干、半径 2 的树冠。
    Oak,
    /// 高树：6–7 格树干、更厚的树冠。
    TallOak,
}

/// 生物群系定义：地表方块、高度范围、树木密度、植被密度与水概率。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BiomeDefinition {
    pub id: BiomeId,
    pub name: &'static str,
    /// 地表方块。
    pub surface: Block,
    /// 地表以下的过渡层方块。
    pub subsurface: Block,
    /// 更深处的石头层方块。
    pub stone: Block,
    /// 基准高度。
    pub base_height: f64,
    /// 高度噪声振幅。
    pub height_amplitude: f64,
    /// 每列生成树木的概率。
    pub tree_density: f64,
    /// 地表植被密度。
    pub vegetation_density: f64,
    /// 湖泊出现概率（0–1）。
    pub water_probability: f64,
    /// 树木种类。
    pub tree: TreeKind,
}
