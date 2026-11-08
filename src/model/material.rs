//! 模型材质：基础颜色、基础颜色纹理、金属度/粗糙度参数与透明模式。
//!
//! 本版本采用基础光照，金属度与粗糙度只作为参数保留，不参与完整 PBR 计算；
//! 渲染重点是基础颜色、UV 纹理、Alpha Mask 与 Alpha Blend。

/// 透明模式；决定渲染管线与是否丢弃低 Alpha 像素。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AlphaMode {
    /// 完全不透明。
    Opaque,
    /// Alpha 低于阈值时直接丢弃像素。
    Mask(f32),
    /// 按 Alpha 混合。
    Blend,
}

impl AlphaMode {
    /// glTF `alphaMode` 字符串解析；未知值按不透明处理并返回 `false`。
    pub fn from_gltf(value: &str, cutoff: Option<f32>) -> (Self, bool) {
        match value {
            "OPAQUE" => (Self::Opaque, true),
            "MASK" => (Self::Mask(cutoff.unwrap_or(0.5).clamp(0.0, 1.0)), true),
            "BLEND" => (Self::Blend, true),
            _ => (Self::Opaque, false),
        }
    }

    /// 是否需要 Alpha 混合管线。
    pub fn is_blend(self) -> bool {
        matches!(self, Self::Blend)
    }

    /// 是否需要 Alpha Mask 管线。
    pub fn is_mask(self) -> bool {
        matches!(self, Self::Mask(_))
    }

    /// Mask 模式的丢弃阈值；其他模式返回 0。
    pub fn cutoff(self) -> f32 {
        match self {
            Self::Mask(cutoff) => cutoff,
            _ => 0.0,
        }
    }
}

/// 运行时模型材质。
#[derive(Clone, Debug, PartialEq)]
pub struct ModelMaterial {
    pub name: String,
    /// 基础颜色（RGBA），与纹理采样结果相乘。
    pub base_color: [f32; 4],
    /// 基础颜色纹理在 `ModelAsset::textures` 中的下标。
    pub base_color_texture: Option<usize>,
    pub metallic: f32,
    pub roughness: f32,
    pub alpha_mode: AlphaMode,
    pub double_sided: bool,
    /// 是否使用了占位材质（纹理缺失或引用无效）。
    pub placeholder: bool,
}

impl Default for ModelMaterial {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            base_color: [1.0, 1.0, 1.0, 1.0],
            base_color_texture: None,
            metallic: 0.0,
            roughness: 1.0,
            alpha_mode: AlphaMode::Opaque,
            double_sided: false,
            placeholder: true,
        }
    }
}

impl ModelMaterial {
    /// 用于程序化模型的纯色材质；带纹理的模型不标记为占位。
    pub fn colored(name: &str, color: [f32; 4]) -> Self {
        Self {
            name: name.to_string(),
            base_color: color,
            placeholder: false,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gltf_alpha_modes_parse_with_cutoff() {
        assert_eq!(
            AlphaMode::from_gltf("OPAQUE", None),
            (AlphaMode::Opaque, true)
        );
        assert_eq!(
            AlphaMode::from_gltf("MASK", Some(0.3)),
            (AlphaMode::Mask(0.3), true)
        );
        assert_eq!(
            AlphaMode::from_gltf("BLEND", None),
            (AlphaMode::Blend, true)
        );
        let (mode, known) = AlphaMode::from_gltf("UNKNOWN", None);
        assert_eq!(mode, AlphaMode::Opaque);
        assert!(!known);
    }

    #[test]
    fn default_material_is_opaque_placeholder() {
        let material = ModelMaterial::default();
        assert!(material.placeholder);
        assert!(!material.alpha_mode.is_blend());
        assert_eq!(material.base_color, [1.0; 4]);
    }
}
