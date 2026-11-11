//! 树木与地表植被。
//!
//! 树木以“树根所在列”为唯一归属：生成某区块时会遍历外扩若干列的树根，
//! 只写入落在本区块内的方块，因此跨区块树冠连续且不会重复。
//! 地表草花与仙人掌完全属于当前区块，不跨边界。

use crate::world::block::Block;
use crate::world::chunk::{Chunk, ChunkPos, CHUNK_SIZE};
use crate::world::terrain::WORLD_HEIGHT;

use super::biome::{BiomeId, ColumnSurface, TreeKind};
use super::config::{VegetationConfig, SPAWN_CLEAR_RADIUS, SPAWN_COLUMN};
use super::noise::hash01;

/// 树冠最大半径外扩列数。
const TREE_MARGIN: i32 = 3;

/// 植被生成器。
pub struct VegetationGenerator {
    seed: u32,
    config: VegetationConfig,
}

impl VegetationGenerator {
    pub fn new(seed: u32, config: VegetationConfig) -> Self {
        Self { seed, config }
    }

    /// 生成落在当前区块内的树木方块。
    pub fn generate_trees(
        &self,
        pos: ChunkPos,
        chunk: &mut Chunk,
        column: impl Fn(i32, i32) -> ColumnSurface,
    ) {
        let origin_x = pos.x * CHUNK_SIZE;
        let origin_z = pos.z * CHUNK_SIZE;
        for z in origin_z - TREE_MARGIN..origin_z + CHUNK_SIZE + TREE_MARGIN {
            for x in origin_x - TREE_MARGIN..origin_x + CHUNK_SIZE + TREE_MARGIN {
                let surface = column(x, z);
                if surface.water_level.is_some() {
                    continue;
                }
                let definition = surface.biome.definition();
                if definition.tree == TreeKind::None || definition.tree_density <= 0.0 {
                    continue;
                }
                if hash01(self.seed, x, z, 0x7EE) >= definition.tree_density {
                    continue;
                }
                if self.is_spawn_clearing(x, z) {
                    continue;
                }
                let plan = TreePlan::for_column(self.seed, x, z, surface, definition.tree);
                if plan.ground + plan.height() >= WORLD_HEIGHT - 1 {
                    continue;
                }
                self.emit_tree(pos, chunk, x, surface.height, z, plan);
            }
        }
    }

    /// 生成高草、花与仙人掌。
    pub fn generate_ground_cover(
        &self,
        pos: ChunkPos,
        chunk: &mut Chunk,
        column: impl Fn(i32, i32) -> ColumnSurface,
    ) {
        let origin_x = pos.x * CHUNK_SIZE;
        let origin_z = pos.z * CHUNK_SIZE;
        for z in origin_z..origin_z + CHUNK_SIZE {
            for x in origin_x..origin_x + CHUNK_SIZE {
                let surface = column(x, z);
                if surface.water_level.is_some() {
                    continue;
                }
                let (lx, _, lz) = pos.local_of(x, 0, z);
                let ground_y = surface.height - 1;
                let (_, ground_local_y, _) = pos.local_of(x, ground_y, z);
                let above_local_y = ground_local_y + 1;
                if !(0..CHUNK_SIZE).contains(&above_local_y) {
                    continue;
                }
                let ground = chunk.block_at(lx, ground_local_y, lz);
                if above_local_y >= CHUNK_SIZE
                    || chunk.block_at(lx, above_local_y, lz) != Block::Air
                {
                    continue;
                }

                if ground == Block::Grass {
                    self.place_grass_or_flower(chunk, x, z, lx, above_local_y, lz, surface.biome);
                } else if ground == Block::Sand && surface.biome == BiomeId::Desert {
                    self.place_cactus(chunk, x, z, lx, above_local_y, lz);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn place_grass_or_flower(
        &self,
        chunk: &mut Chunk,
        x: i32,
        z: i32,
        lx: i32,
        ly: i32,
        lz: i32,
        biome: BiomeId,
    ) {
        let density = biome.definition().vegetation_density;
        let grass_chance = density * self.config.grass_scale;
        let flower_chance = density * self.config.flower_scale;
        let roll = hash01(self.seed, x, z, 0x9A55);
        if roll < grass_chance {
            chunk.set_block(lx, ly, lz, Block::TallGrass);
        } else if roll < grass_chance + flower_chance {
            let flower = if hash01(self.seed, x, z, 0xF10E) < 0.5 {
                Block::FlowerRed
            } else {
                Block::FlowerYellow
            };
            chunk.set_block(lx, ly, lz, flower);
        }
    }

    fn place_cactus(&self, chunk: &mut Chunk, x: i32, z: i32, lx: i32, ly: i32, lz: i32) {
        if hash01(self.seed, x, z, 0xCAC7) >= self.config.cactus_chance {
            return;
        }
        let height = 1 + (hash01(self.seed, x, z, 0xCAC8) * 3.0) as i32;
        if ly + height > CHUNK_SIZE {
            return;
        }
        for offset in 0..height {
            if chunk.block_at(lx, ly + offset, lz) != Block::Air {
                return;
            }
        }
        for offset in 0..height {
            chunk.set_block(lx, ly + offset, lz, Block::Cactus);
        }
    }

    fn is_spawn_clearing(&self, x: i32, z: i32) -> bool {
        (x - SPAWN_COLUMN.0).abs() <= SPAWN_CLEAR_RADIUS
            && (z - SPAWN_COLUMN.1).abs() <= SPAWN_CLEAR_RADIUS
    }

    fn emit_tree(
        &self,
        pos: ChunkPos,
        chunk: &mut Chunk,
        x: i32,
        ground: i32,
        z: i32,
        plan: TreePlan,
    ) {
        let top = ground + plan.trunk_height - 1;
        for y in ground..=top {
            self.put_if_replaceable(chunk, pos, x, y, z, Block::Wood, true);
        }

        let canopy_bottom = top - plan.canopy_drop;
        for y in canopy_bottom..=top + 1 {
            let radius = if y == top + 1 {
                1
            } else if plan.tall && y == canopy_bottom {
                plan.canopy_radius - 1
            } else {
                plan.canopy_radius
            };
            for dz in -radius..=radius {
                for dx in -radius..=radius {
                    if dx * dx + dz * dz > radius * radius + 1 {
                        continue;
                    }
                    if dx == 0 && dz == 0 && y <= top {
                        continue;
                    }
                    self.put_if_replaceable(chunk, pos, x + dx, y, z + dz, Block::Leaves, false);
                }
            }
        }
    }

    /// 写入落在区块内且可被替换的位置；树干可替换植被，树叶只替换空气与树叶。
    #[allow(clippy::too_many_arguments)]
    fn put_if_replaceable(
        &self,
        chunk: &mut Chunk,
        pos: ChunkPos,
        x: i32,
        y: i32,
        z: i32,
        block: Block,
        allow_plants: bool,
    ) {
        if !(0..WORLD_HEIGHT).contains(&y) || ChunkPos::from_world(x, y, z) != pos {
            return;
        }
        let (lx, ly, lz) = pos.local_of(x, y, z);
        let existing = chunk.block_at(lx, ly, lz);
        let replaceable = existing == Block::Air
            || existing == Block::Leaves
            || (allow_plants
                && matches!(
                    existing,
                    Block::TallGrass | Block::FlowerRed | Block::FlowerYellow
                ));
        if replaceable {
            chunk.set_block(lx, ly, lz, block);
        }
    }
}

/// 单棵树的形状参数。
#[derive(Clone, Copy, Debug)]
struct TreePlan {
    ground: i32,
    trunk_height: i32,
    canopy_radius: i32,
    canopy_drop: i32,
    tall: bool,
}

impl TreePlan {
    fn for_column(seed: u32, x: i32, z: i32, surface: ColumnSurface, kind: TreeKind) -> Self {
        let roll = hash01(seed, x, z, 0x7EF);
        match kind {
            TreeKind::TallOak => Self {
                ground: surface.height,
                trunk_height: 6 + (roll * 2.0) as i32,
                canopy_radius: 2,
                canopy_drop: 3,
                tall: true,
            },
            TreeKind::Oak | TreeKind::None => Self {
                ground: surface.height,
                trunk_height: 4 + (roll * 2.0) as i32,
                canopy_radius: 2,
                canopy_drop: 2,
                tall: false,
            },
        }
    }

    fn height(&self) -> i32 {
        self.trunk_height + 1
    }
}
