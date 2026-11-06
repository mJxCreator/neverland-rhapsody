//! 网格层：从区块方块数据提取可见表面。

use glam::Vec3;

use crate::world::{Block, Chunk, Face, CHUNK_SIZE};

/// 带面色的顶点：位置与颜色，直接作为 GPU 顶点数据。
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

/// 一个区块提取出的三角形网格，位置已转换到世界坐标。
#[derive(Debug, Default)]
pub struct ChunkMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl ChunkMesh {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

/// 只为与非实心方块相邻的面生成四边形，隐藏区块内部与跨区块贴合面。
///
/// `block_at_world` 提供世界坐标查询，使区块边界的邻居能查询到相邻区块，
/// 避免在区块接缝处生成多余的侧面。
pub fn build_chunk_mesh(
    chunk: &Chunk,
    block_at_world: impl Fn(i32, i32, i32) -> Block,
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
                if !block.is_solid() {
                    continue;
                }

                let min = origin + Vec3::new(x as f32, y as f32, z as f32);
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
                    push_face(&mut mesh, block, face, min);
                }
            }
        }
    }

    mesh
}

fn push_face(mesh: &mut ChunkMesh, block: Block, face: Face, min: Vec3) {
    let color = (block.face_color(face) * face.shade() * block_variation(min)).to_array();
    let base = mesh.vertices.len() as u32;
    for corner in face_corners(face, min) {
        mesh.vertices.push(Vertex {
            position: corner.to_array(),
            color,
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

    fn chunk_with(blocks: &[(i32, i32, i32, Block)]) -> Chunk {
        let mut chunk = Chunk::new(ChunkPos::new(0, 0, 0));
        for &(x, y, z, block) in blocks {
            chunk.set_block(x, y, z, block);
        }
        chunk
    }

    /// 只为单个区块建网格，区块外的邻居一律视为空气。
    fn mesh_of(chunk: &Chunk) -> ChunkMesh {
        build_chunk_mesh(chunk, |x, y, z| chunk.block_at(x, y, z))
    }

    #[test]
    fn empty_chunk_has_no_geometry() {
        let mesh = mesh_of(&Chunk::new(ChunkPos::new(0, 0, 0)));
        assert!(mesh.is_empty());
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

        let mesh = build_chunk_mesh(&left, |x, y, z| {
            if (0..CHUNK_SIZE).contains(&x) {
                left.block_at(x, y, z)
            } else {
                right.block_at(x - CHUNK_SIZE, y, z)
            }
        });

        // 单个方块有 6 个面，跨区块贴合面被隐藏后只剩 5 个。
        assert_eq!(mesh.vertices.len(), 20);
        assert_eq!(mesh.indices.len(), 30);
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
}
