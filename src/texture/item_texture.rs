//! 物品图标映射：`ItemId` 到 Item Atlas 中图标纹理的查询接口。
//!
//! 图标与工具耐久、堆叠数量完全分离：本模块只回答“图标长什么样”，
//! 不参与任何物品业务属性计算，供 v0.17 的背包与快捷栏 UI 直接复用。

use std::fmt;

use serde::{Deserialize, Serialize};

use super::texture_registry::TextureRegistry;
use super::types::{TextureId, TextureType, Transparency, UVRect};
use crate::item::ItemId;

/// 图标的显示模式。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IconDisplayMode {
    /// 以图标纹理绘制。
    #[default]
    Icon,
    /// 该物品没有图标。
    None,
}

/// 透明区域处理规则。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IconAlphaRule {
    /// 保留 PNG 中的透明通道。
    #[default]
    Preserve,
    /// 需要时强制按不透明绘制。
    ForceOpaque,
}

/// `items.json` 根结构。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ItemManifest {
    #[serde(default)]
    pub items: Vec<ItemManifestEntry>,
}

/// 单个物品的图标配置。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ItemManifestEntry {
    pub id: u16,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub display_mode: Option<IconDisplayMode>,
    #[serde(default)]
    pub alpha: Option<IconAlphaRule>,
}

/// 内置物品图标映射；名称与 `assets/textures/items` 下的 PNG 文件名对应。
pub fn builtin_manifest() -> ItemManifest {
    const ICONS: [(ItemId, &str); 18] = [
        (ItemId::STONE, "stone"),
        (ItemId::DIRT, "dirt"),
        (ItemId::GRASS, "grass"),
        (ItemId::WOODEN_PICKAXE, "wooden_pickaxe"),
        (ItemId::STONE_PICKAXE, "stone_pickaxe"),
        (ItemId::IRON_PICKAXE, "iron_pickaxe"),
        (ItemId::WOODEN_AXE, "wooden_axe"),
        (ItemId::STONE_AXE, "stone_axe"),
        (ItemId::IRON_AXE, "iron_axe"),
        (ItemId::WOODEN_SHOVEL, "wooden_shovel"),
        (ItemId::STONE_SHOVEL, "stone_shovel"),
        (ItemId::IRON_SHOVEL, "iron_shovel"),
        (ItemId::WOOD, "wood"),
        (ItemId::STICK, "stick"),
        (ItemId::IRON_INGOT, "iron_ingot"),
        (ItemId::RAW_PORKCHOP, "raw_porkchop"),
        (ItemId::RAW_BEEF, "raw_beef"),
        (ItemId::LEATHER, "leather"),
    ];

    ItemManifest {
        items: ICONS
            .iter()
            .map(|(item, icon)| ItemManifestEntry {
                id: item.raw(),
                name: Some(icon.to_string()),
                icon: Some(icon.to_string()),
                display_mode: Some(IconDisplayMode::Icon),
                alpha: Some(IconAlphaRule::Preserve),
            })
            .collect(),
    }
}

/// 物品图标解析失败原因。
#[derive(Clone, Debug, PartialEq)]
pub enum ItemTextureError {
    /// 同一物品 ID 重复配置。
    DuplicateItem { id: u16 },
}

impl fmt::Display for ItemTextureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ItemTextureError::DuplicateItem { id } => {
                write!(formatter, "物品图标配置存在重复 ID: {id}")
            }
        }
    }
}

impl std::error::Error for ItemTextureError {}

/// 一个物品图标的完整查询结果。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemIconView {
    pub texture: TextureId,
    pub uv: UVRect,
    pub display_mode: IconDisplayMode,
    pub transparency: Transparency,
    /// 是否使用了占位图标。
    pub placeholder: bool,
}

/// 物品图标映射表；按 `ItemId` 原始值下标查询。
#[derive(Clone, Debug, PartialEq)]
pub struct ItemTextureMap {
    icons: Vec<Option<ItemIconView>>,
    /// 显式配置为“不显示图标”的物品；与“未配置”区分开。
    hidden: Vec<bool>,
    fallback: Option<ItemIconView>,
}

impl ItemTextureMap {
    /// 空映射：任何查询都返回 `None`，用于测试与未初始化场景。
    pub fn empty() -> Self {
        Self {
            icons: vec![None; ItemId::LEATHER.raw() as usize + 1],
            hidden: vec![false; ItemId::LEATHER.raw() as usize + 1],
            fallback: None,
        }
    }

    /// 按配置与纹理注册表构建映射。
    pub fn from_manifest(
        manifest: &ItemManifest,
        registry: &TextureRegistry,
        fallback: TextureId,
        warnings: &mut Vec<String>,
    ) -> Result<Self, ItemTextureError> {
        let max_id = manifest
            .items
            .iter()
            .map(|entry| entry.id)
            .max()
            .unwrap_or(0)
            .max(ItemId::LEATHER.raw());
        let mut icons = vec![None; max_id as usize + 1];
        let mut hidden = vec![false; max_id as usize + 1];

        for (index, entry) in manifest.items.iter().enumerate() {
            if manifest.items[..index]
                .iter()
                .any(|other| other.id == entry.id)
            {
                return Err(ItemTextureError::DuplicateItem { id: entry.id });
            }

            let Some(item) = ItemId::from_raw(entry.id) else {
                warnings.push(format!(
                    "物品图标配置引用未注册 ItemId {}，已忽略",
                    entry.id
                ));
                continue;
            };
            if entry.display_mode == Some(IconDisplayMode::None) {
                hidden[item.raw() as usize] = true;
                continue;
            }

            let icon_name = entry.icon.as_deref().unwrap_or("<未配置>");
            let Some(texture_id) = registry.find(TextureType::Item, icon_name) else {
                warnings.push(format!(
                    "物品 {} 引用图标纹理 {icon_name} 不存在，使用占位图标",
                    entry.id
                ));
                icons[item.raw() as usize] = fallback_icon(registry, fallback);
                continue;
            };

            let (uv, transparency, placeholder) = match registry.get(texture_id) {
                Some(info) => {
                    let transparency = match entry.alpha.unwrap_or_default() {
                        IconAlphaRule::Preserve => info.transparency,
                        IconAlphaRule::ForceOpaque => Transparency::Opaque,
                    };
                    (info.uv, transparency, info.placeholder)
                }
                None => {
                    warnings.push(format!(
                        "物品 {} 的图标纹理 ID {} 未登记，使用占位图标",
                        entry.id,
                        texture_id.raw()
                    ));
                    icons[item.raw() as usize] = fallback_icon(registry, fallback);
                    continue;
                }
            };

            icons[item.raw() as usize] = Some(ItemIconView {
                texture: texture_id,
                uv,
                display_mode: entry.display_mode.unwrap_or_default(),
                transparency,
                placeholder,
            });
        }

        Ok(Self {
            icons,
            hidden,
            fallback: fallback_icon(registry, fallback),
        })
    }

    /// 查询物品图标；无效物品 ID 返回 `None`，已注册但未配置的物品返回占位图标。
    pub fn icon(&self, item: ItemId) -> Option<ItemIconView> {
        if !item.is_valid() {
            return None;
        }
        let index = item.raw() as usize;
        if self.hidden.get(index).copied().unwrap_or(false) {
            return None;
        }
        self.icons
            .get(index)
            .copied()
            .flatten()
            .or(self.fallback)
            .filter(|icon| icon.display_mode != IconDisplayMode::None)
    }

    /// 物品是否有可绘制图标。
    pub fn has_icon(&self, item: ItemId) -> bool {
        self.icon(item).is_some()
    }
}

fn fallback_icon(registry: &TextureRegistry, fallback: TextureId) -> Option<ItemIconView> {
    registry.get(fallback).map(|info| ItemIconView {
        texture: fallback,
        uv: info.uv,
        display_mode: IconDisplayMode::Icon,
        transparency: info.transparency,
        placeholder: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::texture::texture_registry::TextureSeed;

    fn registry_with(names: &[&str]) -> TextureRegistry {
        let mut registry = TextureRegistry::new();
        for name in names {
            registry
                .register(TextureSeed {
                    name: name.to_string(),
                    texture_type: TextureType::Item,
                    resource_path: format!("assets/textures/items/{name}.png"),
                    width: 16,
                    height: 16,
                    transparency: Transparency::Alpha,
                    placeholder: false,
                })
                .expect("登记纹理");
        }
        registry
    }

    #[test]
    fn builtin_manifest_covers_every_registered_item() {
        let manifest = builtin_manifest();
        for entry in &manifest.items {
            assert!(ItemId::from_raw(entry.id).is_some(), "ID 必须已注册");
            assert!(entry.icon.is_some());
        }
    }

    #[test]
    fn every_item_resolves_to_an_icon() {
        let registry = registry_with(&[
            "stone",
            "dirt",
            "grass",
            "wooden_pickaxe",
            "stone_pickaxe",
            "iron_pickaxe",
            "wooden_axe",
            "stone_axe",
            "iron_axe",
            "wooden_shovel",
            "stone_shovel",
            "iron_shovel",
            "wood",
            "stick",
            "iron_ingot",
            "raw_porkchop",
            "raw_beef",
            "leather",
        ]);
        let mut warnings = Vec::new();
        let map = ItemTextureMap::from_manifest(
            &builtin_manifest(),
            &registry,
            TextureId::new(0),
            &mut warnings,
        )
        .expect("构建图标映射");

        for entry in builtin_manifest().items {
            let item = ItemId::from_raw(entry.id).expect("有效物品");
            assert!(map.has_icon(item), "物品 {} 缺少图标", entry.id);
        }
        assert!(warnings.is_empty());
    }

    #[test]
    fn empty_and_unregistered_items_are_handled() {
        let registry = registry_with(&["stone"]);
        let mut warnings = Vec::new();
        let map = ItemTextureMap::from_manifest(
            &builtin_manifest(),
            &registry,
            TextureId::new(0),
            &mut warnings,
        )
        .expect("缺失图标不应中断构建");

        assert!(map.icon(ItemId::INVALID).is_none(), "空物品不显示图标");
        // 未配置图标的已注册物品回退到占位图标。
        assert!(map.has_icon(ItemId::IRON_PICKAXE));
        assert!(!warnings.is_empty());
    }

    #[test]
    fn transparent_icons_keep_their_alpha_mode() {
        let registry = registry_with(&["wooden_pickaxe"]);
        let manifest = ItemManifest {
            items: vec![ItemManifestEntry {
                id: ItemId::WOODEN_PICKAXE.raw(),
                icon: Some("wooden_pickaxe".to_string()),
                alpha: Some(IconAlphaRule::Preserve),
                display_mode: Some(IconDisplayMode::Icon),
                ..ItemManifestEntry::default()
            }],
        };
        let mut warnings = Vec::new();
        let map =
            ItemTextureMap::from_manifest(&manifest, &registry, TextureId::new(0), &mut warnings)
                .expect("构建图标映射");

        let icon = map.icon(ItemId::WOODEN_PICKAXE).expect("图标应存在");
        assert_eq!(icon.transparency, Transparency::Alpha);
        assert_eq!(icon.texture, TextureId::new(0));
    }

    #[test]
    fn display_mode_none_hides_the_icon() {
        let registry = registry_with(&["stone"]);
        let manifest = ItemManifest {
            items: vec![ItemManifestEntry {
                id: ItemId::STONE.raw(),
                icon: Some("stone".to_string()),
                display_mode: Some(IconDisplayMode::None),
                ..ItemManifestEntry::default()
            }],
        };
        let mut warnings = Vec::new();
        let map =
            ItemTextureMap::from_manifest(&manifest, &registry, TextureId::new(0), &mut warnings)
                .expect("构建图标映射");
        assert!(!map.has_icon(ItemId::STONE));
        assert!(warnings.is_empty());
    }

    #[test]
    fn duplicate_item_entries_are_rejected() {
        let registry = registry_with(&["stone"]);
        let manifest = ItemManifest {
            items: vec![
                ItemManifestEntry {
                    id: ItemId::STONE.raw(),
                    icon: Some("stone".to_string()),
                    ..ItemManifestEntry::default()
                },
                ItemManifestEntry {
                    id: ItemId::STONE.raw(),
                    icon: Some("stone".to_string()),
                    ..ItemManifestEntry::default()
                },
            ],
        };
        let mut warnings = Vec::new();
        let error =
            ItemTextureMap::from_manifest(&manifest, &registry, TextureId::new(0), &mut warnings)
                .expect_err("重复配置应报错");
        assert!(matches!(error, ItemTextureError::DuplicateItem { .. }));
    }
}
