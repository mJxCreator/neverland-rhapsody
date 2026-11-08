//! 基础材质系统：把采样方式、透明与深度/混合规则集中定义。
//!
//! 材质不保存任何图片数据，只描述“如何绘制”。方块纹理默认使用不透明
//! 材质，物品图标使用支持 Alpha 的材质，避免把透明规则写死在 UI 代码里。

use super::types::{AtlasId, Transparency};

/// 稳定材质标识。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MaterialId(u32);

impl MaterialId {
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// 纹理采样方式；像素艺术默认使用最近邻，保持边缘清晰。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SamplingMode {
    #[default]
    Nearest,
}

/// 颜色混合模式。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlendMode {
    /// 直接覆盖，适合不透明方块。
    #[default]
    Opaque,
    /// 标准 Alpha 混合，适合带透明背景的图标。
    Alpha,
}

/// 一份材质定义。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    pub id: MaterialId,
    pub atlas: AtlasId,
    pub sampling: SamplingMode,
    pub transparency: Transparency,
    pub depth_write: bool,
    pub blend: BlendMode,
}

impl Material {
    /// 普通方块默认材质：最近邻、不透明、写深度。
    pub const fn block_opaque(id: MaterialId) -> Self {
        Self {
            id,
            atlas: AtlasId::Block,
            sampling: SamplingMode::Nearest,
            transparency: Transparency::Opaque,
            depth_write: true,
            blend: BlendMode::Opaque,
        }
    }

    /// 物品图标材质：最近邻、保留 Alpha、不写深度并采用混合。
    pub const fn item_alpha(id: MaterialId) -> Self {
        Self {
            id,
            atlas: AtlasId::Item,
            sampling: SamplingMode::Nearest,
            transparency: Transparency::Alpha,
            depth_write: false,
            blend: BlendMode::Alpha,
        }
    }

    /// 未绘制生物纹理预留的材质。
    pub const fn entity_reserved(id: MaterialId) -> Self {
        Self {
            id,
            atlas: AtlasId::Entity,
            sampling: SamplingMode::Nearest,
            transparency: Transparency::Alpha,
            depth_write: false,
            blend: BlendMode::Alpha,
        }
    }
}

/// 材质注册表；内置材质拥有固定编号。
#[derive(Clone, Debug)]
pub struct MaterialRegistry {
    materials: Vec<Material>,
}

impl MaterialRegistry {
    /// 普通方块材质。
    pub const BLOCK_OPAQUE: MaterialId = MaterialId::new(0);
    /// 物品图标材质。
    pub const ITEM_ALPHA: MaterialId = MaterialId::new(1);
    /// 生物材质（预留）。
    pub const ENTITY_RESERVED: MaterialId = MaterialId::new(2);

    /// 构建内置材质集合。
    pub fn new() -> Self {
        Self {
            materials: vec![
                Material::block_opaque(Self::BLOCK_OPAQUE),
                Material::item_alpha(Self::ITEM_ALPHA),
                Material::entity_reserved(Self::ENTITY_RESERVED),
            ],
        }
    }

    pub fn get(&self, id: MaterialId) -> Option<&Material> {
        self.materials.get(id.raw() as usize)
    }

    /// 根据图集与透明需求选择材质；透明资源不会误用不透明材质。
    pub fn for_atlas(&self, atlas: AtlasId, transparency: Transparency) -> &Material {
        let target = match (atlas, transparency) {
            (AtlasId::Block, Transparency::Opaque) => Self::BLOCK_OPAQUE,
            (AtlasId::Item, _) | (AtlasId::Ui, _) => Self::ITEM_ALPHA,
            (_, Transparency::Alpha) | (AtlasId::Entity, _) => Self::ENTITY_RESERVED,
        };
        // 内置材质表总是完整，索引失败时退回第一项作为安全默认值。
        self.get(target).unwrap_or(&self.materials[0])
    }

    pub fn len(&self) -> usize {
        self.materials.len()
    }

    pub fn is_empty(&self) -> bool {
        self.materials.is_empty()
    }
}

impl Default for MaterialRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_materials_have_expected_render_rules() {
        let registry = MaterialRegistry::new();
        assert_eq!(registry.len(), 3);

        let block = registry
            .get(MaterialRegistry::BLOCK_OPAQUE)
            .expect("方块材质");
        assert_eq!(block.sampling, SamplingMode::Nearest);
        assert_eq!(block.transparency, Transparency::Opaque);
        assert!(block.depth_write);
        assert_eq!(block.blend, BlendMode::Opaque);

        let item = registry
            .get(MaterialRegistry::ITEM_ALPHA)
            .expect("图标材质");
        assert_eq!(item.transparency, Transparency::Alpha);
        assert!(!item.depth_write);
        assert_eq!(item.blend, BlendMode::Alpha);
    }

    #[test]
    fn transparent_textures_never_use_opaque_block_material() {
        let registry = MaterialRegistry::new();
        let material = registry.for_atlas(AtlasId::Block, Transparency::Alpha);
        assert_eq!(material.transparency, Transparency::Alpha);
        assert!(!material.depth_write);
    }

    #[test]
    fn item_icons_use_alpha_material() {
        let registry = MaterialRegistry::new();
        let material = registry.for_atlas(AtlasId::Item, Transparency::Opaque);
        assert_eq!(material.transparency, Transparency::Alpha);
        assert_eq!(material.blend, BlendMode::Alpha);
    }
}
