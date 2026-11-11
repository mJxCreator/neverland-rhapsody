//! 纹理资源装配：从可配置资源根目录读取 manifest、PNG 与目录结构，
//! 生成方块/物品图集，并把结果交给渲染层。
//!
//! 装配过程不会因为单张纹理缺失而中止：缺失或损坏的图片使用程序生成的
//! 占位纹理，同时记录明确警告；容量或配置类错误会标记为“硬错误”，热重载
//! 时据此保留上一份有效资源。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use glam::Vec3;
use serde::de::DeserializeOwned;
use tracing::{error, warn};

use crate::item::{icon_color, ItemId};
use crate::world::{Block, Face};

use super::block_texture::{
    builtin_manifest as builtin_block_manifest, BlockManifest, BlockTextureMap,
};
use super::item_texture::{
    builtin_manifest as builtin_item_manifest, ItemIconView, ItemManifest, ItemTextureMap,
};
use super::material::MaterialRegistry;
use super::texture_atlas::{build_atlas, AtlasEntry, AtlasImage, AtlasSource, MAX_ATLAS_SIZE};
use super::texture_loader::{load_texture_png, load_ui_texture_png, TextureLoadError};
use super::texture_registry::{TextureRegistry, TextureSeed};
use super::types::{TextureId, TextureImage, TextureType, Transparency};
use super::ui_texture::{
    builtin_manifest as builtin_ui_manifest, image_name, UiManifest, UiManifestEntry, UiTextureMap,
};

/// 默认资源根目录（相对项目根）。
pub const DEFAULT_ASSET_ROOT: &str = "assets";
/// 缺失纹理统一使用的占位名称。
const MISSING_TEXTURE: &str = "__missing";

/// 装配后的全部纹理资源。
#[derive(Debug)]
pub struct TextureAssets {
    pub registry: TextureRegistry,
    pub block_atlas: AtlasImage,
    pub item_atlas: AtlasImage,
    pub ui_atlas: AtlasImage,
    pub block_textures: BlockTextureMap,
    pub item_textures: ItemTextureMap,
    pub ui_textures: UiTextureMap,
    pub materials: MaterialRegistry,
    /// 非致命问题：缺失图片、回退到占位等。
    pub warnings: Vec<String>,
    /// 硬错误：解码失败、图集容量不足、manifest 冲突等。
    pub errors: Vec<String>,
}

/// 资源装配统计，用于启动日志。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureAssetSummary {
    pub block_textures: usize,
    pub item_textures: usize,
    pub ui_textures: usize,
    pub placeholders: usize,
    pub warnings: usize,
    pub errors: usize,
}

impl TextureAssets {
    /// 从默认 `assets` 目录加载。
    pub fn load_default() -> Self {
        Self::load(Path::new(DEFAULT_ASSET_ROOT))
    }

    /// 从可配置资源根目录加载；结构为 `<root>/textures/blocks` 等。
    pub fn load(asset_root: impl AsRef<Path>) -> Self {
        let asset_root = asset_root.as_ref();
        let texture_root = asset_root.join("textures");
        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        let block_manifest = load_manifest(
            &texture_root.join("manifests").join("blocks.json"),
            &mut warnings,
        )
        .unwrap_or_else(builtin_block_manifest);
        let item_manifest = load_manifest(
            &texture_root.join("manifests").join("items.json"),
            &mut warnings,
        )
        .unwrap_or_else(builtin_item_manifest);
        let mut ui_manifest = load_manifest(
            &texture_root.join("manifests").join("ui.json"),
            &mut warnings,
        )
        .unwrap_or_else(builtin_ui_manifest);

        let block_requests = block_texture_requests(&block_manifest);
        let item_requests = item_texture_requests(&item_manifest);

        // 目录中额外的 PNG 也会进入图集，方便开发者先放素材、后补 manifest。
        let mut block_requests = block_requests;
        for name in png_names(&texture_root.join(TextureType::Block.directory())) {
            block_requests
                .entry(name.clone())
                .or_insert_with(|| hashed_color(&name));
        }
        let mut item_requests = item_requests;
        for name in png_names(&texture_root.join(TextureType::Item.directory())) {
            item_requests
                .entry(name.clone())
                .or_insert_with(|| hashed_color(&name));
        }

        // UI 目录中未登记的 PNG 自动作为同名的“整图拉伸”控件，避免必须同步改 manifest。
        let known_ui_images: BTreeSet<String> = ui_manifest.ui.iter().map(image_name).collect();
        for name in png_names(&texture_root.join(TextureType::Ui.directory())) {
            if known_ui_images.contains(&name) {
                continue;
            }
            ui_manifest.ui.push(UiManifestEntry {
                name: name.clone(),
                image: Some(name),
                draw: None,
                slice: None,
            });
        }
        let ui_requests = ui_texture_requests(&ui_manifest);

        let mut registry = TextureRegistry::new();
        let (block_atlas, block_fallback) = build_texture_atlas(
            &mut registry,
            TextureType::Block,
            &block_requests,
            &texture_root,
            &mut warnings,
            &mut errors,
        );
        let (item_atlas, item_fallback) = build_texture_atlas(
            &mut registry,
            TextureType::Item,
            &item_requests,
            &texture_root,
            &mut warnings,
            &mut errors,
        );
        let (ui_atlas, ui_fallback) = build_texture_atlas(
            &mut registry,
            TextureType::Ui,
            &ui_requests,
            &texture_root,
            &mut warnings,
            &mut errors,
        );

        let block_textures = match BlockTextureMap::from_manifest(
            &block_manifest,
            &registry,
            block_fallback,
            &mut warnings,
        ) {
            Ok(map) => map,
            Err(error) => {
                errors.push(error.to_string());
                BlockTextureMap::placeholder()
            }
        };

        let item_textures = match ItemTextureMap::from_manifest(
            &item_manifest,
            &registry,
            item_fallback,
            &mut warnings,
        ) {
            Ok(map) => map,
            Err(error) => {
                errors.push(error.to_string());
                ItemTextureMap::empty()
            }
        };

        let ui_textures = match UiTextureMap::from_manifest(
            &ui_manifest,
            &registry,
            ui_fallback,
            &mut warnings,
        ) {
            Ok(map) => map,
            Err(error) => {
                errors.push(error.to_string());
                UiTextureMap::empty()
            }
        };

        let assets = Self {
            registry,
            block_atlas,
            item_atlas,
            ui_atlas,
            block_textures,
            item_textures,
            ui_textures,
            materials: MaterialRegistry::new(),
            warnings,
            errors,
        };

        for warning in &assets.warnings {
            warn!("{warning}");
        }
        for error in &assets.errors {
            error!("{error}");
        }
        assets
    }

    /// 资源是否完整到可以安全替换上一份资源；缺失图片只算警告。
    pub fn assets_ok(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn summary(&self) -> TextureAssetSummary {
        TextureAssetSummary {
            block_textures: self
                .registry
                .iter()
                .filter(|info| info.texture_type == TextureType::Block)
                .count(),
            item_textures: self
                .registry
                .iter()
                .filter(|info| info.texture_type == TextureType::Item)
                .count(),
            ui_textures: self
                .registry
                .iter()
                .filter(|info| info.texture_type == TextureType::Ui)
                .count(),
            placeholders: self.registry.iter().filter(|info| info.placeholder).count(),
            warnings: self.warnings.len(),
            errors: self.errors.len(),
        }
    }

    /// 方块某个面的纹理 ID。
    pub fn block_texture(&self, block: Block, face: Face) -> TextureId {
        self.block_textures.face_texture(block, face)
    }

    /// 物品图标查询；空物品返回 `None`。
    pub fn item_icon(&self, item: ItemId) -> Option<ItemIconView> {
        self.item_textures.icon(item)
    }
}

/// 为一种纹理类型登记资源、生成图集并回写 UV。
fn build_texture_atlas(
    registry: &mut TextureRegistry,
    texture_type: TextureType,
    requests: &BTreeMap<String, [u8; 4]>,
    texture_root: &Path,
    warnings: &mut Vec<String>,
    errors: &mut Vec<String>,
) -> (AtlasImage, TextureId) {
    let directory = texture_root.join(texture_type.directory());
    let mut sources = Vec::new();
    let mut fallback_image = None;

    for (name, color) in requests {
        let path = directory.join(format!("{name}.png"));
        // UI 控件允许任意尺寸（上限 2048），方块与物品仍使用边长白名单。
        let loaded = if texture_type == TextureType::Ui {
            load_ui_texture_png(&path)
        } else {
            load_texture_png(&path)
        };
        let (image, placeholder, transparency) = match loaded {
            Ok(image) => {
                let transparency = if image.has_alpha() {
                    Transparency::Alpha
                } else {
                    Transparency::Opaque
                };
                (image, false, transparency)
            }
            Err(TextureLoadError::NotFound { .. }) => {
                warnings.push(format!(
                    "纹理缺失: {}，使用程序生成的占位纹理",
                    path_display(&path)
                ));
                (placeholder_texture(*color), true, Transparency::Opaque)
            }
            Err(error) => {
                errors.push(error.to_string());
                warnings.push(format!(
                    "纹理加载失败，使用程序生成的占位纹理: {}",
                    path_display(&path)
                ));
                (placeholder_texture(*color), true, Transparency::Opaque)
            }
        };

        if name == MISSING_TEXTURE {
            fallback_image = Some(image.clone());
        }

        let seed = TextureSeed {
            name: name.clone(),
            texture_type,
            resource_path: path_display(&path),
            width: image.width,
            height: image.height,
            transparency,
            placeholder,
        };
        let id = match registry.register(seed) {
            Ok(id) => id,
            Err(error) => {
                errors.push(error.to_string());
                continue;
            }
        };
        sources.push(AtlasSource {
            id,
            name: name.clone(),
            image,
            transparency,
        });
    }

    let fallback_id = registry
        .find(texture_type, MISSING_TEXTURE)
        .unwrap_or(TextureId::INVALID);

    let atlas = match build_atlas(&sources, MAX_ATLAS_SIZE) {
        Ok(atlas) => atlas,
        Err(error) => {
            errors.push(error.to_string());
            let fallback_source: Vec<AtlasSource> = sources
                .iter()
                .filter(|source| source.name == MISSING_TEXTURE)
                .cloned()
                .collect();
            build_atlas(&fallback_source, MAX_ATLAS_SIZE)
                .unwrap_or_else(|_| minimal_atlas(fallback_image, fallback_id))
        }
    };

    for entry in &atlas.entries {
        let Some(id) = registry.find(texture_type, &entry.name) else {
            errors.push(format!("图集条目未在注册表中找到: {}", entry.name));
            continue;
        };
        if let Err(error) = registry.apply_uv(id, entry.uv) {
            errors.push(error.to_string());
        }
    }

    (atlas, fallback_id)
}

/// 单个纹理登记失败时的最小图集，保证渲染层仍有可绑定资源。
fn minimal_atlas(image: Option<TextureImage>, id: TextureId) -> AtlasImage {
    let image = image.unwrap_or_else(|| placeholder_texture(MAGENTA));
    AtlasImage {
        width: image.width,
        height: image.height,
        pixels: image.pixels,
        entries: vec![AtlasEntry {
            id,
            name: MISSING_TEXTURE.to_string(),
            uv: super::types::UVRect::FULL,
        }],
    }
}

/// 从方块 manifest 收集纹理名，并用方块面色生成缺失时的占位颜色。
fn block_texture_requests(manifest: &BlockManifest) -> BTreeMap<String, [u8; 4]> {
    let mut requests = BTreeMap::new();
    for entry in &manifest.blocks {
        let Some(block) = block_from_id(entry.id) else {
            continue;
        };
        for face in Face::ALL {
            if let Some(name) = entry.texture_for(face) {
                requests
                    .entry(name.to_string())
                    .or_insert_with(|| color_to_rgba(block.face_color(face)));
            }
        }
        if let Some(name) = entry.default_texture.as_deref() {
            requests
                .entry(name.to_string())
                .or_insert_with(|| color_to_rgba(block.face_color(Face::PosY)));
        }
    }
    requests.insert(MISSING_TEXTURE.to_string(), MAGENTA);
    requests
}

/// 从物品 manifest 收集图标名，并用物品图标色生成缺失时的占位颜色。
fn item_texture_requests(manifest: &ItemManifest) -> BTreeMap<String, [u8; 4]> {
    let mut requests = BTreeMap::new();
    for entry in &manifest.items {
        let Some(item) = ItemId::from_raw(entry.id) else {
            continue;
        };
        if let Some(icon) = entry.icon.as_deref() {
            requests
                .entry(icon.to_string())
                .or_insert_with(|| color_to_rgba(icon_color(item)));
        }
    }
    requests.insert(MISSING_TEXTURE.to_string(), MAGENTA);
    requests
}

/// 从 UI manifest 收集纹理名，并用名称散列色生成缺失时的占位颜色。
fn ui_texture_requests(manifest: &UiManifest) -> BTreeMap<String, [u8; 4]> {
    let mut requests = BTreeMap::new();
    for entry in &manifest.ui {
        let image = image_name(entry);
        requests
            .entry(image.clone())
            .or_insert_with(|| hashed_color(&image));
    }
    requests.insert(MISSING_TEXTURE.to_string(), MAGENTA);
    requests
}

/// 扫描目录中的 PNG 文件名；目录不存在时返回空集合。
fn png_names(directory: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let names: BTreeSet<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let path = entry.path();
            let extension = path.extension()?.to_str()?.to_ascii_lowercase();
            if extension != "png" {
                return None;
            }
            path.file_stem()?.to_str().map(str::to_string)
        })
        .collect();
    names.into_iter().collect()
}

/// 读取 JSON manifest；文件不存在返回 `None`，解析失败记录警告。
fn load_manifest<T: DeserializeOwned>(path: &Path, warnings: &mut Vec<String>) -> Option<T> {
    if !path.is_file() {
        return None;
    }
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            warnings.push(format!(
                "manifest 读取失败: {} ({error})",
                path_display(path)
            ));
            return None;
        }
    };
    match serde_json::from_str(&text) {
        Ok(value) => Some(value),
        Err(error) => {
            warnings.push(format!(
                "manifest 解析失败: {} ({error})",
                path_display(path)
            ));
            None
        }
    }
}

fn block_from_id(id: u8) -> Option<Block> {
    match id {
        1 => Some(Block::Stone),
        2 => Some(Block::Dirt),
        3 => Some(Block::Grass),
        4 => Some(Block::Wood),
        5 => Some(Block::Campfire),
        6 => Some(Block::Bed),
        _ => None,
    }
}

const MAGENTA: [u8; 4] = [255, 0, 255, 255];

/// 生成 16×16 棋盘格占位纹理，保留资源名对应的主色。
fn placeholder_texture(color: [u8; 4]) -> TextureImage {
    let size = super::texture_loader::BASE_TEXTURE_SIZE;
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let dark = ((x / 2) + (y / 2)) % 2 == 0;
            let factor = if dark { 1.0 } else { 0.82 };
            pixels.extend_from_slice(&[
                (color[0] as f32 * factor) as u8,
                (color[1] as f32 * factor) as u8,
                (color[2] as f32 * factor) as u8,
                color[3],
            ]);
        }
    }
    TextureImage {
        width: size,
        height: size,
        pixels,
    }
}

fn color_to_rgba(color: Vec3) -> [u8; 4] {
    [
        (color.x.clamp(0.0, 1.0) * 255.0).round() as u8,
        (color.y.clamp(0.0, 1.0) * 255.0).round() as u8,
        (color.z.clamp(0.0, 1.0) * 255.0).round() as u8,
        255,
    ]
}

fn path_display(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// 为未在 manifest 中声明的纹理生成稳定且可区分的占位颜色。
fn hashed_color(name: &str) -> [u8; 4] {
    let hash = name.bytes().fold(0x9e37_79b9u32, |accumulator, byte| {
        accumulator.wrapping_mul(31).wrapping_add(byte as u32)
    });
    [
        96 + (hash & 0x7f) as u8,
        96 + ((hash >> 8) & 0x7f) as u8,
        96 + ((hash >> 16) & 0x7f) as u8,
        255,
    ]
}

/// 扫描两个资源目录中的 PNG 文件名，用于热重载监听。
pub fn watched_png_files(asset_root: &Path) -> Vec<PathBuf> {
    let texture_root = asset_root.join("textures");
    let mut files = Vec::new();
    for texture_type in [
        TextureType::Block,
        TextureType::Item,
        TextureType::Ui,
        TextureType::Entity,
    ] {
        for name in png_names(&texture_root.join(texture_type.directory())) {
            files.push(
                texture_root
                    .join(texture_type.directory())
                    .join(format!("{name}.png")),
            );
        }
    }
    files.push(texture_root.join("manifests").join("blocks.json"));
    files.push(texture_root.join("manifests").join("items.json"));
    files.push(texture_root.join("manifests").join("ui.json"));
    files.retain(|path| path.is_file());
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "neverland-rhapsody-assets-{label}-{}-{nanos}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("textures/blocks")).expect("创建方块目录");
        fs::create_dir_all(dir.join("textures/items")).expect("创建物品目录");
        fs::create_dir_all(dir.join("textures/manifests")).expect("创建 manifest 目录");
        dir
    }

    fn solid_png(path: &Path, color: [u8; 4]) {
        let size = crate::texture::texture_loader::BASE_TEXTURE_SIZE;
        let pixels: Vec<u8> = color
            .iter()
            .copied()
            .cycle()
            .take((size * size * 4) as usize)
            .collect();
        let file = fs::File::create(path).expect("创建 PNG");
        let mut encoder = png::Encoder::new(file, size, size);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("写入 PNG 头");
        writer.write_image_data(&pixels).expect("写入 PNG 数据");
    }

    #[test]
    fn missing_assets_produce_placeholders_without_hard_errors() {
        let dir = unique_dir("empty");
        let assets = TextureAssets::load(&dir);

        assert!(assets.assets_ok(), "缺图只应产生警告");
        assert!(!assets.warnings.is_empty(), "缺图必须输出明确警告");
        assert!(assets.summary().placeholders > 0);
        assert_ne!(
            assets.block_texture(Block::Grass, Face::PosY),
            assets.block_texture(Block::Grass, Face::NegY),
            "草方块顶面与底面必须映射到不同纹理"
        );
        for item in [ItemId::WOODEN_PICKAXE, ItemId::IRON_INGOT, ItemId::RAW_BEEF] {
            assert!(
                assets.item_icon(item).is_some(),
                "物品 {} 应能查询图标",
                item.raw()
            );
        }

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn real_pngs_replace_placeholders() {
        let dir = unique_dir("real");
        solid_png(&dir.join("textures/blocks/stone.png"), [120, 120, 130, 255]);
        solid_png(&dir.join("textures/blocks/dirt.png"), [110, 80, 50, 255]);
        solid_png(
            &dir.join("textures/blocks/grass_top.png"),
            [80, 150, 70, 255],
        );
        solid_png(
            &dir.join("textures/blocks/grass_side.png"),
            [100, 120, 60, 255],
        );
        solid_png(
            &dir.join("textures/items/wooden_pickaxe.png"),
            [150, 100, 50, 0],
        );
        let custom_manifest = r#"{
            "blocks": [
                {"id": 1, "top": "stone", "bottom": "stone", "east": "stone", "west": "stone", "north": "stone", "south": "stone"},
                {"id": 2, "default": "dirt"},
                {"id": 3, "top": "grass_top", "bottom": "dirt", "default": "grass_side"}
            ]
        }"#;
        fs::write(dir.join("textures/manifests/blocks.json"), custom_manifest)
            .expect("写 manifest");

        let assets = TextureAssets::load(&dir);
        let stone = assets
            .registry
            .find(TextureType::Block, "stone")
            .expect("stone 已登记");
        let info = assets.registry.get(stone).expect("纹理信息");
        assert!(!info.placeholder, "真实 PNG 不应标记为占位");
        assert_eq!(info.width, 16);
        assert_eq!(info.transparency, Transparency::Opaque);
        assert!(info.uv.width() > 0.0, "图集分配后 UV 必须有效");

        let icon = assets
            .item_icon(ItemId::WOODEN_PICKAXE)
            .expect("工具图标应存在");
        assert_eq!(icon.transparency, Transparency::Alpha);
        assert!(!icon.placeholder);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn corrupt_png_marks_a_hard_error_but_assets_still_exist() {
        let dir = unique_dir("corrupt");
        fs::write(dir.join("textures/blocks/stone.png"), b"broken").expect("写入损坏 PNG");

        let assets = TextureAssets::load(&dir);
        assert!(!assets.assets_ok(), "解码失败必须记硬错误");
        assert!(!assets.errors.is_empty());
        assert!(assets.block_texture(Block::Stone, Face::PosY).is_valid());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn item_and_block_atlases_are_independent() {
        let dir = unique_dir("atlases");
        let assets = TextureAssets::load(&dir);
        assert!(assets.block_atlas.width >= 1);
        assert!(assets.item_atlas.width >= 1);
        assert!(!assets.block_atlas.entries.is_empty());
        assert!(!assets.item_atlas.entries.is_empty());

        fs::remove_dir_all(&dir).ok();
    }
}
