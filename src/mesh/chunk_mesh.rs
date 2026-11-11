//! 网格层：从区块方块数据提取可见表面。

use glam::Vec3;

use crate::texture::BlockTextureMap;
use crate::world::block_registry::RenderType;
use crate::world::{Block, Chunk, Face, CHUNK_SIZE};

/// 带面色的顶点：位置与颜色，供粒子/掉落物等纯色几何使用。
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

/// 方块网格顶点：位置、明暗色与图集 UV。
///
/// `color` 不再承载方块基色，而是保留面朝向明暗与稳定变化，最终颜色由
/// 着色器乘以图集采样结果得到。
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BlockVertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub uv: [f32; 2],
}

/// 一个区块提取出的三角形网格，位置已转换到世界坐标。
///
/// 不透明/交叉面与水面分开存放：水面需要独立的透明管线与排序。
#[derive(Debug, Default)]
pub struct ChunkMesh {
    pub vertices: Vec<BlockVertex>,
    pub indices: Vec<u32>,
    pub water_vertices: Vec<BlockVertex>,
    pub water_indices: Vec<u32>,
}

impl ChunkMesh {
    /// 是否包含需要透明管线绘制的水面。
    pub fn has_water(&self) -> bool {
        !self.water_indices.is_empty()
    }
}

/// 只为与非实心方块相邻的面生成四边形，隐藏区块内部与跨区块贴合面。
///
/// `block_at_world` 提供世界坐标查询，使区块边界的邻居能查询到相邻区块，
/// 避免在区块接缝处生成多余的侧面。`textures` 决定每个六面各自的图集 UV。
pub fn build_chunk_mesh(
    chunk: &Chunk,
    block_at_world: impl Fn(i32, i32, i32) -> Block,
    textures: &BlockTextureMap,
) -> ChunkMesh {
    let pos = chunk.pos();
    let origin = pos.origin();
    let base_x = pos.x * CHUNK_SIZE;
    let base_y = pos.y * CHUNK_SIZE;
    let base_z = pos.z * CHUNK_SIZE;
    let mut mesh = ChunkMesh::default();

    for y in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let block = chunk.block_at(x, y, z);
                let render_type = block.render_type();
                if render_type == RenderType::None {
                    continue;
                }

                let min = origin + Vec3::new(x as f32, y as f32, z as f32);
                if render_type == RenderType::Cross {
                    push_cross(&mut mesh, block, min, textures);
                    continue;
                }
                if render_type == RenderType::Transparent {
                    for face in Face::ALL {
                        let normal = face.normal();
                        let neighbor = block_at_world(
                            base_x + x + normal.x as i32,
                            base_y + y + normal.y as i32,
                            base_z + z + normal.z as i32,
                        );
                        if neighbor == Block::Water || neighbor.is_solid() {
                            continue;
                        }
                        push_water_face(&mut mesh, block, face, min, textures);
                    }
                    continue;
                }
                if !block.is_solid() {
                    continue;
                }

                for face in Face::ALL {
                    let normal = face.normal();
                    let neighbor = block_at_world(
                        base_x + x + normal.x as i32,
                        base_y + y + normal.y as i32,
                        base_z + z + normal.z as i32,
                    );
                    if neighbor.is_solid() {
                        continue;
                    }
                    push_face(&mut mesh, block, face, min, textures);
                }
            }
        }
    }

    mesh
}

fn push_face(
    mesh: &mut ChunkMesh,
    block: Block,
    face: Face,
    min: Vec3,
    textures: &BlockTextureMap,
) {
    // 纹理提供基色，顶点色只保留朝向明暗与稳定变化。
    let brightness = face.shade() * block_variation(min);
    let color = [brightness; 3];
    let uvs = textures.face_uv(block, face);
    let base = mesh.vertices.len() as u32;
    for (index, corner) in face_corners(face, min).into_iter().enumerate() {
        mesh.vertices.push(BlockVertex {
            position: corner.to_array(),
            color,
            uv: uvs[index],
        });
    }
    mesh.indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn push_water_face(
    mesh: &mut ChunkMesh,
    block: Block,
    face: Face,
    min: Vec3,
    textures: &BlockTextureMap,
) {
    let brightness = face.shade() * block_variation(min);
    let color = [brightness; 3];
    let uvs = textures.face_uv(block, face);
    let base = mesh.water_vertices.len() as u32;
    for (index, corner) in face_corners(face, min).into_iter().enumerate() {
        mesh.water_vertices.push(BlockVertex {
            position: corner.to_array(),
            color,
            uv: uvs[index],
        });
    }
    mesh.water_indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// 交叉植被：两片对角平面，双面写入，避免背面剔除造成消失。
fn push_cross(mesh: &mut ChunkMesh, block: Block, min: Vec3, textures: &BlockTextureMap) {
    let brightness = 0.92 * block_variation(min);
    let color = [brightness; 3];
    let uvs = textures.face_uv(block, Face::PosY);
    let inset = 0.15;
    let low = inset;
    let high = 1.0 - inset;

    // 平面 A：左下 → 右上。
    let a = [
        min + Vec3::new(low, 0.0, low),
        min + Vec3::new(high, 0.0, high),
        min + Vec3::new(high, 1.0, high),
        min + Vec3::new(low, 1.0, low),
    ];
    push_quad(mesh, a, uvs, color);
    push_quad(mesh, [a[0], a[3], a[2], a[1]], uvs, color);

    // 平面 B：右下 → 左上。
    let b = [
        min + Vec3::new(low, 0.0, high),
        min + Vec3::new(high, 0.0, low),
        min + Vec3::new(high, 1.0, low),
        min + Vec3::new(low, 1.0, high),
    ];
    push_quad(mesh, b, uvs, color);
    push_quad(mesh, [b[0], b[3], b[2], b[1]], uvs, color);
}

fn push_quad(mesh: &mut ChunkMesh, corners: [Vec3; 4], uvs: [[f32; 2]; 4], color: [f32; 3]) {
    let base = mesh.vertices.len() as u32;
    for (index, corner) in corners.into_iter().enumerate() {
        mesh.vertices.push(BlockVertex {
            position: corner.to_array(),
            color,
            uv: uvs[index],
        });
    }
    mesh.indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// 按方块坐标生成稳定的小幅明暗差异，让相邻同类方块仍能分辨出格子。
fn block_variation(min: Vec3) -> f32 {
    let hash = (min.x as i32).wrapping_mul(73_856_093)
        ^ (min.y as i32).wrapping_mul(19_349_663)
        ^ (min.z as i32).wrapping_mul(83_492_791);
    let noise = (hash & 0xff) as f32 / 255.0;
    0.94 + 0.12 * noise
}

/// 面从外侧观察时的四个角点，顺序为逆时针，保证正面朝外。
fn face_corners(face: Face, min: Vec3) -> [Vec3; 4] {
    match face {
        Face::PosX => [
            min + Vec3::new(1.0, 0.0, 1.0),
            min + Vec3::new(1.0, 0.0, 0.0),
            min + Vec3::new(1.0, 1.0, 0.0),
            min + Vec3::new(1.0, 1.0, 1.0),
        ],
        Face::NegX => [
            min + Vec3::new(0.0, 0.0, 0.0),
            min + Vec3::new(0.0, 0.0, 1.0),
            min + Vec3::new(0.0, 1.0, 1.0),
            min + Vec3::new(0.0, 1.0, 0.0),
        ],
        Face::PosY => [
            min + Vec3::new(0.0, 1.0, 1.0),
            min + Vec3::new(1.0, 1.0, 1.0),
            min + Vec3::new(1.0, 1.0, 0.0),
            min + Vec3::new(0.0, 1.0, 0.0),
        ],
        Face::NegY => [
            min + Vec3::new(0.0, 0.0, 0.0),
            min + Vec3::new(1.0, 0.0, 0.0),
            min + Vec3::new(1.0, 0.0, 1.0),
            min + Vec3::new(0.0, 0.0, 1.0),
        ],
        Face::PosZ => [
            min + Vec3::new(0.0, 0.0, 1.0),
            min + Vec3::new(1.0, 0.0, 1.0),
            min + Vec3::new(1.0, 1.0, 1.0),
            min + Vec3::new(0.0, 1.0, 1.0),
        ],
        Face::NegZ => [
            min + Vec3::new(1.0, 0.0, 0.0),
            min + Vec3::new(0.0, 0.0, 0.0),
            min + Vec3::new(0.0, 1.0, 0.0),
            min + Vec3::new(1.0, 1.0, 0.0),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::chunk::ChunkPos;
    use crate::world::World;

    fn chunk_with(blocks: &[(i32, i32, i32, Block)]) -> Chunk {
        let mut chunk = Chunk::new(ChunkPos::new(0, 0, 0));
        for &(x, y, z, block) in blocks {
            chunk.set_block(x, y, z, block);
        }
        chunk
    }

    /// 只为单个区块建网格，区块外的邻居一律视为空气。
    fn mesh_of(chunk: &Chunk) -> ChunkMesh {
        build_chunk_mesh(
            chunk,
            |x, y, z| chunk.block_at(x, y, z),
            &BlockTextureMap::placeholder(),
        )
    }

    #[test]
    fn empty_chunk_has_no_geometry() {
        let mesh = mesh_of(&Chunk::new(ChunkPos::new(0, 0, 0)));
        assert!(mesh.vertices.is_empty() && mesh.indices.is_empty());
        assert!(!mesh.has_water());
    }

    #[test]
    fn isolated_block_exposes_six_faces() {
        let mesh = mesh_of(&chunk_with(&[(1, 1, 1, Block::Stone)]));
        assert_eq!(mesh.vertices.len(), 24);
        assert_eq!(mesh.indices.len(), 36);
    }

    #[test]
    fn shared_face_is_culled() {
        let mesh = mesh_of(&chunk_with(&[
            (1, 1, 1, Block::Stone),
            (2, 1, 1, Block::Stone),
        ]));
        // 两个相邻方块共 12 个面，其中 2 个贴合面被隐藏。
        assert_eq!(mesh.vertices.len(), 40);
        assert_eq!(mesh.indices.len(), 60);
    }

    #[test]
    fn neighboring_chunk_culls_the_seam_face() {
        let mut left = Chunk::new(ChunkPos::new(0, 0, 0));
        let mut right = Chunk::new(ChunkPos::new(1, 0, 0));
        left.set_block(15, 1, 1, Block::Stone);
        right.set_block(0, 1, 1, Block::Stone);

        let mesh = build_chunk_mesh(
            &left,
            |x, y, z| {
                if (0..CHUNK_SIZE).contains(&x) {
                    left.block_at(x, y, z)
                } else {
                    right.block_at(x - CHUNK_SIZE, y, z)
                }
            },
            &BlockTextureMap::placeholder(),
        );

        // 单个方块有 6 个面，跨区块贴合面被隐藏后只剩 5 个。
        assert_eq!(mesh.vertices.len(), 20);
        assert_eq!(mesh.indices.len(), 30);
    }

    #[test]
    fn chunk_boundary_faces_follow_neighbor_loading() {
        let mut world = World::default();
        let left = ChunkPos::new(0, 0, 0);
        let right = ChunkPos::new(1, 0, 0);

        let mut chunk = Chunk::new(left);
        chunk.set_block(15, 1, 1, Block::Stone);
        world.insert_chunk(chunk);

        let placeholder = BlockTextureMap::placeholder();
        let mesh_without_neighbor = build_chunk_mesh(
            world.chunk_at(left).unwrap(),
            |x, y, z| world.block_at(x, y, z),
            &placeholder,
        );
        // 邻居未加载时，区块边界外的格子视为空气，六个面都可见。
        assert_eq!(mesh_without_neighbor.vertices.len(), 24);

        let mut chunk = Chunk::new(right);
        chunk.set_block(0, 1, 1, Block::Stone);
        world.insert_chunk(chunk);

        let mesh_with_neighbor = build_chunk_mesh(
            world.chunk_at(left).unwrap(),
            |x, y, z| world.block_at(x, y, z),
            &placeholder,
        );
        // 邻居加载后，跨区块贴合的侧面被剔除。
        assert_eq!(mesh_with_neighbor.vertices.len(), 20);

        world.remove_chunk(right);

        let mesh_after_unload = build_chunk_mesh(
            world.chunk_at(left).unwrap(),
            |x, y, z| world.block_at(x, y, z),
            &placeholder,
        );
        // 邻居卸载后边界面需要重新出现。
        assert_eq!(mesh_after_unload.vertices.len(), 24);
    }

    #[test]
    fn filled_chunk_only_keeps_the_shell() {
        let mut chunk = Chunk::new(ChunkPos::new(0, 0, 0));
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    chunk.set_block(x, y, z, Block::Stone);
                }
            }
        }
        let mesh = mesh_of(&chunk);
        let faces = 6 * (CHUNK_SIZE * CHUNK_SIZE) as usize;
        assert_eq!(mesh.vertices.len(), faces * 4);
        assert_eq!(mesh.indices.len(), faces * 6);
    }

    #[test]
    fn water_is_extracted_into_the_transparent_buffer() {
        let mesh = mesh_of(&chunk_with(&[(1, 1, 1, Block::Water)]));
        assert!(mesh.indices.is_empty(), "水面不应进入不透明缓冲");
        assert_eq!(mesh.water_vertices.len(), 24);
        assert_eq!(mesh.water_indices.len(), 36);
        assert!(mesh.has_water());
    }

    #[test]
    fn adjacent_water_hides_shared_faces() {
        let mesh = mesh_of(&chunk_with(&[
            (1, 1, 1, Block::Water),
            (2, 1, 1, Block::Water),
        ]));
        assert_eq!(mesh.water_vertices.len(), 40);
        assert_eq!(mesh.water_indices.len(), 60);
    }

    #[test]
    fn cross_plants_emit_two_double_sided_planes() {
        let mesh = mesh_of(&chunk_with(&[(1, 1, 1, Block::TallGrass)]));
        // 两片平面 × 双面 × 4 顶点。
        assert_eq!(mesh.vertices.len(), 16);
        assert_eq!(mesh.indices.len(), 24);
    }

    #[test]
    fn non_rendered_and_non_solid_blocks_are_skipped() {
        let mesh = mesh_of(&chunk_with(&[
            (1, 1, 1, Block::Air),
            (2, 1, 1, Block::FlowerRed),
        ]));
        assert_eq!(mesh.vertices.len(), 16, "只有花生成交叉面");
    }

    #[test]
    fn block_faces_use_their_configured_uvs() {
        use crate::texture::block_texture::builtin_manifest;
        use crate::texture::texture_registry::{TextureRegistry, TextureSeed};
        use crate::texture::types::{TextureId, TextureType, Transparency, UVRect};

        let names = [
            "stone",
            "dirt",
            "grass_top",
            "grass_side",
            "wood_top",
            "wood_side",
        ];
        let mut registry = TextureRegistry::new();
        for (index, name) in names.iter().enumerate() {
            let id = registry
                .register(TextureSeed {
                    name: (*name).to_string(),
                    texture_type: TextureType::Block,
                    resource_path: format!("assets/textures/blocks/{name}.png"),
                    width: 16,
                    height: 16,
                    transparency: Transparency::Opaque,
                    placeholder: false,
                })
                .expect("登记纹理");
            let offset = index as f32 * 0.05;
            registry
                .apply_uv(
                    id,
                    UVRect {
                        min: [offset, 0.0],
                        max: [offset + 0.05, 0.25],
                    },
                )
                .expect("写入 UV");
        }

        let map = BlockTextureMap::from_manifest(
            &builtin_manifest(),
            &registry,
            TextureId::new(0),
            &mut Vec::new(),
        )
        .expect("构建纹理映射");
        let chunk = chunk_with(&[(1, 1, 1, Block::Grass)]);
        let mesh = build_chunk_mesh(&chunk, |x, y, z| chunk.block_at(x, y, z), &map);

        // 面顺序为 PosX、NegX、PosY、NegY、PosZ、NegZ，每个面 4 个顶点。
        let top_uv = mesh.vertices[2 * 4].uv;
        let bottom_uv = mesh.vertices[3 * 4].uv;
        let side_uv = mesh.vertices[0].uv;
        assert_ne!(top_uv, bottom_uv, "草方块顶/底应使用不同纹理");
        assert_ne!(top_uv, side_uv, "草方块侧面应与顶面不同");
        for vertex in &mesh.vertices {
            assert!(
                (0.0..=1.0).contains(&vertex.uv[0]) && (0.0..=1.0).contains(&vertex.uv[1]),
                "UV 必须位于图集范围内: {:?}",
                vertex.uv
            );
        }
    }
}
