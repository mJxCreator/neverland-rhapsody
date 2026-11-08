//! GLB 容器解析与 glTF 2.0 JSON 数据结构。
//!
//! 只做容器与 JSON 层校验，不负责资源转换；转换逻辑在 [`super::loader`]。
//! 支持内嵌 BIN 块、外部 `.bin` 与外部 PNG，拒绝稀疏 Accessor 等未实现特性。

use std::path::Path;

use serde::Deserialize;

use super::ModelLoadError;

const GLB_MAGIC: &[u8; 4] = b"glTF";
const CHUNK_JSON: u32 = 0x4E4F_534A;
const CHUNK_BIN: u32 = 0x004E_4942;

/// 解析结果：glTF 文档与内嵌二进制块（可能为空）。
#[derive(Debug)]
pub struct GltfDocument {
    pub gltf: Gltf,
    pub bin: Vec<u8>,
    pub base_dir: std::path::PathBuf,
}

/// 解析 GLB 字节；`base_dir` 用于解析外部 buffer/image 的相对路径。
pub fn parse_glb(bytes: &[u8], base_dir: &Path) -> Result<GltfDocument, ModelLoadError> {
    if bytes.len() < 12 {
        return Err(ModelLoadError::format("GLB 文件长度不足 12 字节"));
    }
    if &bytes[0..4] != GLB_MAGIC {
        return Err(ModelLoadError::format("GLB 文件缺少 glTF 魔数"));
    }
    let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if version != 2 {
        return Err(ModelLoadError::unsupported(format!(
            "GLB 版本为 {version}，仅支持 glTF 2.0"
        )));
    }
    let declared_length = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    if declared_length > bytes.len() || declared_length < 12 {
        return Err(ModelLoadError::format(format!(
            "GLB 声明长度 {declared_length} 与实际长度 {} 不一致",
            bytes.len()
        )));
    }

    let mut offset = 12usize;
    let mut json: Option<Gltf> = None;
    let mut bin = Vec::new();
    while offset + 8 <= declared_length {
        let length = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        let kind = u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]);
        let data_start = offset + 8;
        let data_end = data_start
            .checked_add(length)
            .ok_or_else(|| ModelLoadError::format("GLB 数据块长度溢出"))?;
        if data_end > declared_length {
            return Err(ModelLoadError::format("GLB 数据块超出文件长度"));
        }
        let data = &bytes[data_start..data_end];
        match kind {
            CHUNK_JSON if json.is_none() => {
                let text = std::str::from_utf8(data)
                    .map_err(|error| ModelLoadError::format(format!("JSON 块编码错误: {error}")))?;
                let parsed: Gltf = serde_json::from_str(text).map_err(|error| {
                    ModelLoadError::format(format!("glTF JSON 解析失败: {error}"))
                })?;
                json = Some(parsed);
            }
            CHUNK_JSON => {
                // 规范允许多个 JSON 块，但运行时只需要第一个。
            }
            CHUNK_BIN if bin.is_empty() => bin.extend_from_slice(data),
            _ => {}
        }
        offset = data_end;
    }

    let gltf = json.ok_or_else(|| ModelLoadError::format("GLB 缺少 JSON 块"))?;
    Ok(GltfDocument {
        gltf,
        bin,
        base_dir: base_dir.to_path_buf(),
    })
}

/// glTF 2.0 文档（仅包含运行时需要的字段）。
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gltf {
    pub asset: Option<GltfAsset>,
    pub scene: Option<usize>,
    pub scenes: Option<Vec<GltfScene>>,
    pub nodes: Option<Vec<GltfNode>>,
    pub meshes: Option<Vec<GltfMesh>>,
    pub accessors: Option<Vec<GltfAccessor>>,
    pub buffer_views: Option<Vec<GltfBufferView>>,
    pub buffers: Option<Vec<GltfBuffer>>,
    pub materials: Option<Vec<GltfMaterial>>,
    pub textures: Option<Vec<GltfTexture>>,
    pub images: Option<Vec<GltfImage>>,
    pub skins: Option<Vec<GltfSkin>>,
    pub animations: Option<Vec<GltfAnimation>>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct GltfAsset {
    pub version: Option<String>,
    pub generator: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct GltfScene {
    pub nodes: Option<Vec<usize>>,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GltfNode {
    pub name: Option<String>,
    pub children: Option<Vec<usize>>,
    pub mesh: Option<usize>,
    pub skin: Option<usize>,
    pub translation: Option<[f32; 3]>,
    pub rotation: Option<[f32; 4]>,
    pub scale: Option<[f32; 3]>,
    pub matrix: Option<[f32; 16]>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct GltfMesh {
    pub name: Option<String>,
    pub primitives: Vec<GltfPrimitive>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GltfPrimitive {
    pub attributes: std::collections::BTreeMap<String, usize>,
    pub indices: Option<usize>,
    pub material: Option<usize>,
    pub mode: Option<u32>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GltfAccessor {
    pub buffer_view: Option<usize>,
    pub byte_offset: Option<usize>,
    pub component_type: u32,
    pub normalized: Option<bool>,
    pub count: usize,
    #[serde(rename = "type")]
    pub kind: String,
    pub min: Option<Vec<f32>>,
    pub max: Option<Vec<f32>>,
    pub sparse: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GltfBufferView {
    pub buffer: usize,
    pub byte_offset: Option<usize>,
    pub byte_length: usize,
    pub byte_stride: Option<usize>,
    pub target: Option<u32>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GltfBuffer {
    pub uri: Option<String>,
    pub byte_length: usize,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GltfMaterial {
    pub name: Option<String>,
    pub pbr_metallic_roughness: Option<GltfPbr>,
    pub alpha_mode: Option<String>,
    pub alpha_cutoff: Option<f32>,
    pub double_sided: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GltfPbr {
    pub base_color_factor: Option<[f32; 4]>,
    pub base_color_texture: Option<GltfTextureRef>,
    pub metallic_factor: Option<f32>,
    pub roughness_factor: Option<f32>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct GltfTextureRef {
    pub index: usize,
    #[serde(rename = "texCoord")]
    pub tex_coord: Option<u32>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct GltfTexture {
    pub source: Option<usize>,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GltfImage {
    pub uri: Option<String>,
    pub buffer_view: Option<usize>,
    pub mime_type: Option<String>,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GltfSkin {
    pub joints: Vec<usize>,
    pub inverse_bind_matrices: Option<usize>,
    pub skeleton: Option<usize>,
    pub name: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct GltfAnimation {
    pub name: Option<String>,
    pub channels: Vec<GltfAnimationChannel>,
    pub samplers: Vec<GltfAnimationSampler>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct GltfAnimationChannel {
    pub sampler: usize,
    pub target: GltfAnimationTarget,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct GltfAnimationTarget {
    pub node: Option<usize>,
    pub path: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct GltfAnimationSampler {
    pub input: usize,
    pub output: usize,
    pub interpolation: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_glb(json: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        let json_bytes = json.as_bytes();
        let json_padded = (json_bytes.len() + 3) & !3;
        let total = 12 + 8 + json_padded;
        bytes.extend_from_slice(GLB_MAGIC);
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&(total as u32).to_le_bytes());
        bytes.extend_from_slice(&(json_padded as u32).to_le_bytes());
        bytes.extend_from_slice(&CHUNK_JSON.to_le_bytes());
        bytes.extend_from_slice(json_bytes);
        bytes.resize(total, b' ');
        bytes
    }

    #[test]
    fn parses_minimal_glb() {
        let bytes = minimal_glb(r#"{"asset":{"version":"2.0"},"nodes":[]}"#);
        let doc = parse_glb(&bytes, Path::new(".")).expect("应解析成功");
        assert_eq!(
            doc.gltf.asset.expect("asset").version.as_deref(),
            Some("2.0")
        );
        assert!(doc.bin.is_empty());
    }

    #[test]
    fn rejects_bad_magic_and_version() {
        let mut bytes = minimal_glb("{}");
        bytes[0] = b'X';
        assert!(parse_glb(&bytes, Path::new(".")).is_err());
        let mut bytes = minimal_glb("{}");
        bytes[4..8].copy_from_slice(&1u32.to_le_bytes());
        assert!(parse_glb(&bytes, Path::new(".")).is_err());
    }

    #[test]
    fn rejects_missing_json_chunk() {
        let bytes = minimal_glb("{}");
        let truncated = &bytes[..12];
        assert!(parse_glb(truncated, Path::new(".")).is_err());
    }
}
