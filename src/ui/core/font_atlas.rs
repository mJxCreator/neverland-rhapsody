//! 字体图集：运行时把 TTF 字形栅格化为覆盖率位图并打包进单张图集。
//!
//! 图集为固定 2048×2048 的 R8（覆盖率），按字形像素字号缓存；同一字符的
//! 12/24/36px 版本分别登记。字形只在首次使用时栅格化，避免启动时展开整份字体。
//! 图集新增字形时记录脏矩形，由渲染器每帧上传一次。

use std::collections::HashMap;

use fontdue::{Font, FontSettings};

/// 图集边长；12px 中文下可容纳上万字形。
pub const ATLAS_SIZE: u32 = 2048;
/// 字形之间保留 1 像素间隔，避免最近邻采样串色。
const GLYPH_PADDING: u32 = 1;

/// 一个已栅格化字形在图集中的位置与排版度量。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    /// 图集 UV（左上角）。
    pub uv: [f32; 2],
    /// 字形位图像素尺寸。
    pub size: [f32; 2],
    /// 相对笔位的左偏移与相对基线的顶部偏移（向上为正）。
    pub offset: [f32; 2],
    /// 前进宽度（像素）。
    pub advance: f32,
}

/// 图集脏矩形（像素坐标）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirtyRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

struct Shelf {
    y: u32,
    height: u32,
    next_x: u32,
}

/// CPU 端字体图集。
pub struct FontAtlas {
    font: Option<Font>,
    pixels: Vec<u8>,
    size: u32,
    glyphs: HashMap<(char, u32), Glyph>,
    shelves: Vec<Shelf>,
    dirty: Option<DirtyRect>,
    warned_missing: bool,
    warned_full: bool,
}

impl Default for FontAtlas {
    fn default() -> Self {
        Self::new()
    }
}

impl FontAtlas {
    /// 创建空图集；字体由 [`FontAtlas::set_font`] 在启动时载入。
    pub fn new() -> Self {
        Self {
            font: None,
            pixels: vec![0; (ATLAS_SIZE * ATLAS_SIZE) as usize],
            size: ATLAS_SIZE,
            glyphs: HashMap::new(),
            shelves: Vec::new(),
            dirty: None,
            warned_missing: false,
            warned_full: false,
        }
    }

    /// 载入 TTF；重复调用时忽略后一次。
    pub fn set_font(&mut self, bytes: Vec<u8>) -> Result<(), String> {
        if self.font.is_some() {
            return Ok(());
        }
        let font = Font::from_bytes(bytes, FontSettings::default())
            .map_err(|error| error.to_string())?;
        self.font = Some(font);
        Ok(())
    }

    #[allow(dead_code)]
    pub fn has_font(&self) -> bool {
        self.font.is_some()
    }

    /// 原始图集像素，用于创建 GPU 纹理时的初始上传。
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// 行高（ascent - descent + line_gap）。
    pub fn line_height(&mut self, px: u32) -> f32 {
        let Some(font) = self.font.as_ref() else {
            return px as f32 * 0.8;
        };
        match font.horizontal_line_metrics(px as f32) {
            Some(metrics) => {
                (metrics.ascent - metrics.descent + metrics.line_gap).max(px as f32)
            }
            None => px as f32,
        }
    }

    /// 基线到文字顶部的距离。
    pub fn ascent(&mut self, px: u32) -> f32 {
        let Some(font) = self.font.as_ref() else {
            return px as f32 * 0.8;
        };
        font.horizontal_line_metrics(px as f32)
            .map_or(px as f32 * 0.8, |metrics| metrics.ascent)
    }

    /// 单个字符的前进宽度；不占用图集空间。
    pub fn advance(&mut self, character: char, px: u32) -> f32 {
        let Some(font) = self.font.as_ref() else {
            return px as f32 * 0.6;
        };
        font.metrics(character, px as f32).advance_width
    }

    /// 取得（必要时栅格化）字形；图集已满或字体缺失时返回 `None`。
    pub fn glyph(&mut self, character: char, px: u32) -> Option<Glyph> {
        if let Some(glyph) = self.glyphs.get(&(character, px)) {
            return Some(*glyph);
        }
        let font = self.font.as_ref()?;
        let metrics = font.metrics(character, px as f32);
        let advance = metrics.advance_width;
        if metrics.width == 0 || metrics.height == 0 {
            let glyph = Glyph {
                uv: [0.0; 2],
                size: [0.0; 2],
                offset: [0.0; 2],
                advance,
            };
            self.glyphs.insert((character, px), glyph);
            return Some(glyph);
        }

        let (metrics, bitmap) = font.rasterize(character, px as f32);
        let width = metrics.width as u32;
        let height = metrics.height as u32;
        let Some((x, y)) = self.allocate(width, height) else {
            if !self.warned_full {
                self.warned_full = true;
                tracing::error!(
                    atlas = ATLAS_SIZE,
                    "字体图集已满，后续字形使用缺字回退"
                );
            }
            return None;
        };

        for row in 0..height {
            let source = metrics.width * row as usize;
            let target = ((y + row) * self.size + x) as usize;
            self.pixels[target..target + metrics.width]
                .copy_from_slice(&bitmap[source..source + metrics.width]);
        }
        self.mark_dirty(x, y, width, height);

        let glyph = Glyph {
            uv: [
                x as f32 / self.size as f32,
                y as f32 / self.size as f32,
            ],
            size: [width as f32, height as f32],
            offset: [metrics.xmin as f32, metrics.ymin as f32],
            advance,
        };
        self.glyphs.insert((character, px), glyph);
        Some(glyph)
    }

    /// 取走当前脏矩形与对应像素（按 256 字节对齐的行跨度）。
    pub fn take_dirty(&mut self) -> Option<(DirtyRect, Vec<u8>, u32)> {
        let rect = self.dirty.take()?;
        let stride = rect.width.div_ceil(256) * 256;
        let mut pixels = vec![0u8; (stride * rect.height) as usize];
        for row in 0..rect.height {
            let source =
                ((rect.y + row) * self.size + rect.x) as usize;
            let target = (row * stride) as usize;
            let width = rect.width as usize;
            pixels[target..target + width]
                .copy_from_slice(&self.pixels[source..source + width]);
        }
        Some((rect, pixels, stride))
    }

    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        if width + GLYPH_PADDING > self.size || height > self.size {
            return None;
        }
        for shelf in self.shelves.iter_mut() {
            if shelf.height >= height && shelf.next_x + width <= self.size {
                let x = shelf.next_x;
                shelf.next_x += width + GLYPH_PADDING;
                return Some((x, shelf.y));
            }
        }
        let y = self
            .shelves
            .last()
            .map_or(0, |shelf| shelf.y + shelf.height + GLYPH_PADDING);
        if y + height > self.size {
            return None;
        }
        self.shelves.push(Shelf {
            y,
            height,
            next_x: width + GLYPH_PADDING,
        });
        Some((0, y))
    }

    fn mark_dirty(&mut self, x: u32, y: u32, width: u32, height: u32) {
        self.dirty = Some(match self.dirty {
            None => DirtyRect {
                x,
                y,
                width,
                height,
            },
            Some(rect) => {
                let left = rect.x.min(x);
                let top = rect.y.min(y);
                let right = (rect.x + rect.width).max(x + width);
                let bottom = (rect.y + rect.height).max(y + height);
                DirtyRect {
                    x: left,
                    y: top,
                    width: right - left,
                    height: bottom - top,
                }
            }
        });
    }

    /// 标记缺字警告只输出一次。
    pub fn warn_missing_once(&mut self, character: char) {
        if self.warned_missing {
            return;
        }
        self.warned_missing = true;
        tracing::warn!(character = %character, "字体缺少字形，使用回退符号");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_atlas_reports_missing_font_and_zero_advance() {
        let mut atlas = FontAtlas::new();
        assert!(!atlas.has_font());
        assert!(atlas.glyph('A', 12).is_none());
        assert!((atlas.advance('A', 12) - 7.2).abs() < 1e-4);
        assert_eq!(atlas.take_dirty(), None);
    }

    #[test]
    fn a_real_font_maps_chinese_to_atlas_coverage() {
        let path = std::path::Path::new(
            "assets/fonts/fusion-pixel-12px-monospaced-zh_hans.ttf",
        );
        let Ok(bytes) = std::fs::read(path) else {
            // 未随仓库提供字体时跳过，内置回退路径由其他测试覆盖。
            return;
        };
        let mut atlas = FontAtlas::new();
        atlas.set_font(bytes).expect("字体应能解析");
        let glyph = atlas.glyph('中', 12).expect("中文应有字形");
        assert!(glyph.size[0] > 0.0 && glyph.size[1] > 0.0);
        assert!(glyph.advance > 0.0);
        let (rect, pixels, stride) = atlas.take_dirty().expect("新字形应标记脏区");
        assert!(rect.width > 0 && rect.height > 0);
        assert_eq!(stride % 256, 0);
        assert!(pixels.iter().any(|&coverage| coverage > 0));
    }

    #[test]
    fn allocation_advances_to_new_shelves_when_a_row_is_full() {
        let mut atlas = FontAtlas::new();
        let first = atlas.allocate(2000, 12).expect("首行可放置");
        let second = atlas.allocate(100, 12).expect("首行剩余空间或新行");
        assert_ne!(first, second);
        // 小于已用高度的字形仍可复用首行空白。
        assert!(atlas.allocate(40, 8).is_some());
        assert!(atlas.allocate(ATLAS_SIZE + 1, 8).is_none());
    }

    #[test]
    fn dirty_rect_unions_new_glyphs_and_uploads_padded_rows() {
        let mut atlas = FontAtlas::new();
        atlas.mark_dirty(10, 20, 6, 8);
        atlas.mark_dirty(30, 10, 5, 5);
        let (rect, pixels, stride) = atlas.take_dirty().expect("存在脏区域");
        assert_eq!(
            rect,
            DirtyRect {
                x: 10,
                y: 10,
                width: 25,
                height: 18
            }
        );
        assert_eq!(stride, 256);
        assert_eq!(pixels.len(), (256 * 18) as usize);
        assert_eq!(atlas.take_dirty(), None);
    }
}
