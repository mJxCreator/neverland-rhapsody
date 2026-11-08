//! 纹理基础类型：稳定标识、图集归属、UV 范围、透明模式与图片数据。
//!
//! 纹理 ID 与 `BlockId` / `ItemId` 完全解耦：方块和物品只能通过映射表
//! 引用纹理，纹理本身不感知游戏业务编号。

/// 稳定纹理标识；同一组资源在相同目录结构下始终得到相同编号。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextureId(u32);

impl TextureId {
    /// 无效纹理：任何查询都应按“无纹理”处理。
    pub const INVALID: Self = Self(u32::MAX);

    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }

    pub const fn is_valid(self) -> bool {
        self.0 != u32::MAX
    }
}

/// 纹理所属的资源类别，决定默认目录与图集。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TextureType {
    Block,
    Item,
    /// 未来生物模型与材质使用，v0.16 仅保留目录与类型。
    Entity,
}

impl TextureType {
    /// 资源目录（相对 `assets/textures`）。
    pub const fn directory(self) -> &'static str {
        match self {
            TextureType::Block => "blocks",
            TextureType::Item => "items",
            TextureType::Entity => "entities",
        }
    }

    /// 默认图集；生物纹理在后续版本可独立组织。
    pub const fn atlas(self) -> AtlasId {
        match self {
            TextureType::Block => AtlasId::Block,
            TextureType::Item => AtlasId::Item,
            TextureType::Entity => AtlasId::Entity,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            TextureType::Block => "block",
            TextureType::Item => "item",
            TextureType::Entity => "entity",
        }
    }
}

/// 图集标识；方块与物品使用独立图集，避免世界纹理与 UI 图标耦合。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AtlasId {
    Block,
    Item,
    /// 未来生物材质预留，本版本不生成。
    Entity,
}

impl AtlasId {
    pub const fn label(self) -> &'static str {
        match self {
            AtlasId::Block => "block",
            AtlasId::Item => "item",
            AtlasId::Entity => "entity",
        }
    }
}

/// 透明模式；不透明纹理可继续使用现有深度写入，透明纹理需要混合。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Transparency {
    Opaque,
    Alpha,
}

impl Transparency {
    pub const fn is_alpha(self) -> bool {
        matches!(self, Transparency::Alpha)
    }
}

/// 纹理在图集中的归一化 UV 范围，`min` 为左上、`max` 为右下。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UVRect {
    pub min: [f32; 2],
    pub max: [f32; 2],
}

impl UVRect {
    /// 尚未分配图集位置时的零范围。
    pub const ZERO: Self = Self {
        min: [0.0, 0.0],
        max: [0.0, 0.0],
    };

    /// 整张图集。
    pub const FULL: Self = Self {
        min: [0.0, 0.0],
        max: [1.0, 1.0],
    };

    /// 由图集像素坐标构造归一化范围。
    pub fn from_pixels(
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        atlas_width: u32,
        atlas_height: u32,
    ) -> Self {
        let atlas_width = atlas_width.max(1) as f32;
        let atlas_height = atlas_height.max(1) as f32;
        Self {
            min: [x as f32 / atlas_width, y as f32 / atlas_height],
            max: [
                (x + width) as f32 / atlas_width,
                (y + height) as f32 / atlas_height,
            ],
        }
    }

    pub fn width(self) -> f32 {
        self.max[0] - self.min[0]
    }

    pub fn height(self) -> f32 {
        self.max[1] - self.min[1]
    }
}

/// 已解码的 RGBA8 图片；`pixels` 长度为 `width * height * 4`。
#[derive(Clone, Debug, PartialEq)]
pub struct TextureImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl TextureImage {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Option<Self> {
        let expected = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        (pixels.len() == expected).then_some(Self {
            width,
            height,
            pixels,
        })
    }

    /// 读取指定像素的 RGBA 分量；越界返回 `None`。
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = ((y * self.width + x) * 4) as usize;
        let slice = self.pixels.get(index..index + 4)?;
        Some([slice[0], slice[1], slice[2], slice[3]])
    }

    /// 是否包含任意非完全不透明像素。
    pub fn has_alpha(&self) -> bool {
        self.pixels.chunks_exact(4).any(|pixel| pixel[3] != 255)
    }
}

/// 一张纹理的静态登记信息。
#[derive(Clone, Debug, PartialEq)]
pub struct TextureInfo {
    pub id: TextureId,
    /// 资源名称（不含扩展名），由文件名派生。
    pub name: String,
    pub texture_type: TextureType,
    /// 相对项目根的 PNG 路径，例如 `assets/textures/blocks/stone.png`。
    pub resource_path: String,
    pub atlas: AtlasId,
    /// 图集分配后的归一化坐标；构建图集前为 [`UVRect::ZERO`]。
    pub uv: UVRect,
    pub width: u32,
    pub height: u32,
    pub transparency: Transparency,
    /// 该纹理是否使用了占位资源。
    pub placeholder: bool,
}

impl TextureInfo {
    pub fn is_registered(&self) -> bool {
        self.id.is_valid()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_ids_are_stable_values() {
        assert_eq!(TextureId::new(7).raw(), 7);
        assert!(TextureId::new(0).is_valid());
        assert!(!TextureId::INVALID.is_valid());
    }

    #[test]
    fn uv_rect_normalizes_pixel_bounds() {
        let rect = UVRect::from_pixels(16, 32, 16, 16, 64, 64);
        assert_eq!(rect.min, [0.25, 0.5]);
        assert_eq!(rect.max, [0.5, 0.75]);
        assert!((rect.width() - 0.25).abs() < 1e-6);
    }

    #[test]
    fn images_validate_buffer_size_and_read_pixels() {
        let image = TextureImage::new(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]).expect("合法图片");
        assert_eq!(image.pixel(0, 0), Some([1, 2, 3, 4]));
        assert_eq!(image.pixel(1, 0), Some([5, 6, 7, 8]));
        assert_eq!(image.pixel(2, 0), None);
        assert!(image.has_alpha());
        assert!(TextureImage::new(2, 1, vec![0; 7]).is_none());
    }

    #[test]
    fn texture_types_map_to_their_own_atlas() {
        assert_eq!(TextureType::Block.atlas(), AtlasId::Block);
        assert_eq!(TextureType::Item.atlas(), AtlasId::Item);
        assert_eq!(TextureType::Item.directory(), "items");
    }
}
