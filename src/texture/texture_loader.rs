//! PNG 纹理加载与格式校验。
//!
//! 运行时统一使用 RGBA8：调色板与灰度图片会转换为 RGBA8，16 位深度的
//! 图片会被明确拒绝，避免“看起来能加载但实际规格不一致”的隐性问题。

use std::fmt;
use std::fs::File;
use std::path::{Path, PathBuf};

use super::types::TextureImage;

/// 基础纹理边长（像素）；占位纹理使用该尺寸。
pub const BASE_TEXTURE_SIZE: u32 = 16;

/// 允许导入的正方形纹理边长；保持 2 的幂并限制上限，避免图集无节制增长。
pub const ALLOWED_TEXTURE_SIZES: [u32; 4] = [BASE_TEXTURE_SIZE, 32, 64, 128];

/// 判断宽高是否为支持的纹理尺寸：必须正方形且属于允许列表。
pub const fn is_supported_texture_size(width: u32, height: u32) -> bool {
    if width != height {
        return false;
    }
    let mut index = 0;
    while index < ALLOWED_TEXTURE_SIZES.len() {
        if ALLOWED_TEXTURE_SIZES[index] == width {
            return true;
        }
        index += 1;
    }
    false
}

/// 支持的尺寸列表文本，用于错误提示。
pub const SUPPORTED_SIZES_TEXT: &str = "16×16、32×32、64×64 或 128×128";

/// 纹理加载失败的具体原因；每条错误都携带可定位的资源路径。
#[derive(Debug)]
pub enum TextureLoadError {
    /// 文件不存在。
    NotFound { path: PathBuf },
    /// 读取或解码失败（文件损坏、权限不足等）。
    Io { path: PathBuf, message: String },
    /// 图片尺寸不符合规范。
    InvalidSize {
        path: PathBuf,
        width: u32,
        height: u32,
    },
    /// 像素格式无法转换为 RGBA8。
    UnsupportedFormat { path: PathBuf, message: String },
}

impl fmt::Display for TextureLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextureLoadError::NotFound { path } => {
                write!(formatter, "纹理文件不存在: {}", path.display())
            }
            TextureLoadError::Io { path, message } => {
                write!(
                    formatter,
                    "纹理文件读取或解码失败: {} ({message})",
                    path.display()
                )
            }
            TextureLoadError::InvalidSize {
                path,
                width,
                height,
            } => write!(
                formatter,
                "纹理尺寸不符合规范: {} 为 {width}×{height}，要求 {SUPPORTED_SIZES_TEXT} 的正方形",
                path.display()
            ),
            TextureLoadError::UnsupportedFormat { path, message } => {
                write!(
                    formatter,
                    "纹理像素格式无法转换为 RGBA8: {} ({message})",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for TextureLoadError {}

/// 加载并校验一张纹理 PNG：支持 16/32/64/128 的正方形。
pub fn load_texture_png(path: &Path) -> Result<TextureImage, TextureLoadError> {
    if !path.is_file() {
        return Err(TextureLoadError::NotFound {
            path: path.to_path_buf(),
        });
    }

    let file = File::open(path).map_err(|error| TextureLoadError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;

    let mut decoder = png::Decoder::new(file);
    // 展开调色板、低位深灰度与 tRNS 透明块，统一得到 8 位通道数据。
    decoder.set_transformations(png::Transformations::EXPAND);

    let mut reader = decoder.read_info().map_err(|error| TextureLoadError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;

    let (width, height) = {
        let info = reader.info();
        (info.width, info.height)
    };
    if !is_supported_texture_size(width, height) {
        return Err(TextureLoadError::InvalidSize {
            path: path.to_path_buf(),
            width,
            height,
        });
    }

    let mut buffer = vec![0; reader.output_buffer_size()];
    let frame = reader
        .next_frame(&mut buffer)
        .map_err(|error| TextureLoadError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;

    if frame.bit_depth != png::BitDepth::Eight {
        return Err(TextureLoadError::UnsupportedFormat {
            path: path.to_path_buf(),
            message: format!("位深为 {:?}，运行时要求 8 位", frame.bit_depth),
        });
    }

    let rgba = to_rgba8(&buffer[..frame.buffer_size()], frame.color_type).ok_or_else(|| {
        TextureLoadError::UnsupportedFormat {
            path: path.to_path_buf(),
            message: format!("不支持的颜色类型 {:?}", frame.color_type),
        }
    })?;

    TextureImage::new(width, height, rgba).ok_or_else(|| TextureLoadError::UnsupportedFormat {
        path: path.to_path_buf(),
        message: "像素缓冲区长度与图片尺寸不一致".to_string(),
    })
}

/// 把 PNG 解码出的各颜色类型统一转换为 RGBA8。
fn to_rgba8(pixels: &[u8], color_type: png::ColorType) -> Option<Vec<u8>> {
    let channels = match color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Indexed => 1,
    };
    if !pixels.len().is_multiple_of(channels) {
        return None;
    }

    let count = pixels.len() / channels;
    let mut output = Vec::with_capacity(count * 4);
    for chunk in pixels.chunks_exact(channels) {
        match color_type {
            png::ColorType::Rgba => output.extend_from_slice(chunk),
            png::ColorType::Rgb => {
                output.extend_from_slice(chunk);
                output.push(255);
            }
            png::ColorType::Grayscale => {
                output.extend_from_slice(&[chunk[0], chunk[0], chunk[0], 255]);
            }
            png::ColorType::GrayscaleAlpha => {
                output.extend_from_slice(&[chunk[0], chunk[0], chunk[0], chunk[1]]);
            }
            // EXPAND 之后不应再出现索引色；出现时只能按失败处理。
            png::ColorType::Indexed => return None,
        }
    }
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// 在系统临时目录下创建唯一测试目录。
    fn unique_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "neverland-rhapsody-texture-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("创建测试目录");
        dir
    }

    /// 写入一张 RGBA PNG 供加载测试使用。
    fn write_png(path: &Path, width: u32, height: u32, pixels: &[u8]) {
        let file = File::create(path).expect("创建 PNG");
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("写入 PNG 头");
        writer.write_image_data(pixels).expect("写入 PNG 数据");
    }

    #[test]
    fn rgba_png_loads_with_alpha_channel() {
        let dir = unique_dir("load");
        let path = dir.join("sample.png");
        let mut pixels = vec![255u8; 16 * 16 * 4];
        pixels[..4].copy_from_slice(&[10, 20, 30, 128]);
        write_png(&path, 16, 16, &pixels);

        let image = load_texture_png(&path).expect("加载成功");
        assert_eq!(image.width, 16);
        assert_eq!(image.height, 16);
        assert_eq!(image.pixel(0, 0), Some([10, 20, 30, 128]));
        assert_eq!(image.pixel(1, 0), Some([255, 255, 255, 255]));
        assert!(image.has_alpha());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn larger_square_textures_are_accepted() {
        let dir = unique_dir("large");
        let path = dir.join("large.png");
        write_png(&path, 64, 64, &[200; 64 * 64 * 4]);

        let image = load_texture_png(&path).expect("64×64 应被接受");
        assert_eq!((image.width, image.height), (64, 64));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_files_report_their_path() {
        let path = PathBuf::from("assets/textures/blocks/does_not_exist.png");
        let error = load_texture_png(&path).expect_err("文件不存在应报错");
        match error {
            TextureLoadError::NotFound { path: reported } => assert_eq!(reported, path),
            other => panic!("错误类型不符: {other}"),
        }
    }

    #[test]
    fn corrupt_files_report_a_decode_error() {
        let dir = unique_dir("corrupt");
        let path = dir.join("broken.png");
        fs::write(&path, b"not a png").expect("写入损坏文件");

        let error = load_texture_png(&path).expect_err("损坏文件应报错");
        assert!(matches!(error, TextureLoadError::Io { .. }));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn wrong_sizes_are_rejected_with_explicit_bounds() {
        let dir = unique_dir("size");
        let path = dir.join("wrong.png");
        write_png(&path, 8, 8, &[255; 8 * 8 * 4]);

        let error = load_texture_png(&path).expect_err("8×8 应被拒绝");
        match error {
            TextureLoadError::InvalidSize { width, height, .. } => {
                assert_eq!((width, height), (8, 8));
            }
            other => panic!("错误类型不符: {other}"),
        }

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn non_square_textures_are_rejected() {
        let dir = unique_dir("rect");
        let path = dir.join("rect.png");
        write_png(&path, 32, 16, &[255; 32 * 16 * 4]);

        let error = load_texture_png(&path).expect_err("非正方形应被拒绝");
        assert!(matches!(error, TextureLoadError::InvalidSize { .. }));

        fs::remove_dir_all(&dir).ok();
    }
}
