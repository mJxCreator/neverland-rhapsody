//! 网格层：从区块方块数据提取可见表面并输出顶点数据。

pub mod chunk_mesh;

pub use self::chunk_mesh::{build_chunk_mesh, ChunkMesh, Vertex};
