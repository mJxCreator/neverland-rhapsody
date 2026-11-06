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

/// 只为与空气相邻的面生成四边形，隐藏区块内部面。
pub fn build_chunk_mesh(chunk: &Chunk) -> ChunkMesh {
    let origin = chunk.pos().origin();
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
                    let neighbor = chunk.block_at(
                        x + normal.x as i32,
                        y + normal.y as i32,
                        z + normal.z as i32,
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

    #[test]
    fn empty_chunk_has_no_geometry() {
        let mesh = build_chunk_mesh(&Chunk::new(ChunkPos::new(0, 0, 0)));
        assert!(mesh.is_empty());
    }

    #[test]
    fn isolated_block_exposes_six_faces() {
        let mesh = build_chunk_mesh(&chunk_with(&[(1, 1, 1, Block::Stone)]));
        assert_eq!(mesh.vertices.len(), 24);
        assert_eq!(mesh.indices.len(), 36);
    }

    #[test]
    fn shared_face_is_culled() {
        let mesh = build_chunk_mesh(&chunk_with(&[
            (1, 1, 1, Block::Stone),
            (2, 1, 1, Block::Stone),
        ]));
        // 两个相邻方块共 12 个面，其中 2 个贴合面被隐藏。
        assert_eq!(mesh.vertices.len(), 40);
        assert_eq!(mesh.indices.len(), 60);
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
        let mesh = build_chunk_mesh(&chunk);
        let faces = 6 * (CHUNK_SIZE * CHUNK_SIZE) as usize;
        assert_eq!(mesh.vertices.len(), faces * 4);
        assert_eq!(mesh.indices.len(), faces * 6);
    }
}
