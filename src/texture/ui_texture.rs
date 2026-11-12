//! UI 控件纹理映射：`ui.json` 解析、控件名到独立 UI 图集 UV 的查询。
//!
//! UI 纹理与物品图标使用不同图集：面板通常是非正方形大图，无法塞进按物品
//! 图标尺寸优化的图集。本模块只回答“控件用哪张图、如何切分”，不参与布局。

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ui::texture_ratio;

use super::texture_registry::TextureRegistry;
use super::types::{TextureId, TextureType, Transparency, UVRect};

/// 控件纹理的绘制方式。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiDrawMode {
    /// 整图缩放到目标矩形。
    #[default]
    Stretch,
    /// 四角保真、边与中心拉伸的九宫格。
    NineSlice,
}

/// `ui.json` 根结构。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UiManifest {
    #[serde(default)]
    pub ui: Vec<UiManifestEntry>,
}

/// 单个 UI 控件的纹理配置。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UiManifestEntry {
    /// 代码引用的控件键，必须唯一。
    pub name: String,
    /// `ui/` 下的 PNG 文件名；省略时与 `name` 相同。
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub draw: Option<UiDrawMode>,
    /// 九宫格切边 `[左, 上, 右, 下]`，单位像素。
    #[serde(default)]
    pub slice: Option<[u32; 4]>,
}

/// 内置 UI 控件清单；即使 `ui.json` 缺失，放入同名 PNG 也会自动生效。
pub fn builtin_manifest() -> UiManifest {
    const ENTRIES: [(&str, UiDrawMode, [u32; 4]); 19] = [
        ("hotbar_bg", UiDrawMode::NineSlice, [8, 8, 8, 8]),
        ("hotbar_slot", UiDrawMode::Stretch, [0, 0, 0, 0]),
        ("hotbar_slot_selected", UiDrawMode::Stretch, [0, 0, 0, 0]),
        ("inventory_panel", UiDrawMode::NineSlice, [12, 12, 12, 12]),
        ("inventory_slot", UiDrawMode::Stretch, [0, 0, 0, 0]),
        ("inventory_slot_hover", UiDrawMode::Stretch, [0, 0, 0, 0]),
        ("inventory_slot_selected", UiDrawMode::Stretch, [0, 0, 0, 0]),
        ("tooltip_bg", UiDrawMode::NineSlice, [8, 8, 8, 8]),
        ("crafting_row", UiDrawMode::NineSlice, [6, 6, 6, 6]),
        ("crafting_row_hover", UiDrawMode::NineSlice, [6, 6, 6, 6]),
        ("crafting_row_selected", UiDrawMode::NineSlice, [6, 6, 6, 6]),
        ("crosshair", UiDrawMode::Stretch, [0, 0, 0, 0]),
        ("heart_full", UiDrawMode::Stretch, [0, 0, 0, 0]),
        ("heart_half", UiDrawMode::Stretch, [0, 0, 0, 0]),
        ("heart_empty", UiDrawMode::Stretch, [0, 0, 0, 0]),
        ("progress_bg", UiDrawMode::NineSlice, [4, 4, 4, 4]),
        ("progress_fill", UiDrawMode::NineSlice, [4, 4, 4, 4]),
        ("damage_vignette", UiDrawMode::NineSlice, [16, 16, 16, 16]),
        ("death_overlay", UiDrawMode::Stretch, [0, 0, 0, 0]),
    ];
    // 死亡面板与按钮单独列出，保持清单可读。
    let mut entries: Vec<UiManifestEntry> = ENTRIES
        .iter()
        .map(|(name, draw, slice)| UiManifestEntry {
            name: name.to_string(),
            image: Some(name.to_string()),
            draw: Some(*draw),
            slice: Some(*slice),
        })
        .collect();
    for (name, draw, slice) in [
        ("death_panel", UiDrawMode::NineSlice, [8, 8, 8, 8]),
        ("button", UiDrawMode::NineSlice, [6, 6, 6, 6]),
        ("button_hover", UiDrawMode::NineSlice, [6, 6, 6, 6]),
    ] {
        entries.push(UiManifestEntry {
            name: name.to_string(),
            image: Some(name.to_string()),
            draw: Some(draw),
            slice: Some(slice),
        });
    }
    UiManifest { ui: entries }
}

/// 控件对应的纹理名；`image` 可写 `panel` 或 `panel.png`，统一取无扩展名的资源名。
pub fn image_name(entry: &UiManifestEntry) -> String {
    entry
        .image
        .as_deref()
        .unwrap_or(&entry.name)
        .trim_end_matches(".png")
        .to_string()
}

/// UI 纹理解析失败原因。
#[derive(Clone, Debug, PartialEq)]
pub enum UiTextureError {
    /// 控件键重复。
    DuplicateName { name: String },
}

impl fmt::Display for UiTextureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UiTextureError::DuplicateName { name } => {
                write!(formatter, "UI 纹理存在重复控件名: {name}")
            }
        }
    }
}

impl std::error::Error for UiTextureError {}

/// 一个 UI 控件的完整绘制信息。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiTextureView {
    pub texture: TextureId,
    pub uv: UVRect,
    /// 源图像素尺寸，供九宫格把切边换算为 UV 比例。
    pub source_size: [u32; 2],
    pub draw: UiDrawMode,
    pub slice: [u32; 4],
    pub transparency: Transparency,
    /// 整图拉伸时锁定的目标长宽比 `[宽, 高]`；`None` 表示随目标矩形铺满。
    pub aspect: Option<[u32; 2]>,
    /// 是否使用了占位纹理；占位控件不参与绘制，由代码回退纯色。
    pub placeholder: bool,
}

/// 控件名到绘制信息的映射。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiTextureMap {
    entries: BTreeMap<String, UiTextureView>,
}

impl UiTextureMap {
    /// 空映射：所有查询返回 `None`，用于测试与未初始化场景。
    pub fn empty() -> Self {
        Self::default()
    }

    /// 按配置与纹理注册表构建映射。
    pub fn from_manifest(
        manifest: &UiManifest,
        registry: &TextureRegistry,
        fallback: TextureId,
        warnings: &mut Vec<String>,
    ) -> Result<Self, UiTextureError> {
        let mut entries = BTreeMap::new();
        for (index, entry) in manifest.ui.iter().enumerate() {
            if manifest.ui[..index]
                .iter()
                .any(|other| other.name == entry.name)
            {
                return Err(UiTextureError::DuplicateName {
                    name: entry.name.clone(),
                });
            }

            let image = image_name(entry);
            let Some(texture_id) = registry.find(TextureType::Ui, &image) else {
                warnings.push(format!(
                    "UI 控件 {} 引用纹理 {image} 不存在，回退纯色绘制",
                    entry.name
                ));
                if let Some(view) = fallback_view(registry, fallback) {
                    entries.insert(entry.name.clone(), view);
                }
                continue;
            };
            let Some(info) = registry.get(texture_id) else {
                warnings.push(format!(
                    "UI 控件 {} 的纹理 ID {} 未登记，回退纯色绘制",
                    entry.name,
                    texture_id.raw()
                ));
                continue;
            };

            // 缺图时使用占位纹理，保留条目但不参与绘制，也不能按真实图校验九宫格切边。
            let (draw, slice) = if info.placeholder {
                (UiDrawMode::Stretch, [0; 4])
            } else {
                let draw = entry.draw.unwrap_or_default();
                let slice = entry.slice.unwrap_or([0; 4]);
                if draw == UiDrawMode::NineSlice && !valid_slice(slice, info.width, info.height) {
                    // 切边与当前图片尺寸不匹配时降级为整图拉伸，避免整张 UI 图集不可用。
                    warnings.push(format!(
                        "UI 控件 {} 的九宫格切边 {:?} 与图片 {}×{} 不匹配，已回退整图拉伸",
                        entry.name, slice, info.width, info.height
                    ));
                    (UiDrawMode::Stretch, [0; 4])
                } else {
                    (draw, slice)
                }
            };

            // 登记了比例的控件按代码规定的比例绘制；比例随内容/窗口变化的控件不锁定。
            let aspect = if info.placeholder {
                None
            } else {
                texture_ratio::expected_ratio(&entry.name)
            };
            if let Some((ratio_width, ratio_height)) = aspect {
                if ratio_mismatch(info.width, info.height, [ratio_width, ratio_height]) {
                    warnings.push(format!(
                        "UI 控件 {} 的贴图 {}×{} 与规定比例 {}:{} 不一致，绘制时按 {}:{} 缩放，请按规范导出或修改 src/ui/texture_ratio.rs",
                        entry.name,
                        info.width,
                        info.height,
                        ratio_width,
                        ratio_height,
                        ratio_width,
                        ratio_height
                    ));
                }
            }
            let aspect = aspect.map(|(width, height)| [width, height]);

            entries.insert(
                entry.name.clone(),
                UiTextureView {
                    texture: texture_id,
                    uv: info.uv,
                    source_size: [info.width, info.height],
                    draw,
                    slice,
                    transparency: info.transparency,
                    aspect,
                    placeholder: info.placeholder,
                },
            );
        }
        Ok(Self { entries })
    }

    /// 查询控件；缺失、占位或 UV 无效时返回 `None`，调用方回退纯色绘制。
    pub fn get(&self, name: &str) -> Option<UiTextureView> {
        self.entries.get(name).copied().filter(|view| {
            !view.placeholder && view.uv.max[0] > view.uv.min[0] && view.uv.max[1] > view.uv.min[1]
        })
    }

    /// 已登记的控件数量（含占位），用于日志与测试。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn fallback_view(registry: &TextureRegistry, fallback: TextureId) -> Option<UiTextureView> {
    let info = registry.get(fallback)?;
    Some(UiTextureView {
        texture: fallback,
        uv: info.uv,
        source_size: [info.width, info.height],
        draw: UiDrawMode::Stretch,
        slice: [0; 4],
        transparency: info.transparency,
        aspect: None,
        placeholder: true,
    })
}

fn valid_slice(slice: [u32; 4], width: u32, height: u32) -> bool {
    let [left, top, right, bottom] = slice;
    if left == 0 || top == 0 || right == 0 || bottom == 0 {
        return false;
    }
    left.saturating_add(right) < width && top.saturating_add(bottom) < height
}

/// 图片宽高比与规定比例是否明显不符（偏差超过 1%）。
fn ratio_mismatch(width: u32, height: u32, aspect: [u32; 2]) -> bool {
    let (ratio_width, ratio_height) = (aspect[0] as u64, aspect[1] as u64);
    if width == 0 || height == 0 || ratio_width == 0 || ratio_height == 0 {
        return false;
    }
    let expected = width as u64 * ratio_height;
    let actual = height as u64 * ratio_width;
    let larger = expected.max(actual);
    larger > 0 && expected.abs_diff(actual) * 100 > larger
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::texture::texture_registry::TextureSeed;

    fn registry_with(entries: &[(&str, u32, u32, bool)]) -> TextureRegistry {
        let mut registry = TextureRegistry::new();
        for (name, width, height, placeholder) in entries {
            let id = registry
                .register(TextureSeed {
                    name: name.to_string(),
                    texture_type: TextureType::Ui,
                    resource_path: format!("assets/textures/ui/{name}.png"),
                    width: *width,
                    height: *height,
                    transparency: Transparency::Alpha,
                    placeholder: *placeholder,
                })
                .expect("登记 UI 纹理");
            // 真实流程由图集回写 UV；测试给一个非退化的占位 UV。
            registry
                .apply_uv(
                    id,
                    UVRect {
                        min: [0.0, 0.0],
                        max: [0.5, 0.5],
                    },
                )
                .expect("回写 UV");
        }
        registry
    }

    #[test]
    fn builtin_manifest_defines_unique_controls() {
        let manifest = builtin_manifest();
        assert!(!manifest.ui.is_empty());
        for (index, entry) in manifest.ui.iter().enumerate() {
            assert!(
                !manifest.ui[..index]
                    .iter()
                    .any(|other| other.name == entry.name),
                "控件名 {} 重复",
                entry.name
            );
            assert!(entry.image.is_some());
        }
    }

    #[test]
    fn rectangular_images_resolve_with_optional_extension() {
        let registry = registry_with(&[("inventory_panel", 256, 64, false)]);
        let manifest = UiManifest {
            ui: vec![UiManifestEntry {
                name: "inventory_panel".to_string(),
                image: Some("inventory_panel.png".to_string()),
                draw: Some(UiDrawMode::NineSlice),
                slice: Some([12, 12, 12, 12]),
            }],
        };
        let mut warnings = Vec::new();
        let map =
            UiTextureMap::from_manifest(&manifest, &registry, TextureId::INVALID, &mut warnings)
                .expect("矩形 UI 纹理应可解析");

        let view = map.get("inventory_panel").expect("控件已登记");
        assert_eq!(view.source_size, [256, 64]);
        assert_eq!(view.draw, UiDrawMode::NineSlice);
        assert_eq!(view.aspect, None, "九宫格控件不锁定长宽比");
        assert!(warnings.is_empty());
    }

    #[test]
    fn stretch_controls_carry_the_declared_ratio_and_warn_on_mismatch() {
        let registry =
            registry_with(&[("crosshair", 32, 32, false), ("heart_full", 32, 16, false)]);
        let manifest = UiManifest {
            ui: vec![
                UiManifestEntry {
                    name: "crosshair".to_string(),
                    ..UiManifestEntry::default()
                },
                UiManifestEntry {
                    name: "heart_full".to_string(),
                    ..UiManifestEntry::default()
                },
            ],
        };
        let mut warnings = Vec::new();
        let map =
            UiTextureMap::from_manifest(&manifest, &registry, TextureId::INVALID, &mut warnings)
                .expect("拉伸控件应可解析");

        assert_eq!(
            map.get("crosshair").expect("准星已登记").aspect,
            Some([1, 1])
        );
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("heart_full") && warning.contains("1:1")),
            "比例不符时必须给出明确警告"
        );
    }

    #[test]
    fn placeholder_controls_are_hidden_for_solid_fallback() {
        let registry = registry_with(&[("inventory_panel", 16, 16, true)]);
        let manifest = UiManifest {
            ui: vec![UiManifestEntry {
                name: "inventory_panel".to_string(),
                image: None,
                draw: Some(UiDrawMode::NineSlice),
                slice: Some([12, 12, 12, 12]),
            }],
        };
        let mut warnings = Vec::new();
        let map =
            UiTextureMap::from_manifest(&manifest, &registry, TextureId::INVALID, &mut warnings)
                .expect("占位纹理不应因切边无效而报错");
        assert!(map.get("inventory_panel").is_none());
        assert_eq!(map.len(), 1, "占位条目仍保留用于日志");
    }

    #[test]
    fn duplicate_controls_are_rejected_and_invalid_slices_fall_back() {
        let registry = registry_with(&[("panel", 64, 64, false)]);
        let duplicate = UiManifest {
            ui: vec![
                UiManifestEntry {
                    name: "panel".to_string(),
                    ..UiManifestEntry::default()
                },
                UiManifestEntry {
                    name: "panel".to_string(),
                    ..UiManifestEntry::default()
                },
            ],
        };
        let mut warnings = Vec::new();
        assert_eq!(
            UiTextureMap::from_manifest(&duplicate, &registry, TextureId::INVALID, &mut warnings),
            Err(UiTextureError::DuplicateName {
                name: "panel".to_string()
            })
        );

        let invalid = UiManifest {
            ui: vec![UiManifestEntry {
                name: "panel".to_string(),
                draw: Some(UiDrawMode::NineSlice),
                slice: Some([32, 4, 32, 4]),
                ..UiManifestEntry::default()
            }],
        };
        let map =
            UiTextureMap::from_manifest(&invalid, &registry, TextureId::INVALID, &mut warnings)
                .expect("切边不匹配不应让整张 UI 图集失败");
        let view = map.get("panel").expect("控件仍应可用");
        assert_eq!(view.draw, UiDrawMode::Stretch, "应降级为整图拉伸");
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("已回退整图拉伸")),
            "降级必须输出明确警告"
        );
    }
}
