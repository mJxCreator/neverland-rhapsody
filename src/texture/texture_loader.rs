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

/// 允许导入的纹理边长；宽高各自须属于该列表，因此允许非正方形 UI 面板。
pub const ALLOWED_TEXTURE_SIZES: [u32; 7] = [BASE_TEXTURE_SIZE, 32, 64, 128, 256, 512, 1024];

/// 判断宽高是否为支持的纹理尺寸：宽与高各自属于允许列表，不要求相等。
pub const fn is_supported_texture_size(width: u32, height: u32) -> bool {
    is_allowed_side(width) && is_allowed_side(height)
}

const fn is_allowed_side(side: u32) -> bool {
    let mut index = 0;
    while index < ALLOWED_TEXTURE_SIZES.len() {
        if ALLOWED_TEXTURE_SIZES[index] == side {
            return true;
        }
        index += 1;
    }
    false
}

/// 支持的尺寸列表文本，用于错误提示。
pub const SUPPORTED_SIZES_TEXT: &str = "16/32/64/128/256/512/1024 像素（宽高可不同）";

/// UI 控件允许的最大单边边长；UI 图不做 2 的幂限制，任意尺寸都可直接导入。
pub const MAX_UI_TEXTURE_SIZE: u32 = 2048;

/// UI 纹理尺寸说明，用于错误提示。
pub const SUPPORTED_UI_SIZES_TEXT: &str = "1–2048 像素（宽高可不同）";

/// UI 纹理尺寸校验：任意正整数宽高，只要不超过 [`MAX_UI_TEXTURE_SIZE`]。
pub const fn is_supported_ui_texture_size(width: u32, height: u32) -> bool {
    width >= 1 && height >= 1 && width <= MAX_UI_TEXTURE_SIZE && height <= MAX_UI_TEXTURE_SIZE
}

/// 模型纹理允许的最大单边边长；模型 UV 纹理可以是任意尺寸。
pub const MAX_MODEL_TEXTURE_SIZE: u32 = 4096;

/// 模型纹理尺寸说明，用于错误提示。
pub const SUPPORTED_MODEL_SIZES_TEXT: &str = "1–4096 像素（宽高可不同）";

/// 模型纹理尺寸校验：任意正整数宽高，只要不超过 [`MAX_MODEL_TEXTURE_SIZE`]。
pub const fn is_supported_model_texture_size(width: u32, height: u32) -> bool {
    width >= 1 && height >= 1 && width <= MAX_MODEL_TEXTURE_SIZE && height <= MAX_MODEL_TEXTURE_SIZE
}

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
        /// 该类纹理允许的尺寸说明。
        requirement: &'static str,
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
                requirement,
            } => write!(
                formatter,
                "纹理尺寸不符合规范: {} 为 {width}×{height}，要求 {requirement}",
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

/// 加载并校验一张方块/物品纹理 PNG：宽高各自支持 16/32/64/128/256/512/1024。
pub fn load_texture_png(path: &Path) -> Result<TextureImage, TextureLoadError> {
    load_texture_png_with(path, is_supported_texture_size, SUPPORTED_SIZES_TEXT)
}

/// 加载并校验一张 UI 控件 PNG：宽高各自允许 1–2048 像素的任意尺寸。
pub fn load_ui_texture_png(path: &Path) -> Result<TextureImage, TextureLoadError> {
    load_texture_png_with(path, is_supported_ui_texture_size, SUPPORTED_UI_SIZES_TEXT)
}

fn load_texture_png_with(
    path: &Path,
    supported: fn(u32, u32) -> bool,
    requirement: &'static str,
) -> Result<TextureImage, TextureLoadError> {
    if !path.is_file() {
        return Err(TextureLoadError::NotFound {
            path: path.to_path_buf(),
        });
    }

    let file = File::open(path).map_err(|error| TextureLoadError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    decode_png_reader(file, path, supported, requirement)
}

/// 解码模型内嵌或外部引用的 PNG 字节；允许 1–4096 像素的任意尺寸。
pub fn decode_png_bytes(bytes: &[u8], name: &str) -> Result<TextureImage, TextureLoadError> {
    let path = PathBuf::from(format!("<embedded:{name}>"));
    decode_png_reader(
        std::io::Cursor::new(bytes),
        &path,
        is_supported_model_texture_size,
        SUPPORTED_MODEL_SIZES_TEXT,
    )
}

fn decode_png_reader<R: std::io::Read>(
    source: R,
    path: &Path,
    supported: fn(u32, u32) -> bool,
    requirement: &'static str,
) -> Result<TextureImage, TextureLoadError> {
    let mut decoder = png::Decoder::new(source);
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
    if !supported(width, height) {
        return Err(TextureLoadError::InvalidSize {
            path: path.to_path_buf(),
            width,
            height,
            requirement,
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
    fn non_square_textures_are_accepted() {
        let dir = unique_dir("rect");
        let path = dir.join("rect.png");
        write_png(&path, 1024, 256, &[255; 1024 * 256 * 4]);

        let image = load_texture_png(&path).expect("非正方形应被接受");
        assert_eq!((image.width, image.height), (1024, 256));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unsupported_sides_are_rejected() {
        let dir = unique_dir("bad-side");
        let path = dir.join("bad.png");
        write_png(&path, 24, 64, &[255; 24 * 64 * 4]);

        let error = load_texture_png(&path).expect_err("24 像素宽应被拒绝");
        assert!(matches!(error, TextureLoadError::InvalidSize { .. }));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ui_textures_accept_arbitrary_sizes_within_the_limit() {
        let dir = unique_dir("ui-size");
        let path = dir.join("hotbar_bg.png");
        write_png(&path, 1040, 80, &[255; 1040 * 80 * 4]);

        let image = load_ui_texture_png(&path).expect("UI 图应接受 1040×80");
        assert_eq!((image.width, image.height), (1040, 80));

        let too_wide = dir.join("too_wide.png");
        let width = MAX_UI_TEXTURE_SIZE + 1;
        write_png(&too_wide, width, 16, &vec![255; width as usize * 16 * 4]);
        let error = load_ui_texture_png(&too_wide).expect_err("超过上限应被拒绝");
        assert!(matches!(error, TextureLoadError::InvalidSize { .. }));

        fs::remove_dir_all(&dir).ok();
    }
}
