//! 方块六面纹理映射：把稳定 `BlockId` 关联到顶/底与四个侧面的纹理。
//!
//! 映射只负责“哪个面用哪张纹理”，方块硬度、采集等级与掉落规则仍由
//! 方块属性注册表持有，避免纹理配置演变成第二套游戏数据源。

use std::fmt;

use serde::{Deserialize, Serialize};

use super::texture_registry::TextureRegistry;
use super::types::{TextureId, TextureType, UVRect};
use crate::world::{Block, Face};

/// `blocks.json` 根结构。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BlockManifest {
    #[serde(default)]
    pub blocks: Vec<BlockManifestEntry>,
}

/// 单个方块的纹理配置；未填写的面回退到 `default`。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BlockManifestEntry {
    pub id: u8,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub top: Option<String>,
    #[serde(default)]
    pub bottom: Option<String>,
    #[serde(default)]
    pub north: Option<String>,
    #[serde(default)]
    pub south: Option<String>,
    #[serde(default)]
    pub east: Option<String>,
    #[serde(default)]
    pub west: Option<String>,
    #[serde(default, rename = "default")]
    pub default_texture: Option<String>,
}

impl BlockManifestEntry {
    /// 按朝向取配置的纹理名。
    pub fn texture_for(&self, face: Face) -> Option<&str> {
        match face {
            Face::PosY => self.top.as_deref(),
            Face::NegY => self.bottom.as_deref(),
            Face::NegX => self.west.as_deref(),
            Face::PosX => self.east.as_deref(),
            Face::NegZ => self.north.as_deref(),
            Face::PosZ => self.south.as_deref(),
        }
    }

    /// 面级配置缺失时回退到默认纹理。
    fn texture_or_default(&self, face: Face) -> Option<&str> {
        self.texture_for(face).or(self.default_texture.as_deref())
    }
}

/// 内置方块纹理映射：在没有 `blocks.json` 时仍能给出稳定的资源引用。
pub fn builtin_manifest() -> BlockManifest {
    BlockManifest {
        blocks: vec![
            BlockManifestEntry {
                id: Block::Stone.id(),
                name: Some("stone".to_string()),
                default_texture: Some("stone".to_string()),
                ..BlockManifestEntry::default()
            },
            BlockManifestEntry {
                id: Block::Dirt.id(),
                name: Some("dirt".to_string()),
                default_texture: Some("dirt".to_string()),
                ..BlockManifestEntry::default()
            },
            BlockManifestEntry {
                id: Block::Grass.id(),
                name: Some("grass".to_string()),
                top: Some("grass_top".to_string()),
                bottom: Some("dirt".to_string()),
                north: Some("grass_side".to_string()),
                south: Some("grass_side".to_string()),
                east: Some("grass_side".to_string()),
                west: Some("grass_side".to_string()),
                default_texture: Some("grass_side".to_string()),
            },
            BlockManifestEntry {
                id: Block::Wood.id(),
                name: Some("wood".to_string()),
                top: Some("wood_top".to_string()),
                bottom: Some("wood_top".to_string()),
                north: Some("wood_side".to_string()),
                south: Some("wood_side".to_string()),
                east: Some("wood_side".to_string()),
                west: Some("wood_side".to_string()),
                default_texture: Some("wood_side".to_string()),
            },
        ],
    }
}

/// 方块纹理解析失败的原因，用于报告配置问题。
#[derive(Clone, Debug, PartialEq)]
pub enum BlockTextureError {
    /// 同一方块 ID 重复配置。
    DuplicateBlock { id: u8 },
}

impl fmt::Display for BlockTextureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlockTextureError::DuplicateBlock { id } => {
                write!(formatter, "方块纹理配置存在重复 ID: {id}")
            }
        }
    }
}

impl std::error::Error for BlockTextureError {}

/// 方块六面纹理映射；按方块 ID 下标直接查询，网格生成无需再做字符串查找。
#[derive(Clone, Debug, PartialEq)]
pub struct BlockTextureMap {
    ids: Vec<[TextureId; 6]>,
    faces: Vec<[UVRect; 6]>,
}

impl BlockTextureMap {
    /// 所有面都使用整张纹理的测试/占位映射。
    pub fn placeholder() -> Self {
        Self {
            ids: vec![[TextureId::INVALID; 6]; 5],
            faces: vec![[UVRect::FULL; 6]; 5],
        }
    }

    /// 按配置与纹理注册表构建映射。
    ///
    /// 配置缺失或引用不存在的纹理时使用 `fallback`，并写入可读警告，
    /// 不会因为一张图片缺失而阻止方块渲染。
    pub fn from_manifest(
        manifest: &BlockManifest,
        registry: &TextureRegistry,
        fallback: TextureId,
        warnings: &mut Vec<String>,
    ) -> Result<Self, BlockTextureError> {
        let max_id = manifest
            .blocks
            .iter()
            .map(|entry| entry.id)
            .max()
            .unwrap_or(0)
            .max(Block::Wood.id());
        let mut ids = vec![[fallback; 6]; max_id as usize + 1];
        let mut faces = vec![[UVRect::ZERO; 6]; max_id as usize + 1];

        let mut seen_ids: Vec<u8> = Vec::new();
        for entry in &manifest.blocks {
            if seen_ids.contains(&entry.id) {
                return Err(BlockTextureError::DuplicateBlock { id: entry.id });
            }
            seen_ids.push(entry.id);
            if entry.id == Block::Air.id() || entry.id > Block::Wood.id() {
                warnings.push(format!(
                    "方块纹理配置 ID {} 不在当前方块注册表中，已忽略",
                    entry.id
                ));
                continue;
            }
            for face in Face::ALL {
                let index = face_index(face);
                let texture = entry
                    .texture_or_default(face)
                    .and_then(|name| registry.find(TextureType::Block, name));
                match texture {
                    Some(id) => {
                        ids[entry.id as usize][index] = id;
                        faces[entry.id as usize][index] =
                            registry.get(id).map_or(UVRect::ZERO, |info| info.uv);
                    }
                    None => {
                        let missing = entry
                            .texture_or_default(face)
                            .unwrap_or("<未配置>")
                            .to_string();
                        warnings.push(format!(
                            "方块 {} 的面 {} 引用纹理 {missing} 不存在，使用占位纹理",
                            entry.id,
                            face_label(face)
                        ));
                        ids[entry.id as usize][index] = fallback;
                        faces[entry.id as usize][index] =
                            registry.get(fallback).map_or(UVRect::ZERO, |info| info.uv);
                    }
                }
            }
        }

        // 未出现在配置中的方块整体回退到占位纹理。
        let fallback_uv = registry.get(fallback).map_or(UVRect::ZERO, |info| info.uv);
        for block in [Block::Stone, Block::Dirt, Block::Grass, Block::Wood] {
            if !manifest.blocks.iter().any(|entry| entry.id == block.id()) {
                warnings.push(format!("方块 {} 缺少纹理配置，使用占位纹理", block.id()));
                ids[block.id() as usize] = [fallback; 6];
                faces[block.id() as usize] = [fallback_uv; 6];
            }
        }

        Ok(Self { ids, faces })
    }

    /// 指定方块某个面使用的纹理 ID。
    pub fn face_texture(&self, block: Block, face: Face) -> TextureId {
        self.ids
            .get(block.id() as usize)
            .map_or(TextureId::INVALID, |ids| ids[face_index(face)])
    }

    /// 指定方块某个面的 UV 范围；未配置时返回零范围。
    pub fn face_rect(&self, block: Block, face: Face) -> UVRect {
        self.faces
            .get(block.id() as usize)
            .map_or(UVRect::ZERO, |faces| faces[face_index(face)])
    }

    /// 指定方块某个面四个角点的 UV，顺序与网格顶点角点顺序一致。
    pub fn face_uv(&self, block: Block, face: Face) -> [[f32; 2]; 4] {
        uv_corners(face, self.face_rect(block, face))
    }
}

/// 面在 `Face::ALL` 中的固定下标。
fn face_index(face: Face) -> usize {
    match face {
        Face::PosX => 0,
        Face::NegX => 1,
        Face::PosY => 2,
        Face::NegY => 3,
        Face::PosZ => 4,
        Face::NegZ => 5,
    }
}

fn face_label(face: Face) -> &'static str {
    match face {
        Face::PosX => "east",
        Face::NegX => "west",
        Face::PosY => "top",
        Face::NegY => "bottom",
        Face::PosZ => "south",
        Face::NegZ => "north",
    }
}

/// 按面的顶点顺序给出 UV；保持世界方向一致，避免侧面旋转或镜像。
pub fn uv_corners(face: Face, rect: UVRect) -> [[f32; 2]; 4] {
    let (u0, v0) = (rect.min[0], rect.min[1]);
    let (u1, v1) = (rect.max[0], rect.max[1]);
    match face {
        Face::PosX => [[u0, v1], [u1, v1], [u1, v0], [u0, v0]],
        Face::NegX => [[u0, v1], [u1, v1], [u1, v0], [u0, v0]],
        Face::PosY => [[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
        Face::NegY => [[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
        Face::PosZ => [[u0, v1], [u1, v1], [u1, v0], [u0, v0]],
        Face::NegZ => [[u0, v1], [u1, v1], [u1, v0], [u0, v0]],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::texture::texture_registry::TextureSeed;
    use crate::texture::types::Transparency;

    fn registry_with(names: &[&str]) -> TextureRegistry {
        let mut registry = TextureRegistry::new();
        for (index, name) in names.iter().enumerate() {
            let id = registry
                .register(TextureSeed {
                    name: name.to_string(),
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
        registry
    }

    #[test]
    fn builtin_manifest_keeps_grass_faces_distinct() {
        let manifest = builtin_manifest();
        assert_eq!(
            manifest.blocks[2].top.as_deref(),
            Some("grass_top"),
            "草方块顶面应使用草地纹理"
        );
        assert_eq!(manifest.blocks[2].bottom.as_deref(), Some("dirt"));
        assert_eq!(manifest.blocks[3].top.as_deref(), Some("wood_top"));
        assert_eq!(manifest.blocks[3].east.as_deref(), Some("wood_side"));
    }

    #[test]
    fn faces_resolve_to_distinct_uvs() {
        let registry = registry_with(&[
            "stone",
            "dirt",
            "grass_top",
            "grass_side",
            "wood_top",
            "wood_side",
        ]);
        let mut warnings = Vec::new();
        let map = BlockTextureMap::from_manifest(
            &builtin_manifest(),
            &registry,
            TextureId::new(0),
            &mut warnings,
        )
        .expect("构建映射");

        let top = map.face_rect(Block::Grass, Face::PosY);
        let bottom = map.face_rect(Block::Grass, Face::NegY);
        let side = map.face_rect(Block::Grass, Face::PosX);
        assert_ne!(top, bottom);
        assert_ne!(top, side);
        assert_eq!(
            map.face_texture(Block::Grass, Face::PosY),
            registry
                .find(TextureType::Block, "grass_top")
                .expect("grass_top 已登记")
        );
        assert_eq!(
            map.face_texture(Block::Grass, Face::NegY),
            registry
                .find(TextureType::Block, "dirt")
                .expect("dirt 已登记")
        );
        assert!(warnings.is_empty(), "内置映射不应产生缺失警告");
    }

    #[test]
    fn missing_textures_fall_back_with_warnings() {
        let registry = registry_with(&["stone"]);
        let mut warnings = Vec::new();
        let map = BlockTextureMap::from_manifest(
            &builtin_manifest(),
            &registry,
            TextureId::new(0),
            &mut warnings,
        )
        .expect("缺失纹理不应中断构建");

        assert!(!warnings.is_empty());
        assert_eq!(
            map.face_rect(Block::Grass, Face::PosY),
            map.face_rect(Block::Stone, Face::PosY),
            "缺失纹理应回退到占位 UV"
        );
    }

    #[test]
    fn uv_corners_stay_inside_the_rect() {
        let rect = UVRect {
            min: [0.25, 0.5],
            max: [0.5, 0.75],
        };
        for face in Face::ALL {
            for [u, v] in uv_corners(face, rect) {
                assert!((0.25..=0.5).contains(&u));
                assert!((0.5..=0.75).contains(&v));
            }
        }
    }

    #[test]
    fn top_and_bottom_orientations_are_consistent() {
        let rect = UVRect::FULL;
        let top = uv_corners(Face::PosY, rect);
        let bottom = uv_corners(Face::NegY, rect);
        assert_eq!(top[0], [0.0, 0.0]);
        assert_eq!(bottom[0], [0.0, 0.0]);
        assert_eq!(top[2], [1.0, 1.0]);
    }

    #[test]
    fn placeholder_map_is_uniform_and_in_bounds() {
        let map = BlockTextureMap::placeholder();
        for face in Face::ALL {
            for [u, v] in map.face_uv(Block::Stone, face) {
                assert!((0.0..=1.0).contains(&u));
                assert!((0.0..=1.0).contains(&v));
            }
        }
    }
}
