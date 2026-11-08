//! Texture Atlas 自动打包：确定性布局、边缘像素扩展与 UV 生成。
//!
//! 开发者只维护独立 PNG，图集由资源系统自动生成。纹理可以为任意宽高
//! （含 UI 的非正方形大图），只要不超过图集容量；布局按高度、宽度降序，
//! 再按名称排序，因此相同资源集合无论读取顺序如何都会得到相同结果。

use std::fmt;

use super::types::{TextureId, TextureImage, Transparency, UVRect};

/// 每张纹理四周的像素间隔；间隔由边缘像素复制填充，防止采样串色。
pub const ATLAS_PADDING: u32 = 1;
/// 默认图集最大边长；超出时返回明确的容量错误而不是静默丢弃。
pub const MAX_ATLAS_SIZE: u32 = 4096;

/// 图集打包输入。
#[derive(Clone, Debug)]
pub struct AtlasSource {
    pub id: TextureId,
    pub name: String,
    pub image: TextureImage,
    pub transparency: Transparency,
}

/// 一张纹理在图集中的位置。
#[derive(Clone, Debug, PartialEq)]
pub struct AtlasEntry {
    pub id: TextureId,
    pub name: String,
    pub uv: UVRect,
}

/// 打包后的 RGBA8 图集。
#[derive(Clone, Debug, PartialEq)]
pub struct AtlasImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub entries: Vec<AtlasEntry>,
}

impl AtlasImage {
    /// 读取图集像素；越界返回 `None`。
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = ((y * self.width + x) * 4) as usize;
        let slice = self.pixels.get(index..index + 4)?;
        Some([slice[0], slice[1], slice[2], slice[3]])
    }

    /// 按纹理名查询图集位置。
    pub fn entry(&self, name: &str) -> Option<&AtlasEntry> {
        self.entries.iter().find(|entry| entry.name == name)
    }
}

/// 图集构建失败原因。
#[derive(Clone, Debug, PartialEq)]
pub enum AtlasError {
    /// 纹理数量超过图集容量。
    CapacityExceeded { count: usize, max_size: u32 },
    /// 输入中存在重复纹理名。
    DuplicateName(String),
    /// 输入中存在重复纹理 ID。
    DuplicateId(TextureId),
}

impl fmt::Display for AtlasError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AtlasError::CapacityExceeded { count, max_size } => write!(
                formatter,
                "图集容量不足: {count} 张纹理超出最大边长 {max_size} 像素"
            ),
            AtlasError::DuplicateName(name) => write!(formatter, "图集输入存在重复纹理名: {name}"),
            AtlasError::DuplicateId(id) => {
                write!(formatter, "图集输入存在重复纹理 ID: {}", id.raw())
            }
        }
    }
}

impl std::error::Error for AtlasError {}

/// 自动打包一份图集；布局由“边长从大到小、名称升序”决定，具备确定性。
pub fn build_atlas(sources: &[AtlasSource], max_size: u32) -> Result<AtlasImage, AtlasError> {
    if sources.is_empty() {
        return Ok(AtlasImage {
            width: 1,
            height: 1,
            pixels: vec![0; 4],
            entries: Vec::new(),
        });
    }

    let mut ordered: Vec<&AtlasSource> = sources.iter().collect();
    // 先按高度降序、再按宽度降序、最后按名称升序：货架式打包把相近尺寸排在一起，
    // 同时保证同一资源集合的布局与输入顺序无关。
    ordered.sort_by(|left, right| {
        right
            .image
            .height
            .cmp(&left.image.height)
            .then_with(|| right.image.width.cmp(&left.image.width))
            .then_with(|| left.name.cmp(&right.name))
    });

    for (index, source) in ordered.iter().enumerate() {
        if ordered[..index]
            .iter()
            .any(|other| other.name == source.name)
        {
            return Err(AtlasError::DuplicateName(source.name.clone()));
        }
        if ordered[..index].iter().any(|other| other.id == source.id) {
            return Err(AtlasError::DuplicateId(source.id));
        }
    }

    let count = ordered.len();
    // 货架式布局：逐行填充，行高由该行最高的纹理决定；宽高独立计算，支持矩形纹理。
    let mut placements = Vec::with_capacity(count);
    let mut cursor_x = 0u32;
    let mut cursor_y = 0u32;
    let mut row_height = 0u32;
    let mut used_width = 0u32;

    for source in &ordered {
        let cell_width = source.image.width + ATLAS_PADDING * 2;
        let cell_height = source.image.height + ATLAS_PADDING * 2;
        if cell_width > max_size || cell_height > max_size {
            return Err(AtlasError::CapacityExceeded { count, max_size });
        }
        if cursor_x + cell_width > max_size {
            cursor_y += row_height;
            cursor_x = 0;
            row_height = 0;
        }
        if cursor_y + cell_height > max_size {
            return Err(AtlasError::CapacityExceeded { count, max_size });
        }
        placements.push((cursor_x, cursor_y, cell_width, cell_height));
        cursor_x += cell_width;
        row_height = row_height.max(cell_height);
        used_width = used_width.max(cursor_x);
    }

    let used_height = cursor_y + row_height;
    let width = used_width.max(1).next_power_of_two();
    let height = used_height.max(1).next_power_of_two();
    if width > max_size || height > max_size {
        return Err(AtlasError::CapacityExceeded { count, max_size });
    }

    let mut pixels = vec![0; (width * height * 4) as usize];
    let mut entries = Vec::with_capacity(count);

    for (source, (origin_x, origin_y, cell_width, cell_height)) in ordered.iter().zip(placements) {
        let image_width = source.image.width;
        let image_height = source.image.height;
        // 包含间隔的格内，每个目标像素都从纹理内部钳制取样，
        // 从而把边缘像素复制到间隔区域，消除最近邻采样串色。
        for dy in 0..cell_height {
            let source_y =
                (dy as i32 - ATLAS_PADDING as i32).clamp(0, image_height as i32 - 1) as u32;
            for dx in 0..cell_width {
                let source_x =
                    (dx as i32 - ATLAS_PADDING as i32).clamp(0, image_width as i32 - 1) as u32;
                let Some([r, g, b, a]) = source.image.pixel(source_x, source_y) else {
                    continue;
                };
                let x = origin_x + dx;
                let y = origin_y + dy;
                let index = ((y * width + x) * 4) as usize;
                pixels[index..index + 4].copy_from_slice(&[r, g, b, a]);
            }
        }

        let uv = UVRect::from_pixels(
            origin_x + ATLAS_PADDING,
            origin_y + ATLAS_PADDING,
            image_width,
            image_height,
            width,
            height,
        );
        entries.push(AtlasEntry {
            id: source.id,
            name: source.name.clone(),
            uv,
        });
    }

    Ok(AtlasImage {
        width,
        height,
        pixels,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SIZE: u32 = 16;
    const TEST_CELL: u32 = TEST_SIZE + ATLAS_PADDING * 2;

    fn solid(name: &str, id: u32, color: [u8; 4]) -> AtlasSource {
        solid_sized(name, id, color, TEST_SIZE)
    }

    fn solid_sized(name: &str, id: u32, color: [u8; 4], size: u32) -> AtlasSource {
        let pixels = color
            .iter()
            .copied()
            .cycle()
            .take((size * size * 4) as usize)
            .collect();
        AtlasSource {
            id: TextureId::new(id),
            name: name.to_string(),
            image: TextureImage::new(size, size, pixels).expect("合法图片"),
            transparency: Transparency::Opaque,
        }
    }

    #[test]
    fn multiple_textures_pack_into_one_atlas() {
        let sources = vec![
            solid("stone", 0, [10, 10, 10, 255]),
            solid("dirt", 1, [20, 30, 40, 255]),
            solid("grass", 2, [30, 60, 30, 255]),
        ];
        let atlas = build_atlas(&sources, MAX_ATLAS_SIZE).expect("打包成功");
        assert_eq!(atlas.entries.len(), 3);
        assert!(atlas.width >= TEST_CELL);
        assert!(atlas.height >= TEST_CELL);
        assert_eq!(
            atlas.entry("stone").map(|entry| entry.id),
            Some(TextureId::new(0))
        );
    }

    #[test]
    fn mixed_texture_sizes_share_one_atlas() {
        let sources = vec![
            solid_sized("stone", 0, [10, 10, 10, 255], 16),
            solid_sized("dirt", 1, [20, 30, 40, 255], 32),
            solid_sized("grass", 2, [30, 60, 30, 255], 64),
            solid_sized("wood", 3, [80, 50, 20, 255], 128),
        ];
        let atlas = build_atlas(&sources, MAX_ATLAS_SIZE).expect("混合尺寸应可打包");
        assert_eq!(atlas.entries.len(), 4);

        for (name, size) in [("stone", 16), ("dirt", 32), ("grass", 64), ("wood", 128)] {
            let entry = atlas.entry(name).expect("条目存在");
            let pixel_width = ((entry.uv.max[0] - entry.uv.min[0]) * atlas.width as f32).round();
            let pixel_height = ((entry.uv.max[1] - entry.uv.min[1]) * atlas.height as f32).round();
            assert_eq!(pixel_width as u32, size, "{name} 的 UV 宽度应等于原始纹理");
            assert_eq!(pixel_height as u32, size, "{name} 的 UV 高度应等于原始纹理");
        }
    }

    #[test]
    fn uvs_never_overlap_and_stay_inside_the_atlas() {
        let sources: Vec<AtlasSource> = (0..10)
            .map(|index| solid(&format!("t{index:02}"), index, [255, 0, 0, 255]))
            .collect();
        let atlas = build_atlas(&sources, MAX_ATLAS_SIZE).expect("打包成功");

        let mut rects: Vec<[u32; 4]> = atlas
            .entries
            .iter()
            .map(|entry| {
                [
                    (entry.uv.min[0] * atlas.width as f32).round() as u32,
                    (entry.uv.min[1] * atlas.height as f32).round() as u32,
                    (entry.uv.max[0] * atlas.width as f32).round() as u32,
                    (entry.uv.max[1] * atlas.height as f32).round() as u32,
                ]
            })
            .collect();
        rects.sort_unstable();
        for rect in &rects {
            assert!(rect[0] < rect[2] && rect[1] < rect[3]);
            assert!(rect[2] <= atlas.width && rect[3] <= atlas.height);
        }
        for pair in rects.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let overlap = a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3];
            assert!(!overlap, "纹理 UV 不得重叠: {a:?} 与 {b:?}");
        }
    }

    #[test]
    fn padding_repeats_edge_pixels() {
        let mut source = solid("edge", 0, [255, 0, 0, 255]);
        // 让左上角像素为红色、其余为蓝色，检查间隔区域复制了边缘像素。
        for y in 0..TEST_SIZE {
            for x in 0..TEST_SIZE {
                let color = if x == 0 && y == 0 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                };
                let index = ((y * TEST_SIZE + x) * 4) as usize;
                source.image.pixels[index..index + 4].copy_from_slice(&color);
            }
        }

        let atlas = build_atlas(&[source], MAX_ATLAS_SIZE).expect("打包成功");
        let uv_min_x = (atlas.entries[0].uv.min[0] * atlas.width as f32).round() as u32;
        let uv_min_y = (atlas.entries[0].uv.min[1] * atlas.height as f32).round() as u32;

        // 纹理左上角向右一格是间隔区，应复制左上角红色像素。
        assert_eq!(
            atlas.pixel(uv_min_x - 1, uv_min_y),
            Some([255, 0, 0, 255]),
            "间隔像素必须复制相邻纹理边缘"
        );
        assert_eq!(atlas.pixel(uv_min_x + 1, uv_min_y), Some([0, 0, 255, 255]));
    }

    #[test]
    fn rectangular_textures_keep_their_aspect_ratio() {
        let sources = vec![
            AtlasSource {
                id: TextureId::new(0),
                name: "panel".to_string(),
                image: TextureImage::new(256, 64, vec![10; 256 * 64 * 4]).expect("合法图片"),
                transparency: Transparency::Opaque,
            },
            AtlasSource {
                id: TextureId::new(1),
                name: "tall".to_string(),
                image: TextureImage::new(32, 128, vec![20; 32 * 128 * 4]).expect("合法图片"),
                transparency: Transparency::Opaque,
            },
        ];
        let atlas = build_atlas(&sources, MAX_ATLAS_SIZE).expect("矩形纹理应可打包");
        for (name, width, height) in [("panel", 256, 64), ("tall", 32, 128)] {
            let entry = atlas.entry(name).expect("条目存在");
            let pixel_width = ((entry.uv.max[0] - entry.uv.min[0]) * atlas.width as f32).round();
            let pixel_height = ((entry.uv.max[1] - entry.uv.min[1]) * atlas.height as f32).round();
            assert_eq!(pixel_width as u32, width, "{name} 的 UV 宽度应保持");
            assert_eq!(pixel_height as u32, height, "{name} 的 UV 高度应保持");
        }
    }

    #[test]
    fn layout_is_deterministic_regardless_of_input_order() {
        let sources = vec![
            solid("stone", 0, [1, 1, 1, 255]),
            solid("dirt", 1, [2, 2, 2, 255]),
            solid("grass", 2, [3, 3, 3, 255]),
        ];
        let mut reversed = sources.clone();
        reversed.reverse();

        let first = build_atlas(&sources, MAX_ATLAS_SIZE).expect("打包成功");
        let second = build_atlas(&reversed, MAX_ATLAS_SIZE).expect("打包成功");
        assert_eq!(first.entries, second.entries);
        assert_eq!(first.pixels, second.pixels);
    }

    #[test]
    fn capacity_overflow_is_reported_instead_of_dropped() {
        let sources: Vec<AtlasSource> = (0..9)
            .map(|index| solid(&format!("t{index}"), index, [255, 0, 0, 255]))
            .collect();
        // 3×3 格需要 54 像素，限制到 32 必然容量不足。
        let error = build_atlas(&sources, 32).expect_err("容量应不足");
        match error {
            AtlasError::CapacityExceeded { count, max_size } => {
                assert_eq!(count, 9);
                assert_eq!(max_size, 32);
            }
            other => panic!("错误类型不符: {other}"),
        }
    }

    #[test]
    fn arbitrary_texture_sizes_are_packed_when_they_fit() {
        let source = AtlasSource {
            id: TextureId::new(0),
            name: "ui_panel".to_string(),
            image: TextureImage::new(1040, 80, vec![7; 1040 * 80 * 4]).expect("合法缓冲"),
            transparency: Transparency::Opaque,
        };
        let atlas = build_atlas(&[source], MAX_ATLAS_SIZE).expect("1040×80 应可打包");
        let entry = atlas.entry("ui_panel").expect("条目存在");
        let pixel_width = ((entry.uv.max[0] - entry.uv.min[0]) * atlas.width as f32).round();
        let pixel_height = ((entry.uv.max[1] - entry.uv.min[1]) * atlas.height as f32).round();
        assert_eq!(pixel_width as u32, 1040);
        assert_eq!(pixel_height as u32, 80);
    }
}
