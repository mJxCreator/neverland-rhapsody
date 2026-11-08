//! 纹理注册表：按类型与资源名登记纹理，返回稳定 `TextureId`。
//!
//! 重复登记同一资源不会产生多份映射：元数据一致时幂等返回原有 ID，
//! 元数据冲突时明确报错，避免同一资源被两个不同路径或规格覆盖。

use std::collections::HashMap;
use std::fmt;

use super::types::{TextureId, TextureInfo, TextureType, Transparency, UVRect};

/// 登记一张纹理所需的元数据。
#[derive(Clone, Debug, PartialEq)]
pub struct TextureSeed {
    pub name: String,
    pub texture_type: TextureType,
    pub resource_path: String,
    pub width: u32,
    pub height: u32,
    pub transparency: Transparency,
    pub placeholder: bool,
}

/// 注册失败的原因。
#[derive(Clone, Debug, PartialEq)]
pub enum TextureRegistryError {
    /// 同一类型下已存在同名但元数据不同的纹理。
    Duplicate {
        texture_type: TextureType,
        name: String,
    },
    /// 纹理 ID 越界或未登记。
    UnknownId(TextureId),
}

impl fmt::Display for TextureRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextureRegistryError::Duplicate { texture_type, name } => write!(
                formatter,
                "纹理重复注册且元数据冲突: {} / {name}",
                texture_type.label()
            ),
            TextureRegistryError::UnknownId(id) => {
                write!(formatter, "纹理 ID 未登记: {}", id.raw())
            }
        }
    }
}

impl std::error::Error for TextureRegistryError {}

/// 只用追加方式维护的纹理登记表。
#[derive(Debug, Default)]
pub struct TextureRegistry {
    entries: Vec<TextureInfo>,
    by_key: HashMap<(TextureType, String), TextureId>,
}

impl TextureRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记纹理；成功返回稳定 ID。
    pub fn register(&mut self, seed: TextureSeed) -> Result<TextureId, TextureRegistryError> {
        let key = (seed.texture_type, seed.name.clone());
        if let Some(&existing) = self.by_key.get(&key) {
            let info = self.entries.get(existing.raw() as usize);
            let identical = info.is_some_and(|info| {
                info.resource_path == seed.resource_path
                    && info.width == seed.width
                    && info.height == seed.height
                    && info.transparency == seed.transparency
            });
            return if identical {
                Ok(existing)
            } else {
                Err(TextureRegistryError::Duplicate {
                    texture_type: seed.texture_type,
                    name: seed.name,
                })
            };
        }

        let id = TextureId::new(self.entries.len() as u32);
        let info = TextureInfo {
            id,
            name: seed.name,
            texture_type: seed.texture_type,
            resource_path: seed.resource_path,
            atlas: seed.texture_type.atlas(),
            uv: UVRect::ZERO,
            width: seed.width,
            height: seed.height,
            transparency: seed.transparency,
            placeholder: seed.placeholder,
        };
        self.entries.push(info);
        self.by_key.insert(key, id);
        Ok(id)
    }

    /// 按 ID 查询纹理信息；无效 ID 返回 `None`。
    pub fn get(&self, id: TextureId) -> Option<&TextureInfo> {
        self.entries
            .get(id.raw() as usize)
            .filter(|info| info.id == id)
    }

    /// 按类型与名称查询纹理 ID。
    pub fn find(&self, texture_type: TextureType, name: &str) -> Option<TextureId> {
        self.by_key.get(&(texture_type, name.to_string())).copied()
    }

    /// 写入图集分配后的 UV 范围。
    pub fn apply_uv(&mut self, id: TextureId, uv: UVRect) -> Result<(), TextureRegistryError> {
        let entry = self
            .entries
            .get_mut(id.raw() as usize)
            .ok_or(TextureRegistryError::UnknownId(id))?;
        entry.uv = uv;
        Ok(())
    }

    /// 已登记的纹理数量。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 遍历全部纹理。
    pub fn iter(&self) -> impl Iterator<Item = &TextureInfo> {
        self.entries.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(name: &str, texture_type: TextureType) -> TextureSeed {
        TextureSeed {
            name: name.to_string(),
            texture_type,
            resource_path: format!("assets/textures/{}/{name}.png", texture_type.directory()),
            width: 16,
            height: 16,
            transparency: Transparency::Opaque,
            placeholder: false,
        }
    }

    #[test]
    fn ids_are_assigned_in_registration_order() {
        let mut registry = TextureRegistry::new();
        let stone = registry
            .register(seed("stone", TextureType::Block))
            .expect("登记");
        let dirt = registry
            .register(seed("dirt", TextureType::Block))
            .expect("登记");
        assert_eq!(stone.raw(), 0);
        assert_eq!(dirt.raw(), 1);
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn duplicate_registration_is_idempotent_for_identical_metadata() {
        let mut registry = TextureRegistry::new();
        let first = registry
            .register(seed("stone", TextureType::Block))
            .expect("首次登记");
        let second = registry
            .register(seed("stone", TextureType::Block))
            .expect("重复登记");
        assert_eq!(first, second);
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn conflicting_duplicate_is_rejected() {
        let mut registry = TextureRegistry::new();
        registry
            .register(seed("stone", TextureType::Block))
            .expect("首次登记");

        let mut conflict = seed("stone", TextureType::Block);
        conflict.width = 32;
        let error = registry.register(conflict).expect_err("冲突应报错");
        assert!(matches!(error, TextureRegistryError::Duplicate { .. }));
    }

    #[test]
    fn the_same_name_can_exist_in_different_texture_types() {
        let mut registry = TextureRegistry::new();
        let block = registry
            .register(seed("stone", TextureType::Block))
            .expect("方块纹理");
        let item = registry
            .register(seed("stone", TextureType::Item))
            .expect("物品纹理");
        assert_ne!(block, item);
        assert_eq!(registry.find(TextureType::Block, "stone"), Some(block));
        assert_eq!(registry.find(TextureType::Item, "stone"), Some(item));
        assert_eq!(registry.find(TextureType::Entity, "stone"), None);
    }

    #[test]
    fn uv_can_be_applied_after_atlas_packing() {
        let mut registry = TextureRegistry::new();
        let id = registry
            .register(seed("stone", TextureType::Block))
            .expect("登记");
        assert_eq!(registry.get(id).map(|info| info.uv), Some(UVRect::ZERO));

        let uv = UVRect {
            min: [0.0, 0.0],
            max: [0.25, 0.25],
        };
        registry.apply_uv(id, uv).expect("写入 UV");
        assert_eq!(registry.get(id).map(|info| info.uv), Some(uv));
        assert!(registry.apply_uv(TextureId::new(99), uv).is_err());
    }
}
